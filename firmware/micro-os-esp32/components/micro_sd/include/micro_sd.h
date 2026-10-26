#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

/** Mount the board's TF card and create the Micro OS storage roots.
 *
 * Idempotent. Returns ESP_OK only when the card is mounted; a missing card or
 * a failed mount is reported as an error and leaves every operation disabled.
 * Never formats the card; the existing filesystem must be FAT12/16/32.
 */
esp_err_t micro_sd_init(void);

/** 1 while the card is mounted, 0 otherwise. */
int micro_sd_available(void);

/** Read one App-private UTF-8 file.
 *
 * `path` is relative to `/sdcard/micros/apps/<app_id>`. On success `*out` is a
 * NUL-terminated malloc buffer (NULL when the file does not exist) and
 * `*out_len` is the byte length without the terminator. Free with
 * `micro_sd_free`. Returns 0 on success, -1 on any error.
 */
int micro_sd_read(const char *app_id, const char *path, char **out, size_t *out_len);

/** Create/replace (`append == 0`) or append (`append == 1`) a file. */
int micro_sd_write(const char *app_id, const char *path, const char *data,
                   size_t data_len, int append);

int micro_sd_exists(const char *app_id, const char *path, int *out);
int micro_sd_remove(const char *app_id, const char *path, int *out);

/** List immediate children, one per line, directories suffixed by '/'.
 *
 * `*out` is a malloc buffer (NULL when empty); free with `micro_sd_free`.
 */
int micro_sd_list(const char *app_id, const char *path, char **out, size_t *out_len);

int micro_sd_mkdir(const char *app_id, const char *path, int *out);

/** File size in bytes, 0 for a directory, -1 when missing. */
int micro_sd_size(const char *app_id, const char *path, int64_t *out);

void micro_sd_free(void *ptr);

#ifdef __cplusplus
}
#endif
