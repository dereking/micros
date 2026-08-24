/* Real ESP32 STA Wi-Fi backend for the OS shell's `net.*` host calls.
 *
 * Owns every esp_wifi_* call and bridges the asynchronous esp_wifi events to
 * the synchronous host reads. The radio state, connected SSID, and scan result
 * are shared between the wifi task (which writes them in the event handler)
 * and the LVGL task (which reads them inside micro_runtime_tick), so all
 * accesses go through a spinlock.
 *
 * Connect flow: micro_wifi_connect -> esp_wifi_set_config + esp_wifi_connect.
 * On WIFI_EVENT_STA_CONNECTED the credentials are persisted to the default NVS
 * partition ("wifi" namespace) so a reboot auto-reconnects: the persisted pair
 * is loaded at init and applied from WIFI_EVENT_STA_START. Disconnect both
 * drops the link and erases the persisted network.
 *
 * Scan flow: micro_wifi_start_scan -> esp_wifi_scan_start(non-blocking);
 * WIFI_EVENT_SCAN_DONE builds a "\n"-joined "SSID, -rssi dBm" list into shared
 * state; micro_wifi_take_scan_result consumes it once and clears the flag.
 */

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "esp_event.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#include "nvs_flash.h"

#include "micro_wifi.h"

static const char *TAG = "micro_wifi";

#define MICRO_WIFI_STATE_CAP 16U
#define MICRO_WIFI_SSID_CAP 33U
#define MICRO_WIFI_PASS_CAP 65U
#define MICRO_WIFI_SCAN_CAP 512U
#define MICRO_WIFI_MAX_APS 16U

/* Shared radio state (see file header). */
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static char s_state[MICRO_WIFI_STATE_CAP] = "off";
static char s_ssid[MICRO_WIFI_SSID_CAP] = "";
static bool s_scan_done;
static char s_scan_result[MICRO_WIFI_SCAN_CAP];

/* The network a connect() was asked to join; replayed into the STA_CONNECTED /
 * STA_DISCONNECTED handlers (the event only carries BSSID, not SSID). */
static char s_last_ssid[MICRO_WIFI_SSID_CAP] = "";
static char s_last_pass[MICRO_WIFI_PASS_CAP] = "";
static bool s_auto_connect; /* persisted creds loaded; connect on STA_START */

static void set_state(const char *state, const char *ssid)
{
    portENTER_CRITICAL(&s_lock);
    if (state != NULL) {
        strncpy(s_state, state, sizeof s_state - 1);
        s_state[sizeof s_state - 1] = '\0';
    }
    if (ssid != NULL) {
        strncpy(s_ssid, ssid, sizeof s_ssid - 1);
        s_ssid[sizeof s_ssid - 1] = '\0';
    }
    portEXIT_CRITICAL(&s_lock);
}

/* --- NVS persistence (default "nvs" partition, "wifi" namespace) --- */

static void persist_creds(const char *ssid, const char *pass)
{
    nvs_handle_t handle;
    if (nvs_open("wifi", NVS_READWRITE, &handle) != ESP_OK) {
        ESP_LOGW(TAG, "cannot open NVS to persist %s", ssid);
        return;
    }
    esp_err_t result = nvs_set_str(handle, "ssid", ssid);
    result |= nvs_set_str(handle, "pass", pass);
    result |= nvs_commit(handle);
    nvs_close(handle);
    if (result != ESP_OK) {
        ESP_LOGW(TAG, "failed to persist %s: %s", ssid, esp_err_to_name(result));
    } else {
        ESP_LOGI(TAG, "persisted network %s", ssid);
    }
}

static void load_persisted_creds(void)
{
    nvs_handle_t handle;
    if (nvs_open("wifi", NVS_READONLY, &handle) != ESP_OK) {
        return;
    }
    size_t len = sizeof s_last_ssid;
    if (nvs_get_str(handle, "ssid", s_last_ssid, &len) == ESP_OK &&
        s_last_ssid[0] != '\0') {
        len = sizeof s_last_pass;
        if (nvs_get_str(handle, "pass", s_last_pass, &len) != ESP_OK) {
            s_last_pass[0] = '\0';
        }
        s_auto_connect = true;
        ESP_LOGI(TAG, "saved network %s; will auto-reconnect", s_last_ssid);
    } else {
        s_last_ssid[0] = '\0';
    }
    nvs_close(handle);
}

static void clear_persisted_creds(void)
{
    nvs_handle_t handle;
    if (nvs_open("wifi", NVS_READWRITE, &handle) == ESP_OK) {
        nvs_erase_all(handle);
        nvs_commit(handle);
        nvs_close(handle);
    }
}

/* --- connect --- */

static void start_connect(const char *ssid, const char *pass)
{
    wifi_config_t config = {0};
    strncpy((char *)config.sta.ssid, ssid, sizeof config.sta.ssid - 1);
    strncpy((char *)config.sta.password, pass, sizeof config.sta.password - 1);
    /* Leave threshold.authmode at the default so the driver negotiates the
     * AP's security instead of rejecting a mismatched auth type. */
    esp_err_t result = esp_wifi_set_config(WIFI_IF_STA, &config);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "set_config failed: %s", esp_err_to_name(result));
        set_state("error", ssid);
        return;
    }
    strncpy(s_last_ssid, ssid, sizeof s_last_ssid - 1);
    s_last_ssid[sizeof s_last_ssid - 1] = '\0';
    strncpy(s_last_pass, pass, sizeof s_last_pass - 1);
    s_last_pass[sizeof s_last_pass - 1] = '\0';
    set_state("connecting", ssid);
    result = esp_wifi_connect();
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "esp_wifi_connect failed: %s", esp_err_to_name(result));
        set_state("error", ssid);
    }
}

void micro_wifi_connect(const char *ssid, const char *pass)
{
    if (ssid == NULL || ssid[0] == '\0') {
        ESP_LOGW(TAG, "connect with empty SSID ignored");
        return;
    }
    start_connect(ssid, pass != NULL ? pass : "");
}

void micro_wifi_disconnect(void)
{
    set_state("off", "");
    clear_persisted_creds();
    esp_wifi_disconnect();
}

/* --- scan --- */

static void build_scan_result(void)
{
    uint16_t count = 0;
    if (esp_wifi_scan_get_ap_num(&count) != ESP_OK) {
        count = 0;
    }
    if (count > MICRO_WIFI_MAX_APS) {
        count = MICRO_WIFI_MAX_APS;
    }
    wifi_ap_record_t *records = NULL;
    if (count > 0) {
        records = calloc(count, sizeof *records);
        if (records == NULL) {
            count = 0;
        } else if (esp_wifi_scan_get_ap_records(&count, records) != ESP_OK) {
            count = 0;
        }
    }
    char result[MICRO_WIFI_SCAN_CAP];
    size_t used = 0;
    for (uint16_t i = 0; i < count && used + 2 < sizeof result; ++i) {
        int written = snprintf(result + used, sizeof result - used, "%s, %d dBm\n",
                               records[i].ssid, records[i].rssi);
        if (written <= 0 || (size_t)written >= sizeof result - used) {
            break;
        }
        used += (size_t)written;
    }
    if (records != NULL) {
        free(records);
    }
    portENTER_CRITICAL(&s_lock);
    memcpy(s_scan_result, result, used);
    s_scan_result[used] = '\0';
    s_scan_done = true;
    portEXIT_CRITICAL(&s_lock);
}

void micro_wifi_start_scan(void)
{
    wifi_scan_config_t config = {
        .ssid = NULL,
        .bssid = NULL,
        .channel = 0,
        .show_hidden = false,
        .scan_type = WIFI_SCAN_TYPE_ACTIVE,
        .scan_time = {
            .active = {.min = 0, .max = 0},
            .passive = 0,
        },
    };
    esp_err_t result = esp_wifi_scan_start(&config, false);
    if (result != ESP_OK) {
        ESP_LOGW(TAG, "scan_start failed: %s", esp_err_to_name(result));
    }
}

int micro_wifi_take_scan_result(char *buf, size_t cap)
{
    if (buf == NULL || cap == 0) {
        return -1;
    }
    buf[0] = '\0';
    portENTER_CRITICAL(&s_lock);
    bool fresh = s_scan_done;
    if (fresh) {
        s_scan_done = false;
        strncpy(buf, s_scan_result, cap - 1);
        buf[cap - 1] = '\0';
    }
    portEXIT_CRITICAL(&s_lock);
    return fresh ? 1 : 0;
}

/* --- state reads --- */

int micro_wifi_state(char *buf, size_t cap)
{
    if (buf == NULL || cap == 0) {
        return -1;
    }
    portENTER_CRITICAL(&s_lock);
    strncpy(buf, s_state, cap - 1);
    buf[cap - 1] = '\0';
    portEXIT_CRITICAL(&s_lock);
    return 0;
}

int micro_wifi_ssid(char *buf, size_t cap)
{
    if (buf == NULL || cap == 0) {
        return -1;
    }
    portENTER_CRITICAL(&s_lock);
    strncpy(buf, s_ssid, cap - 1);
    buf[cap - 1] = '\0';
    portEXIT_CRITICAL(&s_lock);
    return 0;
}

/* --- events --- */

static void wifi_event_handler(void *arg, esp_event_base_t base, int32_t id,
                               void *data)
{
    (void)arg;
    (void)base;
    (void)data;
    switch (id) {
    case WIFI_EVENT_STA_START:
        if (s_auto_connect) {
            s_auto_connect = false;
            ESP_LOGI(TAG, "auto-reconnecting to %s", s_last_ssid);
            start_connect(s_last_ssid, s_last_pass);
        }
        break;
    case WIFI_EVENT_STA_CONNECTED:
        set_state("connected", s_last_ssid);
        persist_creds(s_last_ssid, s_last_pass);
        break;
    case WIFI_EVENT_STA_DISCONNECTED:
        /* Not an "off" transition (deliberate disconnect clears state before
         * esp_wifi_disconnect), so this is a failed attempt or a dropped link. */
        char state[MICRO_WIFI_STATE_CAP];
        portENTER_CRITICAL(&s_lock);
        strncpy(state, s_state, sizeof state - 1);
        state[sizeof state - 1] = '\0';
        portEXIT_CRITICAL(&s_lock);
        if (strcmp(state, "off") != 0) {
            set_state("error", s_last_ssid);
        }
        break;
    case WIFI_EVENT_SCAN_DONE:
        build_scan_result();
        break;
    default:
        break;
    }
}

static void ip_event_handler(void *arg, esp_event_base_t base, int32_t id,
                             void *data)
{
    (void)arg;
    (void)base;
    ip_event_got_ip_t *event = data;
    ESP_LOGI(TAG, "STA got IP: " IPSTR, IP2STR(&event->ip_info.ip));
    (void)id;
}

void micro_wifi_init(void)
{
    esp_err_t result = nvs_flash_init();
    if (result == ESP_ERR_NVS_NO_FREE_PAGES ||
        result == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_LOGW(TAG, "NVS needs erase; re-initializing");
        nvs_flash_erase();
        result = nvs_flash_init();
    }
    if (result != ESP_OK) {
        ESP_LOGW(TAG, "NVS init failed: %s", esp_err_to_name(result));
    }

    result = esp_netif_init();
    if (result != ESP_OK && result != ESP_ERR_INVALID_STATE) {
        ESP_LOGE(TAG, "esp_netif_init failed: %s", esp_err_to_name(result));
    }
    result = esp_event_loop_create_default();
    if (result != ESP_OK && result != ESP_ERR_INVALID_STATE) {
        ESP_LOGE(TAG, "event loop create failed: %s", esp_err_to_name(result));
    }
    esp_netif_create_default_wifi_sta();

    wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
    result = esp_wifi_init(&cfg);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "esp_wifi_init failed: %s", esp_err_to_name(result));
        return;
    }
    esp_event_handler_instance_register(WIFI_EVENT, ESP_EVENT_ANY_ID,
                                        &wifi_event_handler, NULL, NULL);
    esp_event_handler_instance_register(IP_EVENT, IP_EVENT_STA_GOT_IP,
                                        &ip_event_handler, NULL, NULL);
    esp_wifi_set_mode(WIFI_MODE_STA);
    load_persisted_creds();
    result = esp_wifi_start();
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "esp_wifi_start failed: %s", esp_err_to_name(result));
    }
}
