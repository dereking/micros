/* Generic per-App key-value storage on the ESP32's NVS partition.
 *
 * The Rust host owns the scalar type encoding (`n:`, `s:`, `b:`, `z`) and
 * treats the value as an opaque string; this component only maps a namespace
 * (the App id) and a key to an NVS entry. NVS remains the single source of
 * truth, so App data survives a reboot and is isolated by namespace.
 */
#pragma once

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Returns the value length (excluding the NUL) and writes a NUL-terminated
 * value into `out`; 0 when the key is absent; -1 on an NVS error; -2 when the
 * caller's buffer is too small. */
int micro_kv_get(const char *namespace_id, const char *key, char *out, size_t cap);

/* Stores a NUL-terminated value. Returns 0 on success, -1 on error. */
int micro_kv_set(const char *namespace_id, const char *key, const char *value);

/* Removes a key; missing keys are a successful no-op. Returns 0/-1. */
int micro_kv_remove(const char *namespace_id, const char *key);

/* Mount the dedicated `micro_cfg` NVS partition. Falls back to the default
 * `nvs` partition (and logs a warning) if `micro_cfg` is absent. Call once at
 * boot before creating an App runtime. */
void micro_kv_init(void);

#ifdef __cplusplus
}
#endif
