#ifndef MICRO_SPEECH_H
#define MICRO_SPEECH_H

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#define MICRO_SPEECH_EXPORT __declspec(dllexport)
#else
#define MICRO_SPEECH_EXPORT __attribute__((visibility("default")))
#endif

/* System text-to-speech bridge used by the native host's `audio.speak`.
 *
 * `rate` is an AVSpeechUtterance rate in the 0.0..1.0 range (0.5 is normal).
 * A new call replaces the utterance already in flight; calls are safe from the
 * VM thread and are dispatched to the main queue internally. */
MICRO_SPEECH_EXPORT void micro_speech_speak(const char *text, float rate);

/* Stop the current utterance immediately. Safe to call when idle. */
MICRO_SPEECH_EXPORT void micro_speech_stop(void);

#ifdef __cplusplus
}
#endif

#endif
