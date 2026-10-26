#include "micro_kv.h"

#include <string.h>

#include "esp_err.h"
#include "esp_log.h"
#include "nvs.h"
#include "nvs_flash.h"

/* NVS limits both namespaces and keys to 15 bytes. The compiler/SDK only
 * expose short ASCII keys, so reject anything that could be silently
 * truncated instead of aliasing another App's data. */
#define MICRO_KV_NAME_CAP 16
#define MICRO_KV_PARTITION "micro_cfg"

static const char *TAG = "micro_kv";
static int s_use_default_partition;
static int s_initialized;

void micro_kv_init(void)
{
    if (s_initialized) {
        return;
    }
    s_initialized = 1;
    esp_err_t result = nvs_flash_init_partition(MICRO_KV_PARTITION);
    if (result == ESP_ERR_NVS_NO_FREE_PAGES ||
        result == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_LOGW(TAG, "%s needs erase; re-initializing", MICRO_KV_PARTITION);
        nvs_flash_erase_partition(MICRO_KV_PARTITION);
        result = nvs_flash_init_partition(MICRO_KV_PARTITION);
    }
    s_use_default_partition = result != ESP_OK;
    if (s_use_default_partition) {
        ESP_LOGW(TAG, "cannot mount %s (%s); falling back to default nvs",
                 MICRO_KV_PARTITION, esp_err_to_name(result));
    }
}

static esp_err_t kv_open(const char *namespace_id, nvs_open_mode_t mode,
                         nvs_handle_t *out)
{
    if (!s_initialized) {
        micro_kv_init();
    }
    if (s_use_default_partition) {
        return nvs_open(namespace_id, mode, out);
    }
    return nvs_open_from_partition(MICRO_KV_PARTITION, namespace_id, mode, out);
}

static int valid_name(const char *name)
{
    return name != NULL && name[0] != '\0' && strlen(name) < MICRO_KV_NAME_CAP;
}

int micro_kv_get(const char *namespace_id, const char *key, char *out, size_t cap)
{
    if (!valid_name(namespace_id) || !valid_name(key) || out == NULL || cap == 0) {
        return -1;
    }
    nvs_handle_t handle;
    esp_err_t result = kv_open(namespace_id, NVS_READONLY, &handle);
    if (result == ESP_ERR_NVS_NOT_FOUND) {
        return 0;
    }
    if (result != ESP_OK) {
        return -1;
    }
    size_t length = 0;
    result = nvs_get_str(handle, key, NULL, &length);
    if (result == ESP_ERR_NVS_NOT_FOUND) {
        nvs_close(handle);
        return 0;
    }
    if (result != ESP_OK || length == 0 || length > cap) {
        nvs_close(handle);
        return result == ESP_ERR_NVS_INVALID_LENGTH ? -2 : -1;
    }
    result = nvs_get_str(handle, key, out, &length);
    nvs_close(handle);
    if (result != ESP_OK) {
        return -1;
    }
    /* `length` includes the NUL. */
    return (int)(length - 1);
}

int micro_kv_set(const char *namespace_id, const char *key, const char *value)
{
    if (!valid_name(namespace_id) || !valid_name(key) || value == NULL) {
        return -1;
    }
    nvs_handle_t handle;
    esp_err_t result = kv_open(namespace_id, NVS_READWRITE, &handle);
    if (result != ESP_OK) {
        return -1;
    }
    result = nvs_set_str(handle, key, value);
    if (result == ESP_OK) {
        result = nvs_commit(handle);
    }
    nvs_close(handle);
    return result == ESP_OK ? 0 : -1;
}

int micro_kv_remove(const char *namespace_id, const char *key)
{
    if (!valid_name(namespace_id) || !valid_name(key)) {
        return -1;
    }
    nvs_handle_t handle;
    esp_err_t result = kv_open(namespace_id, NVS_READWRITE, &handle);
    if (result == ESP_ERR_NVS_NOT_FOUND) {
        return 0;
    }
    if (result != ESP_OK) {
        return -1;
    }
    result = nvs_erase_key(handle, key);
    if (result == ESP_ERR_NVS_NOT_FOUND) {
        result = ESP_OK;
    }
    if (result == ESP_OK) {
        result = nvs_commit(handle);
    }
    nvs_close(handle);
    return result == ESP_OK ? 0 : -1;
}
