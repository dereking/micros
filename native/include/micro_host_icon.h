#ifndef MICRO_HOST_ICON_H
#define MICRO_HOST_ICON_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#define MICRO_HOST_ICON_EXPORT __declspec(dllexport)
#else
#define MICRO_HOST_ICON_EXPORT __attribute__((visibility("default")))
#endif

/* Set the process (Dock / Cmd-Tab) icon from PNG bytes.
 *
 * The native host embeds assets/app-icon/icon-512.png and calls this once a
 * visible window exists, so a bare `cargo run` shows the Micro OS icon instead
 * of the generic console executable. macOS-only: the stub returns 0 elsewhere.
 * Returns 1 when the image was decoded and applied, 0 on bad data or an
 * unsupported platform. */
MICRO_HOST_ICON_EXPORT int micro_host_icon_set_png(const void *bytes, size_t length);

#ifdef __cplusplus
}
#endif

#endif
