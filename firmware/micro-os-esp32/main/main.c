#include <inttypes.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

#include "driver/gpio.h"
#include "esp_err.h"
#include "esp_flash.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_partition.h"
#include "esp_psram.h"
#include "esp_system.h"

#include "lvgl.h"

#include "micro_bsp_lcd7.h"
#include "micro_runtime_ffi.h"
#include "micro_system_ui.h"

#define MICRO_EXPECTED_MEMORY_BYTES (8U * 1024U * 1024U)
#define MICRO_APP_PARTITION_LABEL "micro_app"
#define MICRO_RUNTIME_EVENT_BUDGET 10000U
#define MICRO_RUNTIME_TICK_PERIOD_MS 30U
#define MICRO_RUNTIME_ERROR_BUFFER_SIZE 192U
#define MICRO_OS_MAX_ACTIONS 16U
#define MICRO_BOOT_BUTTON_GPIO 0U
#define MICRO_BOOT_DEBOUNCE_TICKS 4U
#define MICRO_WIFI_CONNECT_STEP_TICKS 2U

/* MBC1 file layout (little-endian, see crates/micro-ir/src/codec.rs):
 *   offset 0..4   magic "MBC1"
 *   offset 4..6   version (u16)
 *   offset 6..10  payload length (u32)
 *   offset 10..14 crc32 (u32) over the payload
 *   offset 14..   payload bytes
 * The total file size is therefore 14 + payload_length. */
#define MICRO_MBC_HEADER_SIZE 14U

static const char *TAG = "micro_os";

static micro_os_t *s_os;
static micro_bsp_display_t s_display;
static micro_runtime_t *s_app_runtime;
static uint32_t s_app_session;
static bool s_app_running;
static char s_runtime_error[MICRO_RUNTIME_ERROR_BUFFER_SIZE];

static micro_action_t s_actions[MICRO_OS_MAX_ACTIONS];
static micro_action_buffer_t s_action_buffer;

static bool s_shell_visible;
static bool s_shell_settings;
static char s_wifi_state[16] = "off";
static char s_wifi_ssid[64] = "";
static uint32_t s_backlight = 3;
static bool s_wifi_connecting;
static uint32_t s_wifi_connect_ticks;

static uint32_t s_boot_level_count;

static void micro_os_apply_action(const micro_action_t *action);
static size_t micro_os_apply_batch(const micro_action_t *actions, size_t count);
static void os_dispatch_event(const micro_event_t *event);

static void micro_boot_button_init(void)
{
    gpio_config_t config = {
        .pin_bit_mask = 1ULL << MICRO_BOOT_BUTTON_GPIO,
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_ENABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    gpio_config(&config);
    s_boot_level_count = 0;
}

/* The BOOT button acts as Back. Debounced: requires several consecutive
 * pressed samples before it is reported. */
static bool micro_boot_button_back_pressed(void)
{
    bool pressed = gpio_get_level(MICRO_BOOT_BUTTON_GPIO) == 0;
    if (pressed) {
        if (s_boot_level_count < MICRO_BOOT_DEBOUNCE_TICKS) {
            s_boot_level_count++;
        }
        return s_boot_level_count >= MICRO_BOOT_DEBOUNCE_TICKS;
    }
    s_boot_level_count = 0;
    return false;
}

static void micro_shell_render(void)
{
    if (s_shell_settings) {
        micro_system_ui_show_settings(s_wifi_state, s_wifi_ssid, s_backlight);
    } else {
        micro_system_ui_show_launcher(s_wifi_state, s_wifi_ssid, s_backlight);
    }
}

static void micro_wifi_set_state(const char *state, const char *ssid)
{
    if (state != NULL) {
        snprintf(s_wifi_state, sizeof s_wifi_state, "%s", state);
    }
    if (ssid != NULL) {
        snprintf(s_wifi_ssid, sizeof s_wifi_ssid, "%s", ssid);
    }
    micro_esp_host_set_wifi_state(s_wifi_state, s_wifi_ssid);
}

static void micro_wifi_begin_connect(const char *ssid)
{
    micro_wifi_set_state("connecting", ssid);
    s_wifi_connecting = true;
    s_wifi_connect_ticks = 0;
    if (s_shell_visible) {
        micro_shell_render();
    }
}

static void micro_backlight_apply(uint32_t level)
{
    s_backlight = level > 4 ? 4 : level;
    micro_esp_host_mirror_backlight(s_backlight);
    micro_bsp_backlight_set(&s_display, s_backlight > 1);
}

static void os_dispatch_event(const micro_event_t *event)
{
    s_action_buffer.actions = s_actions;
    s_action_buffer.capacity = MICRO_OS_MAX_ACTIONS;
    s_action_buffer.count = 0;
    s_action_buffer.required = 0;
    micro_error_t result =
        micro_os_dispatch(s_os, event, &s_action_buffer, s_runtime_error,
                          sizeof s_runtime_error);
    if (result != MICRO_OK) {
        ESP_LOGE(TAG, "os dispatch failed: code=%d message=%s", (int)result,
                 s_runtime_error);
        return;
    }
    micro_os_apply_batch(s_action_buffer.actions, s_action_buffer.count);
}

static size_t micro_os_apply_batch(const micro_action_t *actions, size_t count)
{
    if (count == 0) {
        return 0;
    }
    const micro_action_t *action = &actions[0];
    if (action->kind == MICRO_ACTION_ACTIONS) {
        size_t consumed = 1;
        for (uint32_t child = 0; child < action->child_count; ++child) {
            consumed += micro_os_apply_batch(&actions[consumed], count - consumed);
        }
        return consumed;
    }
    micro_os_apply_action(action);
    return 1;
}

static int micro_app_load(uint8_t **out_buffer, size_t *out_size)
{
    const esp_partition_t *partition = esp_partition_find_first(
        ESP_PARTITION_TYPE_DATA, ESP_PARTITION_SUBTYPE_ANY, MICRO_APP_PARTITION_LABEL);
    if (partition == NULL) {
        ESP_LOGE(TAG, "micro_app partition not found in partition table");
        return -1;
    }

    uint8_t header[MICRO_MBC_HEADER_SIZE] = {0};
    esp_err_t result = esp_partition_read(partition, 0, header, sizeof header);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "failed to read MBC header: %s", esp_err_to_name(result));
        return -1;
    }
    if (memcmp(header, "MBC1", 4U) != 0) {
        ESP_LOGE(TAG, "no MBC1 magic in micro_app partition");
        return -1;
    }
    uint32_t payload_length = 0U;
    memcpy(&payload_length, header + 6U, sizeof payload_length);
    const uint64_t total_size = (uint64_t)MICRO_MBC_HEADER_SIZE + payload_length;
    if (total_size > partition->size || total_size > UINT32_MAX) {
        ESP_LOGE(TAG, "MBC payload length %" PRIu32 " exceeds partition size",
                 payload_length);
        return -1;
    }
    uint8_t *buffer = heap_caps_malloc((size_t)total_size, MALLOC_CAP_SPIRAM);
    if (buffer == NULL) {
        ESP_LOGE(TAG, "failed to allocate %" PRIu64 " bytes in PSRAM for MBC",
                 total_size);
        return -1;
    }
    result = esp_partition_read(partition, 0, buffer, (size_t)total_size);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "failed to read MBC body: %s", esp_err_to_name(result));
        heap_caps_free(buffer);
        return -1;
    }
    ESP_LOGI(TAG, "loaded MBC: %" PRIu64 " bytes from micro_app partition",
             total_size);
    *out_buffer = buffer;
    *out_size = (size_t)total_size;
    return 0;
}

static void micro_os_start_app(uint32_t session)
{
    if (s_app_running) {
        return;
    }
    uint8_t *mbc = NULL;
    size_t mbc_size = 0;
    if (micro_app_load(&mbc, &mbc_size) != 0) {
        micro_event_t event = {0};
        event.kind = MICRO_EVENT_APP_FAILED;
        event.session_id = session;
        event.failure = MICRO_FAILURE_INTERNAL;
        os_dispatch_event(&event);
        return;
    }
    micro_runtime_t *runtime = micro_runtime_create(
        mbc, mbc_size, MICRO_RUNTIME_EVENT_BUDGET, s_runtime_error,
        sizeof s_runtime_error);
    heap_caps_free(mbc);
    if (runtime == NULL) {
        ESP_LOGE(TAG, "micro_runtime_create failed: %s", s_runtime_error);
        micro_event_t event = {0};
        event.kind = MICRO_EVENT_APP_FAILED;
        event.session_id = session;
        event.failure = MICRO_FAILURE_APP_CRASHED;
        os_dispatch_event(&event);
        return;
    }
    s_app_runtime = runtime;
    s_app_session = session;
    s_app_running = true;
    micro_system_ui_hide();
    s_shell_visible = false;

    /* AppStarted/AppStopped/AppFailed carry only session_id (and failure);
     * the app field must stay Unused (validate_canonical). */
    micro_event_t event = {0};
    event.kind = MICRO_EVENT_APP_STARTED;
    event.session_id = session;
    os_dispatch_event(&event);
    ESP_LOGI(TAG, "micro runtime created; app session %" PRIu32, (uint32_t)session);
}

static void micro_os_stop_app(uint32_t session)
{
    if (!s_app_running) {
        return;
    }
    micro_runtime_destroy(s_app_runtime);
    s_app_runtime = NULL;
    s_app_running = false;

    micro_event_t event = {0};
    event.kind = MICRO_EVENT_APP_STOPPED;
    event.session_id = session;
    os_dispatch_event(&event);
}

static void micro_os_apply_action(const micro_action_t *action)
{
    switch (action->kind) {
    case MICRO_ACTION_SHOW_LAUNCHER:
        s_shell_visible = true;
        s_shell_settings = false;
        micro_shell_render();
        break;
    case MICRO_ACTION_SHOW_SETTINGS:
        s_shell_visible = true;
        s_shell_settings = true;
        micro_shell_render();
        break;
    case MICRO_ACTION_SHOW_APP_ERROR:
        ESP_LOGW(TAG, "app error page (reason=%d); returning to launcher",
                 (int)action->failure);
        s_shell_visible = true;
        s_shell_settings = false;
        micro_shell_render();
        break;
    case MICRO_ACTION_START_APP:
        micro_os_start_app(action->session_id);
        break;
    case MICRO_ACTION_STOP_APP:
        micro_os_stop_app(action->session_id);
        break;
    case MICRO_ACTION_APPLY_BACKLIGHT:
        micro_backlight_apply((uint32_t)action->backlight);
        if (s_shell_visible) {
            micro_shell_render();
        }
        break;
    case MICRO_ACTION_REBOOT:
        ESP_LOGW(TAG, "reboot requested");
        esp_restart();
        break;
    default:
        /* No-op: the demo simulates Wi-Fi directly in the shell; the reducer's
         * Wi-Fi actions (scan/connect/persist) are not driven on-device. */
        break;
    }
}

static void micro_os_handle_tap(micro_system_ui_tap_t tap)
{
    switch (tap) {
    case MICRO_SYSTEM_UI_TAP_OPEN_COUNTER: {
        micro_event_t event = {0};
        event.kind = MICRO_EVENT_OPEN_APP;
        event.app = MICRO_APP_COUNTER;
        os_dispatch_event(&event);
        break;
    }
    case MICRO_SYSTEM_UI_TAP_OPEN_SETTINGS: {
        micro_event_t event = {0};
        event.kind = MICRO_EVENT_OPEN_SETTINGS;
        os_dispatch_event(&event);
        break;
    }
    case MICRO_SYSTEM_UI_TAP_BACK: {
        micro_event_t event = {0};
        event.kind = MICRO_EVENT_BACK_PRESSED;
        os_dispatch_event(&event);
        break;
    }
    case MICRO_SYSTEM_UI_TAP_BACKLIGHT_TOGGLE:
        micro_backlight_apply(s_backlight > 1 ? 1 : 4);
        if (s_shell_visible) {
            micro_shell_render();
        }
        break;
    case MICRO_SYSTEM_UI_TAP_WIFI_CONNECT:
        micro_wifi_begin_connect("micro-demo");
        break;
    case MICRO_SYSTEM_UI_TAP_WIFI_DISCONNECT:
        s_wifi_connecting = false;
        micro_wifi_set_state("off", "");
        if (s_shell_visible) {
            micro_shell_render();
        }
        break;
    default:
        break;
    }
}

static void micro_os_drain_host_intents(void)
{
    uint32_t backlight = 0;
    if (micro_esp_host_take_backlight_intent(&backlight)) {
        micro_backlight_apply(backlight);
    }
    char ssid[33] = {0};
    char pass[65] = {0};
    if (micro_esp_host_take_wifi_connect(ssid, sizeof ssid, pass, sizeof pass)) {
        ESP_LOGI(TAG, "app requested Wi-Fi connect to %s", ssid);
        micro_wifi_begin_connect(ssid);
    }
    if (micro_esp_host_take_wifi_disconnect()) {
        s_wifi_connecting = false;
        micro_wifi_set_state("off", "");
    }
}

static void micro_os_tick_cb(lv_timer_t *timer)
{
    (void)timer;

    micro_system_ui_tap_t tap;
    while ((tap = micro_system_ui_take_tap()) != MICRO_SYSTEM_UI_TAP_NONE) {
        micro_os_handle_tap(tap);
    }
    micro_os_drain_host_intents();

    if (micro_boot_button_back_pressed()) {
        micro_event_t event = {0};
        event.kind = MICRO_EVENT_BACK_PRESSED;
        os_dispatch_event(&event);
    }

    if (s_wifi_connecting) {
        if (++s_wifi_connect_ticks >= MICRO_WIFI_CONNECT_STEP_TICKS) {
            s_wifi_connecting = false;
            micro_wifi_set_state("connected", s_wifi_ssid[0] != '\0' ? s_wifi_ssid : "micro-demo");
            if (s_shell_visible) {
                micro_shell_render();
            }
        }
    }

    if (s_app_running && s_app_runtime != NULL) {
        micro_error_t result = micro_runtime_tick(s_app_runtime, s_runtime_error,
                                                  sizeof s_runtime_error);
        if (result != MICRO_OK) {
            ESP_LOGE(TAG, "app runtime tick failed: code=%d message=%s",
                     (int)result, s_runtime_error);
            micro_os_stop_app(s_app_session);
        }
    }
}

void app_main(void)
{
    uint32_t flash_size = 0;
    esp_err_t flash_result = esp_flash_get_size(NULL, &flash_size);
    size_t psram_size = esp_psram_get_size();

    ESP_LOGI(TAG, "reset reason: %d", (int)esp_reset_reason());

    if (flash_result != ESP_OK) {
        ESP_LOGE(TAG, "failed to detect Flash size: %s", esp_err_to_name(flash_result));
        abort();
    }

    ESP_LOGI(TAG, "detected Flash: %" PRIu32 " bytes", flash_size);
    ESP_LOGI(TAG, "detected PSRAM: %zu bytes", psram_size);

    if (flash_size != MICRO_EXPECTED_MEMORY_BYTES ||
        psram_size != MICRO_EXPECTED_MEMORY_BYTES) {
        ESP_LOGE(TAG,
                 "unsupported memory class: expected 8 MB Flash and 8 MB PSRAM");
        abort();
    }

    ESP_LOGI(TAG, "8 MB Flash / 8 MB PSRAM hardware class verified");

    esp_err_t result = micro_bsp_init(&s_display);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "LCD7 BSP initialization failed: %s", esp_err_to_name(result));
        abort();
    }

    result = micro_bsp_backlight_set(&s_display, true);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "backlight initialization failed: %s", esp_err_to_name(result));
        abort();
    }

    if (!lvgl_port_lock(0)) {
        ESP_LOGE(TAG, "could not lock LVGL for health screen");
        abort();
    }
    result = micro_bsp_draw_smoke_screen(&s_display);
    lvgl_port_unlock();
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "health screen failed: %s", esp_err_to_name(result));
        abort();
    }

    result = micro_bsp_mark_healthy();
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "profile health commit failed: %s", esp_err_to_name(result));
        abort();
    }

    ESP_LOGI(TAG, "clearing smoke screen for OS shell bring-up");
    if (!lvgl_port_lock(0)) {
        ESP_LOGE(TAG, "could not lock LVGL to clear smoke screen");
        abort();
    }
    lv_obj_clean(lv_screen_active());
    lv_obj_set_style_bg_color(lv_screen_active(), lv_color_hex(0xF2F4F0), 0);
    lv_obj_set_style_bg_opa(lv_screen_active(), LV_OPA_COVER, 0);
    lvgl_port_unlock();

    micro_boot_button_init();
    micro_backlight_apply(3);

    s_os = micro_os_create();
    if (s_os == NULL) {
        ESP_LOGE(TAG, "micro_os_create failed");
        abort();
    }

    /* The result field is only carried by the *_INITIALIZED events; the other
     * boot events reject a non-UNUSED result (validate_canonical). */
    micro_event_t boot_events[] = {
        {.kind = MICRO_EVENT_BOOT_SAMPLED, .flag = 0},
        {.kind = MICRO_EVENT_STORAGE_INITIALIZED, .result = MICRO_RESULT_OK},
        {.kind = MICRO_EVENT_PROFILE_VALIDATED, .result = MICRO_RESULT_OK},
        {.kind = MICRO_EVENT_DISPLAY_INITIALIZED, .result = MICRO_RESULT_OK},
        {.kind = MICRO_EVENT_SYSTEM_UI_INITIALIZED, .result = MICRO_RESULT_OK},
        {.kind = MICRO_EVENT_NETWORK_CONFIG_LOADED, .flag = 0},
        {.kind = MICRO_EVENT_SETUP_SKIPPED},
    };
    for (size_t i = 0; i < sizeof boot_events / sizeof boot_events[0]; ++i) {
        os_dispatch_event(&boot_events[i]);
    }

    /* TEMP-HEADLESS-TEST: auto-open the Counter app so the full app lifecycle
     * (load MBC, create runtime, host calls, AppRunning) can be verified over
     * the serial log without touching the screen. REMOVE AFTER TESTING. */
    micro_event_t auto_open = {0};
    auto_open.kind = MICRO_EVENT_OPEN_APP;
    auto_open.app = MICRO_APP_COUNTER;
    os_dispatch_event(&auto_open);

    ESP_LOGI(TAG, "OS shell ready; ticking every %u ms",
             (unsigned)MICRO_RUNTIME_TICK_PERIOD_MS);
    lv_timer_create(micro_os_tick_cb, MICRO_RUNTIME_TICK_PERIOD_MS, NULL);
}
