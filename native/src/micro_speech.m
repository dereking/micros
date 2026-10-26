/* macOS system TTS bridge.
 *
 * The native host used to fork `say` for every word, which left overlapping
 * processes and made "play again" queue stale audio. Keeping one
 * AVSpeechSynthesizer alive gives us the system voice, immediate stop/replace
 * semantics and no subprocess per tap.
 */

#import <AVFoundation/AVFoundation.h>
#import <Foundation/Foundation.h>

#include "micro_speech.h"

static AVSpeechSynthesizer *g_synthesizer = nil;
static AVSpeechSynthesisVoice *g_voice = nil;

/* Create the synthesizer and pick an English voice once. Samantha matches the
 * voice the App used before; if it is not installed we fall back to the system
 * default for en-US, then to whatever English voice is available. */
static void micro_speech_prepare(void) {
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        g_synthesizer = [[AVSpeechSynthesizer alloc] init];

        AVSpeechSynthesisVoice *en_us = nil;
        for (AVSpeechSynthesisVoice *voice in [AVSpeechSynthesisVoice speechVoices]) {
            if (![voice.language isEqualToString:@"en-US"]) {
                continue;
            }
            if ([voice.identifier containsString:@"Samantha"]) {
                g_voice = voice;
                return;
            }
            if (en_us == nil) {
                en_us = voice;
            }
        }
        g_voice = en_us ?: [AVSpeechSynthesisVoice voiceWithLanguage:@"en-US"];
    });
}

static void micro_speech_stop_now(void) {
    if (g_synthesizer != nil) {
        [g_synthesizer stopSpeakingAtBoundary:AVSpeechBoundaryImmediate];
    }
}

void micro_speech_speak(const char *text, float rate) {
    if (text == NULL) {
        return;
    }
    NSString *string = [NSString stringWithUTF8String:text];
    if (string.length == 0) {
        return;
    }
    dispatch_block_t work = ^{
        micro_speech_prepare();
        if (g_synthesizer == nil) {
            return;
        }
        /* Replace instead of queue: a fast tap-through of words should only
         * hear the word currently on screen. */
        micro_speech_stop_now();

        AVSpeechUtterance *utterance = [AVSpeechUtterance speechUtteranceWithString:string];
        utterance.voice = g_voice;
        utterance.rate = rate;
        utterance.pitchMultiplier = 1.0f;
        utterance.volume = 1.0f;
        [g_synthesizer speakUtterance:utterance];
    };
    if ([NSThread isMainThread]) {
        work();
    } else {
        dispatch_async(dispatch_get_main_queue(), work);
    }
}

void micro_speech_stop(void) {
    dispatch_block_t work = ^{
        micro_speech_stop_now();
    };
    if ([NSThread isMainThread]) {
        work();
    } else {
        dispatch_async(dispatch_get_main_queue(), work);
    }
}
