#ifndef MICRO_AUDIO_H
#define MICRO_AUDIO_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#define MICRO_AUDIO_EXPORT __declspec(dllexport)
#else
#define MICRO_AUDIO_EXPORT __attribute__((visibility("default")))
#endif

/* URL audio bridge used by the native host's `audio.playAudio` / `audio.playBackground`.
 *
 * `play_url` returns 1 when a player was created and 0 for an invalid URL or
 * failed player. The caller supplies the handle, which must be positive and
 * unique for the lifetime of the process. Foreground players are independent
 * AVPlayer instances, so several can mix. `play_background` replaces the one
 * global looping music player; that player is intentionally not stopped by
 * `stop_all_urls`, which is how it survives leaving an App.
 *
 * All calls are safe from the VM thread; the bridge serializes state on a
 * private queue, so it never waits on the UI run loop. */
MICRO_AUDIO_EXPORT int32_t micro_audio_play_url(const char *url, int32_t handle);
MICRO_AUDIO_EXPORT void micro_audio_stop_url(int32_t handle);
MICRO_AUDIO_EXPORT void micro_audio_stop_all_urls(void);
MICRO_AUDIO_EXPORT void micro_audio_play_background(const char *url);
MICRO_AUDIO_EXPORT void micro_audio_stop_background(void);

#ifdef __cplusplus
}
#endif

#endif
