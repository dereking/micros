/* ESP32 host capabilities for the app SDK (`device.*` / `net.*`).
 *
 * Device reads use real IDF values (Flash/PSRAM size, reset reason). Backlight
 * and Wi-Fi live in plain C globals mirrored by the OS shell (main.c): the
 * App's `device.backlight()` / `net.wifiState()` read them, and
 * `device.setBacklight` / `net.wifiConnect` / `net.wifiDisconnect` set pending
 * intents that main.c drains into the OS reducer on its next tick.
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

#include "micro_runtime_ffi.h"

static uint32_t s_host_backlight = 3;
static char s_host_wifi_state[16] = "off";
static char s_host_wifi_ssid[64] = "";
static uint32_t s_host_backlight_intent_pending = 0;
static uint32_t s_host_backlight_intent_level = 3;
static uint32_t s_host_wifi_connect_pending = 0;
static char s_host_wifi_connect_ssid[33] = "";
static char s_host_wifi_connect_pass[65] = "";
static uint32_t s_host_wifi_disconnect_pending = 0;

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
    return copy_str(buf, cap, s_host_wifi_state);
}

int micro_esp_host_wifi_ssid(char *buf, size_t cap)
{
    return copy_str(buf, cap, s_host_wifi_ssid);
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

void micro_esp_host_set_wifi_state(const char *state, const char *ssid)
{
    if (state != NULL) {
        copy_str(s_host_wifi_state, sizeof s_host_wifi_state, state);
    }
    if (ssid != NULL) {
        copy_str(s_host_wifi_ssid, sizeof s_host_wifi_ssid, ssid);
    }
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
