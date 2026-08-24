/* Minimal async HTTP over the connected STA link, using a raw lwip socket
 * (HTTP/1.0, http:// only). This is deliberately NOT esp_http_client: that
 * component pulls esp-tls/mbedtls into the image, which the ~330 KB of free app
 * partition cannot afford. Any method (GET/POST/PUT/DELETE/HEAD/…) with an
 * optional body is supported. The request runs on its own FreeRTOS task so the
 * LVGL task never blocks; the result is parked in shared state and consumed
 * once by the app host (micro_http_take_result), mirroring the scan bridge.
 *
 * Threading: the shared s_method / s_url / s_body / s_busy / s_ready / s_result
 * are written by the http task (or micro_http_request on the caller task) and
 * read by the LVGL task, so every access is under s_lock. One request at a
 * time; a second request while one is in flight is rejected.
 */

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/time.h>

#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "lwip/err.h"
#include "lwip/netdb.h"
#include "lwip/sockets.h"

#include "micro_http.h"

static const char *TAG = "micro_http";

#define MICRO_HTTP_METHOD_CAP 16U
#define MICRO_HTTP_URL_CAP 128U
#define MICRO_HTTP_BODY_CAP 256U
#define MICRO_HTTP_RESULT_CAP 512U
#define MICRO_HTTP_RAW_CAP 1024U
#define MICRO_HTTP_TASK_STACK 8192U
#define MICRO_HTTP_TIMEOUT_MS 8000U

static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static char s_method[MICRO_HTTP_METHOD_CAP];
static char s_url[MICRO_HTTP_URL_CAP];
static char s_body[MICRO_HTTP_BODY_CAP];
static bool s_busy;
static bool s_ready;
static char s_result[MICRO_HTTP_RESULT_CAP];

/* Split "http://host[:port][/path]" into host / port / path. Returns -1 on a
 * malformed URL; on success host and path are NUL-terminated in the buffers. */
static int parse_url(const char *url, char *host, size_t host_cap,
                     int *port, char *path, size_t path_cap)
{
    const char *p = url;
    if (strncmp(p, "http://", 7) == 0) {
        p += 7;
    }
    const char *slash = strchr(p, '/');
    const char *host_end = slash != NULL ? slash : p + strlen(p);
    size_t host_len = (size_t)(host_end - p);
    if (host_len == 0 || host_len >= host_cap) {
        return -1;
    }
    memcpy(host, p, host_len);
    host[host_len] = '\0';

    *port = 80;
    char *colon = strchr(host, ':');
    if (colon != NULL) {
        *colon = '\0';
        char *end = NULL;
        long parsed = strtol(colon + 1, &end, 10);
        if (end == colon + 1 || *end != '\0' || parsed <= 0 || parsed > 65535) {
            return -1;
        }
        *port = (int)parsed;
    }

    if (slash != NULL) {
        strncpy(path, slash, path_cap - 1);
        path[path_cap - 1] = '\0';
    } else {
        strcpy(path, "/");
    }
    return 0;
}

static void micro_http_task(void *arg)
{
    (void)arg;
    char method[MICRO_HTTP_METHOD_CAP];
    char url[MICRO_HTTP_URL_CAP];
    char body[MICRO_HTTP_BODY_CAP];
    portENTER_CRITICAL(&s_lock);
    memcpy(method, s_method, sizeof method);
    memcpy(url, s_url, sizeof url);
    memcpy(body, s_body, sizeof body);
    portEXIT_CRITICAL(&s_lock);

    char result[MICRO_HTTP_RESULT_CAP];
    size_t used = 0;

    /* Reject anything that is not an uppercase token: blocks header injection
     * through the method string while allowing GET/POST/PUT/DELETE/HEAD/… */
    size_t method_len = strlen(method);
    if (method_len == 0 ||
        strspn(method, "ABCDEFGHIJKLMNOPQRSTUVWXYZ") != method_len) {
        used = (size_t)snprintf(result, sizeof result, "HTTP 0\nbad method: %s", method);
        goto done;
    }
    size_t body_len = strlen(body);

    char host[64];
    char path[96];
    int port = 80;
    if (parse_url(url, host, sizeof host, &port, path, sizeof path) != 0) {
        used = (size_t)snprintf(result, sizeof result, "HTTP 0\nbad URL: %s", url);
        goto done;
    }

    struct hostent *entry = gethostbyname(host);
    if (entry == NULL) {
        used = (size_t)snprintf(result, sizeof result, "HTTP 0\nDNS failed for %s", host);
        goto done;
    }

    int fd = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (fd < 0) {
        used = (size_t)snprintf(result, sizeof result, "HTTP 0\nsocket failed");
        goto done;
    }
    struct timeval timeout = {.tv_sec = MICRO_HTTP_TIMEOUT_MS / 1000,
                              .tv_usec = (MICRO_HTTP_TIMEOUT_MS % 1000) * 1000};
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof timeout);

    struct sockaddr_in addr = {0};
    addr.sin_family = AF_INET;
    addr.sin_port = htons((u16_t)port);
    memcpy(&addr.sin_addr, entry->h_addr_list[0], entry->h_length);
    if (connect(fd, (struct sockaddr *)&addr, sizeof addr) != 0) {
        used = (size_t)snprintf(result, sizeof result,
                                "HTTP 0\nconnect %s:%d failed", host, port);
        close(fd);
        goto done;
    }

    /* Send the request head (method line + Host + optional Content-Length), then
     * the body separately so it never overflows a fixed request buffer. */
    char content_length[24] = "";
    if (body_len > 0) {
        snprintf(content_length, sizeof content_length, "Content-Length: %zu\r\n", body_len);
    }
    char head[256];
    int head_len = snprintf(head, sizeof head,
                            "%s %s HTTP/1.0\r\nHost: %s\r\n%s"
                            "Connection: close\r\n\r\n",
                            method, path, host, content_length);
    if (head_len <= 0 || send(fd, head, (size_t)head_len, 0) != head_len ||
        (body_len > 0 && send(fd, body, body_len, 0) != (ssize_t)body_len)) {
        used = (size_t)snprintf(result, sizeof result, "HTTP 0\nsend failed");
        close(fd);
        goto done;
    }

    /* Read the whole (bounded) response, then split status line / body. */
    char raw[MICRO_HTTP_RAW_CAP];
    size_t raw_used = 0;
    ssize_t got;
    while (raw_used + 1 < sizeof raw &&
           (got = recv(fd, raw + raw_used, sizeof raw - raw_used - 1, 0)) > 0) {
        raw_used += (size_t)got;
    }
    raw[raw_used] = '\0';
    close(fd);

    if (raw_used == 0) {
        used = (size_t)snprintf(result, sizeof result, "HTTP 0\nno response");
        goto done;
    }
    int status = 0;
    if (sscanf(raw, "HTTP/%*d.%*d %d", &status) != 1) {
        status = 0;
    }
    const char *body_out = strstr(raw, "\r\n\r\n");
    if (body_out == NULL) {
        body_out = strstr(raw, "\n\n");
        body_out = body_out != NULL ? body_out + 2 : NULL;
    } else {
        body_out += 4;
    }
    if (body_out == NULL) {
        body_out = raw;
    }
    used = (size_t)snprintf(result, sizeof result, "HTTP %d\n%s", status, body_out);
    if (used >= sizeof result) {
        used = sizeof result - 1;
    }

done:
    portENTER_CRITICAL(&s_lock);
    memcpy(s_result, result, used);
    s_result[used] = '\0';
    s_ready = true;
    s_busy = false;
    portEXIT_CRITICAL(&s_lock);
    vTaskDelete(NULL);
}

static int spawn_request(const char *method, size_t method_len,
                         const char *url, size_t url_len,
                         const char *body, size_t body_len)
{
    if (method == NULL || method_len == 0 || method_len >= MICRO_HTTP_METHOD_CAP ||
        url == NULL || url_len == 0 || url_len >= MICRO_HTTP_URL_CAP ||
        (body_len > 0 && (body == NULL || body_len >= MICRO_HTTP_BODY_CAP))) {
        return -1;
    }
    portENTER_CRITICAL(&s_lock);
    bool busy = s_busy;
    if (!busy) {
        memcpy(s_method, method, method_len);
        s_method[method_len] = '\0';
        memcpy(s_url, url, url_len);
        s_url[url_len] = '\0';
        if (body_len > 0) {
            memcpy(s_body, body, body_len);
        }
        s_body[body_len] = '\0';
        s_busy = true;
    }
    portEXIT_CRITICAL(&s_lock);
    if (busy) {
        ESP_LOGW(TAG, "request already in flight; rejected %s", url);
        return -1;
    }
    if (xTaskCreate(micro_http_task, "micro_http", MICRO_HTTP_TASK_STACK, NULL, 5,
                    NULL) != pdPASS) {
        portENTER_CRITICAL(&s_lock);
        s_busy = false;
        portEXIT_CRITICAL(&s_lock);
        ESP_LOGE(TAG, "cannot spawn http task");
        return -1;
    }
    return 0;
}

int micro_http_get(const char *url, size_t len)
{
    return spawn_request("GET", 3, url, len, "", 0);
}

int micro_http_request(const char *method, size_t method_len,
                       const char *url, size_t url_len,
                       const char *body, size_t body_len)
{
    return spawn_request(method, method_len, url, url_len, body, body_len);
}

int micro_http_take_result(char *buf, size_t cap)
{
    if (buf == NULL || cap == 0) {
        return -1;
    }
    buf[0] = '\0';
    portENTER_CRITICAL(&s_lock);
    bool fresh = s_ready;
    if (fresh) {
        s_ready = false;
        strncpy(buf, s_result, cap - 1);
        buf[cap - 1] = '\0';
    }
    portEXIT_CRITICAL(&s_lock);
    return fresh ? 1 : 0;
}
