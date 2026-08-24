/* ESP32 host capabilities for the app SDK (`device.*` / `net.*`).
 *
 * Device reads use real IDF values (Flash/PSRAM size, reset reason). Backlight
 * lives in a plain C global mirrored by main.c. Wi-Fi state/SSID/scan read the
 * real STA radio through the micro_wifi component (spinlock-guarded shared
 * state); connect/disconnect set pending intents that main.c drains into
 * micro_wifi_connect / micro_wifi_disconnect on its next tick.
 *
 * None of these functions take the LVGL lock: the app runtime calls them from
 * inside micro_runtime_tick on the LVGL task, where the lock is already held.
 */

#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "esp_flash.h"
#include "esp_psram.h"
#include "esp_system.h"
#include "esp_timer.h"

#include "driver/gpio.h"
#include "esp_err.h"

#include "micro_runtime_ffi.h"
#include "micro_http.h"
#include "micro_wifi.h"

static uint32_t s_host_backlight = 3;
static uint32_t s_host_backlight_intent_pending = 0;
static uint32_t s_host_backlight_intent_level = 3;
static uint32_t s_host_wifi_connect_pending = 0;
static char s_host_wifi_connect_ssid[33] = "";
static char s_host_wifi_connect_pass[65] = "";
static uint32_t s_host_wifi_disconnect_pending = 0;
static uint32_t s_host_launch_index_pending = 0;
static uint32_t s_host_launch_index_value = 0;
static uint32_t s_host_back_intent_pending = 0;

static int copy_str(char *buf, size_t cap, const char *value)
{
    if (buf == NULL || cap == 0) {
        return -1;
    }
    size_t len = strlen(value);
    if (len >= cap) {
        len = cap - 1;
    }
    memcpy(buf, value, len);
    buf[len] = '\0';
    return 0;
}

int micro_esp_host_device_name(char *buf, size_t cap)
{
    return copy_str(buf, cap, "micro-os");
}

int micro_esp_host_device_chip(char *buf, size_t cap)
{
    return copy_str(buf, cap, "ESP32-S3");
}

int micro_esp_host_device_flash_bytes(uint32_t *out)
{
    if (out == NULL) {
        return -1;
    }
    return esp_flash_get_size(NULL, out) == ESP_OK ? 0 : -1;
}

int micro_esp_host_device_psram_bytes(uint32_t *out)
{
    if (out == NULL) {
        return -1;
    }
    *out = (uint32_t)esp_psram_get_size();
    return 0;
}

static const char *reset_reason_string(esp_reset_reason_t reason)
{
    switch (reason) {
    case ESP_RST_UNKNOWN:
        return "unknown";
    case ESP_RST_POWERON:
        return "power-on";
    case ESP_RST_EXT:
        return "external-pin";
    case ESP_RST_SW:
        return "software";
    case ESP_RST_PANIC:
        return "panic";
    case ESP_RST_INT_WDT:
        return "int-wdt";
    case ESP_RST_TASK_WDT:
        return "task-wdt";
    case ESP_RST_WDT:
        return "wdt";
    case ESP_RST_DEEPSLEEP:
        return "deep-sleep";
    case ESP_RST_BROWNOUT:
        return "brownout";
    case ESP_RST_SDIO:
        return "sdio";
    case ESP_RST_USB:
        return "usb";
    case ESP_RST_JTAG:
        return "jtag";
    case ESP_RST_EFUSE:
        return "efuse";
    case ESP_RST_PWR_GLITCH:
        return "power-glitch";
    case ESP_RST_CPU_LOCKUP:
        return "cpu-lockup";
    default:
        return "unknown";
    }
}

int micro_esp_host_device_reset_reason(char *buf, size_t cap)
{
    return copy_str(buf, cap, reset_reason_string(esp_reset_reason()));
}

int micro_esp_host_backlight(uint32_t *out)
{
    if (out == NULL) {
        return -1;
    }
    *out = s_host_backlight;
    return 0;
}

int micro_esp_host_set_backlight(uint32_t level)
{
    s_host_backlight = level > 4 ? 4 : level;
    s_host_backlight_intent_pending = 1;
    s_host_backlight_intent_level = s_host_backlight;
    return 0;
}

/* OS shell mirrors the reducer's backlight here (no intent). */
void micro_esp_host_mirror_backlight(uint32_t level)
{
    s_host_backlight = level > 4 ? 4 : level;
}

int micro_esp_host_wifi_state(char *buf, size_t cap)
{
    return micro_wifi_state(buf, cap);
}

int micro_esp_host_wifi_ssid(char *buf, size_t cap)
{
    return micro_wifi_ssid(buf, cap);
}

void micro_esp_host_scan_start(void)
{
    micro_wifi_start_scan();
}

int micro_esp_host_take_scan_result(char *buf, size_t cap)
{
    return micro_wifi_take_scan_result(buf, cap);
}

int micro_esp_host_wifi_ap_name(uint32_t index, char *buf, size_t cap)
{
    return micro_wifi_ap_name(index, buf, cap);
}

int micro_esp_host_http_get(const uint8_t *url, size_t url_len)
{
    return micro_http_get((const char *)url, url_len);
}

int micro_esp_host_http_request(const uint8_t *method, size_t method_len,
                                const uint8_t *url, size_t url_len,
                                const uint8_t *body, size_t body_len)
{
    return micro_http_request((const char *)method, method_len, (const char *)url,
                              url_len, (const char *)body, body_len);
}

int micro_esp_host_http_take_result(char *buf, size_t cap)
{
    return micro_http_take_result(buf, cap);
}

/* --- device.gpio* (real GPIO, no lock: the app runtime calls these from
 * inside micro_runtime_tick on the LVGL task) --- */

int micro_esp_host_gpio_setup(uint32_t pin, const uint8_t *mode, size_t mode_len)
{
    if (mode == NULL || mode_len == 0 || mode_len > 8 || pin >= GPIO_NUM_MAX) {
        return -1;
    }
    char mode_str[9] = {0};
    memcpy(mode_str, mode, mode_len);
    gpio_mode_t gpio_mode;
    bool pull_up = false;
    bool pull_down = false;
    if (strcmp(mode_str, "in") == 0) {
        gpio_mode = GPIO_MODE_INPUT;
    } else if (strcmp(mode_str, "in-pullup") == 0) {
        gpio_mode = GPIO_MODE_INPUT;
        pull_up = true;
    } else if (strcmp(mode_str, "in-pulldown") == 0) {
        gpio_mode = GPIO_MODE_INPUT;
        pull_down = true;
    } else if (strcmp(mode_str, "out") == 0) {
        /* Input/output so the app can drive the pin and read it back: a pure
         * output keeps the input buffer off, so gpio_get_level would read 0. */
        gpio_mode = GPIO_MODE_INPUT_OUTPUT;
    } else {
        return -1;
    }
    gpio_config_t io = {
        .pin_bit_mask = UINT64_C(1) << pin,
        .mode = gpio_mode,
        .pull_up_en = pull_up ? GPIO_PULLUP_ENABLE : GPIO_PULLUP_DISABLE,
        .pull_down_en = pull_down ? GPIO_PULLDOWN_ENABLE : GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    return gpio_config(&io) == ESP_OK ? 0 : -1;
}

int micro_esp_host_gpio_write(uint32_t pin, uint32_t level)
{
    if (pin >= GPIO_NUM_MAX) {
        return -1;
    }
    return gpio_set_level((gpio_num_t)pin, level ? 1 : 0) == ESP_OK ? 0 : -1;
}

int micro_esp_host_gpio_read(uint32_t pin)
{
    if (pin >= GPIO_NUM_MAX) {
        return -1;
    }
    return gpio_get_level((gpio_num_t)pin);
}

int micro_esp_host_wifi_connect(const uint8_t *ssid, size_t ssid_len,
                                const uint8_t *pass, size_t pass_len)
{
    if (ssid == NULL || pass == NULL || ssid_len == 0 ||
        ssid_len >= sizeof(s_host_wifi_connect_ssid) ||
        pass_len >= sizeof(s_host_wifi_connect_pass)) {
        return -1;
    }
    memcpy(s_host_wifi_connect_ssid, ssid, ssid_len);
    s_host_wifi_connect_ssid[ssid_len] = '\0';
    memcpy(s_host_wifi_connect_pass, pass, pass_len);
    s_host_wifi_connect_pass[pass_len] = '\0';
    s_host_wifi_connect_pending = 1;
    return 0;
}

int micro_esp_host_wifi_disconnect(void)
{
    s_host_wifi_disconnect_pending = 1;
    return 0;
}

/* --- OS-shell accessors (main.c drains the app's pending intents) --- */

uint32_t micro_esp_host_take_backlight_intent(uint32_t *level)
{
    if (s_host_backlight_intent_pending == 0) {
        return 0;
    }
    s_host_backlight_intent_pending = 0;
    if (level != NULL) {
        *level = s_host_backlight_intent_level;
    }
    return 1;
}

uint32_t micro_esp_host_take_wifi_connect(char *ssid, size_t ssid_cap,
                                          char *pass, size_t pass_cap)
{
    if (s_host_wifi_connect_pending == 0) {
        return 0;
    }
    s_host_wifi_connect_pending = 0;
    if (ssid != NULL && ssid_cap > 0) {
        copy_str(ssid, ssid_cap, s_host_wifi_connect_ssid);
    }
    if (pass != NULL && pass_cap > 0) {
        copy_str(pass, pass_cap, s_host_wifi_connect_pass);
    }
    return 1;
}

uint32_t micro_esp_host_take_wifi_disconnect(void)
{
    if (s_host_wifi_disconnect_pending == 0) {
        return 0;
    }
    s_host_wifi_disconnect_pending = 0;
    return 1;
}

/* --- os.* host calls (app registry + navigation intents) ---
 * The app registry is populated by main.c (partition scan) via
 * micro_esp_host_set_app_count / micro_esp_host_set_app_entry. Until then the
 * registry is empty, so appName/appIcon return "". */

#define MICRO_HOST_MAX_APPS 8U

static uint32_t s_host_app_count = 0;
static char s_host_app_name[MICRO_HOST_MAX_APPS][32];
static char s_host_app_icon[MICRO_HOST_MAX_APPS][4];

void micro_esp_host_set_app_count(uint32_t count)
{
    s_host_app_count = count > MICRO_HOST_MAX_APPS ? MICRO_HOST_MAX_APPS : count;
}

void micro_esp_host_set_app_entry(uint32_t index, const char *name, const char *icon)
{
    if (index >= MICRO_HOST_MAX_APPS) {
        return;
    }
    copy_str(s_host_app_name[index], sizeof s_host_app_name[index], name != NULL ? name : "");
    copy_str(s_host_app_icon[index], sizeof s_host_app_icon[index], icon != NULL ? icon : "");
}

int micro_esp_host_app_name(uint32_t index, char *buf, size_t cap)
{
    if (index >= s_host_app_count) {
        return copy_str(buf, cap, "");
    }
    return copy_str(buf, cap, s_host_app_name[index]);
}

int micro_esp_host_app_icon(uint32_t index, char *buf, size_t cap)
{
    if (index >= s_host_app_count) {
        return copy_str(buf, cap, "");
    }
    return copy_str(buf, cap, s_host_app_icon[index]);
}

void micro_esp_host_set_launch_index(uint32_t index)
{
    s_host_launch_index_pending = 1;
    s_host_launch_index_value = index;
}

uint32_t micro_esp_host_take_launch_index(uint32_t *out)
{
    if (s_host_launch_index_pending == 0) {
        return 0;
    }
    s_host_launch_index_pending = 0;
    if (out != NULL) {
        *out = s_host_launch_index_value;
    }
    return 1;
}

void micro_esp_host_set_back_intent(void)
{
    s_host_back_intent_pending = 1;
}

uint32_t micro_esp_host_take_back(void)
{
    if (s_host_back_intent_pending == 0) {
        return 0;
    }
    s_host_back_intent_pending = 0;
    return 1;
}

uint32_t micro_esp_host_uptime_ms(void)
{
    return (uint32_t)(esp_timer_get_time() / 1000);
}

/* Forces this object file into the final link via
 * `--undefined=micro_esp_host_keepalive`, mirroring how placeholder.c anchors
 * the Rust ABI exports. Without it the archive member is dropped because the
 * Rust static library references micro_esp_host_* symbols after this archive
 * has already been consumed. */
const void *micro_esp_host_keepalive(void)
{
    return (const void *)micro_esp_host_wifi_state;
}
