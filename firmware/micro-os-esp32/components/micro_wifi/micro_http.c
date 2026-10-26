/* Blocking HTTP/HTTPS client for the App `net.*` host calls.
 *
 * This uses IDF's esp_http_client instead of a raw lwip socket:
 *   - HTTPS with the configured mbedTLS certificate bundle,
 *   - arbitrary request headers (Authorization, Accept, Content-Type, ...),
 *   - redirects and chunked transfer encoding handled by IDF,
 *   - 4xx/5xx response bodies preserved for the App's error UI.
 *
 * One request runs synchronously on the caller's worker task. The Rust host
 * owns the async boundary (`std::thread` + mpsc), so this file has no shared
 * global state and no request slot to go stale after a failed handshake.
 *
 * Memory: the response is heap-allocated and returned to Rust, which copies it
 * into a String and calls micro_http_free(). Request buffers are borrowed from
 * the caller and stay valid for the duration of the call.
 */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#include "esp_crt_bundle.h"
#include "esp_err.h"
#include "esp_http_client.h"
#include "esp_log.h"

#include "micro_http.h"

static const char *TAG = "micro_http";

#define MICRO_HTTP_MAX_METHOD 16U
#define MICRO_HTTP_MAX_URL 4096U
#define MICRO_HTTP_MAX_HEADER_NAME 64U
#define MICRO_HTTP_MAX_HEADER_VALUE 1024U
#define MICRO_HTTP_MAX_BODY (64U * 1024U)
#define MICRO_HTTP_MAX_RESPONSE (96U * 1024U)
#define MICRO_HTTP_TIMEOUT_MS 20000

/* Builds an error body and installs it as the response. Returns 0 so callers
 * can `return failf(...)` directly, or -1 when even the message cannot be
 * allocated. */
static int failf(int *status, char **response, size_t *response_len,
                 const char *format, ...)
{
    char stack[256];
    va_list args;
    va_list copy;
    va_start(args, format);
    va_copy(copy, args);
    int written = vsnprintf(stack, sizeof stack, format, args);
    va_end(args);

    char *message = NULL;
    if (written >= 0 && (size_t)written < sizeof stack) {
        message = strdup(stack);
    } else if (written >= 0) {
        message = malloc((size_t)written + 1);
        if (message != NULL) {
            vsnprintf(message, (size_t)written + 1, format, copy);
        }
    }
    va_end(copy);

    if (message == NULL) {
        *status = 0;
        *response = NULL;
        *response_len = 0;
        return -1;
    }
    *status = 0;
    *response = message;
    *response_len = strlen(message);
    return 0;
}

static bool token_byte(unsigned char byte)
{
    return (byte >= '0' && byte <= '9') || (byte >= 'A' && byte <= 'Z') ||
           (byte >= 'a' && byte <= 'z') ||
           (byte != 0 && strchr("!#$%&'*+-.^_`|~", (int)byte) != NULL);
}

static bool managed_header(const char *name)
{
    static const char *const managed[] = {
        "host", "content-length", "connection", "transfer-encoding",
        "expect", "upgrade", "proxy-connection", "te", "trailer",
    };
    for (size_t i = 0; i < sizeof managed / sizeof managed[0]; ++i) {
        if (strcasecmp(name, managed[i]) == 0) {
            return true;
        }
    }
    return false;
}

static bool method_from_token(const char *method, size_t len,
                              esp_http_client_method_t *out)
{
#define MICRO_METHOD(name, value)                                            \
    if (len == sizeof(name) - 1 && memcmp(method, name, len) == 0) {         \
        *out = value;                                                        \
        return true;                                                         \
    }
    MICRO_METHOD("GET", HTTP_METHOD_GET)
    MICRO_METHOD("POST", HTTP_METHOD_POST)
    MICRO_METHOD("PUT", HTTP_METHOD_PUT)
    MICRO_METHOD("PATCH", HTTP_METHOD_PATCH)
    MICRO_METHOD("DELETE", HTTP_METHOD_DELETE)
    MICRO_METHOD("HEAD", HTTP_METHOD_HEAD)
    MICRO_METHOD("OPTIONS", HTTP_METHOD_OPTIONS)
#undef MICRO_METHOD
    return false;
}

/* Applies a newline-delimited header block through esp_http_client_set_header.
 * The IDF copies both strings, so the stack buffers can be reused per line. */
static int apply_headers(esp_http_client_handle_t client, const char *headers,
                         size_t headers_len)
{
    const char *cursor = headers;
    const char *end = headers + headers_len;
    while (cursor < end) {
        const char *newline = memchr(cursor, '\n', (size_t)(end - cursor));
        size_t line_len = newline != NULL ? (size_t)(newline - cursor)
                                          : (size_t)(end - cursor);
        if (line_len > 0 && cursor[line_len - 1] == '\r') {
            line_len--;
        }
        while (line_len > 0 && (cursor[0] == ' ' || cursor[0] == '\t')) {
            cursor++;
            line_len--;
        }
        while (line_len > 0 &&
               (cursor[line_len - 1] == ' ' || cursor[line_len - 1] == '\t')) {
            line_len--;
        }
        if (line_len == 0) {
            cursor = newline != NULL ? newline + 1 : end;
            continue;
        }

        const char *colon = memchr(cursor, ':', line_len);
        if (colon == NULL) {
            ESP_LOGW(TAG, "header line without a colon");
            return -1;
        }
        size_t name_len = (size_t)(colon - cursor);
        while (name_len > 0 &&
               (cursor[name_len - 1] == ' ' || cursor[name_len - 1] == '\t')) {
            name_len--;
        }
        const char *value = colon + 1;
        size_t value_len = line_len - (size_t)(value - cursor);
        while (value_len > 0 && (value[0] == ' ' || value[0] == '\t')) {
            value++;
            value_len--;
        }

        if (name_len == 0 || name_len >= MICRO_HTTP_MAX_HEADER_NAME ||
            value_len >= MICRO_HTTP_MAX_HEADER_VALUE) {
            ESP_LOGW(TAG, "header name/value is out of range");
            return -1;
        }
        for (size_t i = 0; i < name_len; ++i) {
            if (!token_byte((unsigned char)cursor[i])) {
                ESP_LOGW(TAG, "invalid header name");
                return -1;
            }
        }
        for (size_t i = 0; i < value_len; ++i) {
            unsigned char byte = (unsigned char)value[i];
            if (byte == '\r' || byte == '\n' || byte == '\0' || byte == 0x7f) {
                ESP_LOGW(TAG, "invalid header value");
                return -1;
            }
        }

        char name[MICRO_HTTP_MAX_HEADER_NAME];
        char text[MICRO_HTTP_MAX_HEADER_VALUE];
        memcpy(name, cursor, name_len);
        name[name_len] = '\0';
        memcpy(text, value, value_len);
        text[value_len] = '\0';
        if (managed_header(name)) {
            ESP_LOGW(TAG, "transport-owned header rejected: %s", name);
            return -1;
        }
        if (esp_http_client_set_header(client, name, text) != ESP_OK) {
            ESP_LOGW(TAG, "cannot set header %s", name);
            return -1;
        }
        cursor = newline != NULL ? newline + 1 : end;
    }
    return 0;
}

/* Reads the complete response body into a heap buffer. esp_http_client has
 * already parsed framing (chunked/content-length) in esp_http_client_perform,
 * so a 0-byte read means the message is complete. */
static int read_response(esp_http_client_handle_t client, bool body_expected,
                         int *status, char **response, size_t *response_len)
{
    size_t capacity = 8192;
    size_t used = 0;
    char *buffer = malloc(capacity);
    if (buffer == NULL) {
        return failf(status, response, response_len, "out of memory");
    }

    for (;;) {
        if (used + 1 >= capacity) {
            if (capacity >= MICRO_HTTP_MAX_RESPONSE) {
                free(buffer);
                return failf(status, response, response_len,
                             "response exceeds %u bytes", MICRO_HTTP_MAX_RESPONSE);
            }
            size_t next = capacity * 2;
            if (next > MICRO_HTTP_MAX_RESPONSE) {
                next = MICRO_HTTP_MAX_RESPONSE;
            }
            char *grown = realloc(buffer, next);
            if (grown == NULL) {
                free(buffer);
                return failf(status, response, response_len, "out of memory");
            }
            buffer = grown;
            capacity = next;
        }

        int read = esp_http_client_read(client, buffer + used,
                                        (int)(capacity - used - 1));
        if (read < 0) {
            free(buffer);
            return failf(status, response, response_len, "response read failed");
        }
        if (read == 0) {
            /* HEAD (and 204/304) legitimately carry no body even when the
             * headers advertise a Content-Length. */
            if (body_expected && !esp_http_client_is_complete_data_received(client)) {
                free(buffer);
                return failf(status, response, response_len,
                             "incomplete HTTP response");
            }
            break;
        }
        used += (size_t)read;
    }

    buffer[used] = '\0';
    *status = esp_http_client_get_status_code(client);
    *response = buffer;
    *response_len = used;
    return 0;
}

int micro_http_request(const char *method, size_t method_len,
                       const char *url, size_t url_len,
                       const char *headers, size_t headers_len,
                       const char *body, size_t body_len,
                       int *status, char **response, size_t *response_len)
{
    if (status == NULL || response == NULL || response_len == NULL) {
        return -1;
    }
    *status = 0;
    *response = NULL;
    *response_len = 0;

    if (method == NULL || method_len == 0 || method_len > MICRO_HTTP_MAX_METHOD ||
        url == NULL || url_len == 0 || url_len > MICRO_HTTP_MAX_URL ||
        (headers_len > 0 && headers == NULL) ||
        (body_len > 0 && body == NULL) || body_len > MICRO_HTTP_MAX_BODY) {
        return failf(status, response, response_len, "invalid HTTP request");
    }

    esp_http_client_method_t method_id;
    if (!method_from_token(method, method_len, &method_id)) {
        return failf(status, response, response_len,
                     "unsupported HTTP method: %.*s", (int)method_len, method);
    }

    char *url_text = malloc(url_len + 1);
    if (url_text == NULL) {
        return failf(status, response, response_len, "out of memory");
    }
    memcpy(url_text, url, url_len);
    url_text[url_len] = '\0';

    esp_http_client_config_t config = {0};
    config.url = url_text;
    config.method = method_id;
    config.timeout_ms = MICRO_HTTP_TIMEOUT_MS;
    config.buffer_size = 4096;
    config.buffer_size_tx = 1024;
    config.keep_alive_enable = false;
    config.disable_auto_redirect = false;
    config.user_agent = "micros-os/1.0";
#if defined(CONFIG_MBEDTLS_CERTIFICATE_BUNDLE)
    config.crt_bundle_attach = esp_crt_bundle_attach;
#endif

    int result = -1;
    esp_http_client_handle_t client = esp_http_client_init(&config);
    if (client == NULL) {
        result = failf(status, response, response_len,
                       "cannot create HTTP client");
        free(url_text);
        return result;
    }

    if (headers_len > 0 && apply_headers(client, headers, headers_len) != 0) {
        result = failf(status, response, response_len, "invalid header block");
        goto done;
    }
    if (body_len > 0) {
        esp_err_t set = esp_http_client_set_post_field(client, body, (int)body_len);
        if (set != ESP_OK) {
            result = failf(status, response, response_len,
                           "cannot attach request body: %s", esp_err_to_name(set));
            goto done;
        }
    }

    esp_err_t performed = esp_http_client_perform(client);
    if (performed != ESP_OK) {
        result = failf(status, response, response_len,
                       "HTTP transport error: %s", esp_err_to_name(performed));
        goto done;
    }
    result = read_response(client, method_id != HTTP_METHOD_HEAD, status, response,
                           response_len);
    if (result == 0) {
        ESP_LOGD(TAG, "%s %s -> %d", method, url_text, *status);
    }

done:
    esp_http_client_cleanup(client);
    free(url_text);
    return result;
}

void micro_http_free(char *response)
{
    free(response);
}
