//! System speech output for the native host.
//!
//! `audio.speak` is backed by the platform's own synthesizer instead of a
//! command-line tool: macOS goes through a small Objective-C bridge in
//! `native/src/micro_speech.m` that owns an `AVSpeechSynthesizer`. Other
//! targets (and test builds without the `native` feature) compile to a silent
//! stub, which keeps the host API identical everywhere.

/// The SDK's rate multiplier (`1.0` = normal). The native host speaks slightly
/// slower by default because the App is a vocabulary trainer.
const DEFAULT_RATE: f64 = 0.9;
const MIN_RATE: f64 = 0.5;
const MAX_RATE: f64 = 2.0;

/// Speech output state owned by [`crate::host::NativeHost`].
pub struct Speech {
    rate: f64,
    enabled: bool,
}

impl Default for Speech {
    fn default() -> Self {
        Self {
            rate: DEFAULT_RATE,
            enabled: true,
        }
    }
}

impl Speech {
    /// The clamped rate multiplier applied by the next [`Self::speak`].
    #[cfg(test)]
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// Set the rate multiplier. Non-finite values are ignored; finite values
    /// are clamped to the SDK's 0.5–2.0 range.
    pub fn set_rate(&mut self, rate: f64) {
        if rate.is_finite() {
            self.rate = rate.clamp(MIN_RATE, MAX_RATE);
        }
    }

    /// Speak `text`, replacing any utterance already in flight. Empty or
    /// whitespace-only text is ignored so a missing word never makes noise.
    pub fn speak(&mut self, text: &str) {
        if !self.enabled {
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        platform::speak(text, self.rate);
    }

    /// Stop the current utterance. Idempotent.
    pub fn stop(&mut self) {
        if self.enabled {
            platform::stop();
        }
    }

    /// Disable output (used by headless tests and `--smoke` runs). Turning
    /// this off also stops anything already speaking.
    pub fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.stop();
        }
        self.enabled = enabled;
    }
}

impl Drop for Speech {
    fn drop(&mut self) {
        platform::stop();
    }
}

#[cfg(all(target_os = "macos", feature = "native"))]
mod platform {
    use std::ffi::{CString, c_char};

    unsafe extern "C" {
        fn micro_speech_speak(text: *const c_char, rate: f32);
        fn micro_speech_stop();
    }

    pub fn speak(text: &str, rate: f64) {
        let Ok(text) = CString::new(text) else {
            return;
        };
        /* AVSpeechUtterance treats 0.5 as the normal rate. Map the SDK's
         * multiplier onto that scale before handing it to the synthesizer. */
        let utterance_rate = (0.5 * rate).clamp(0.1, 1.0) as f32;
        unsafe { micro_speech_speak(text.as_ptr(), utterance_rate) }
    }

    pub fn stop() {
        unsafe { micro_speech_stop() }
    }
}

#[cfg(not(all(target_os = "macos", feature = "native")))]
mod platform {
    pub fn speak(_text: &str, _rate: f64) {}

    pub fn stop() {}
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_RATE, MAX_RATE, MIN_RATE, Speech};

    #[test]
    fn rate_defaults_slow_and_is_clamped() {
        let mut speech = Speech::default();
        assert_eq!(speech.rate(), DEFAULT_RATE);

        speech.set_rate(0.1);
        assert_eq!(speech.rate(), MIN_RATE);
        speech.set_rate(9.0);
        assert_eq!(speech.rate(), MAX_RATE);
        speech.set_rate(1.25);
        assert_eq!(speech.rate(), 1.25);
    }

    #[test]
    fn non_finite_rate_is_ignored() {
        let mut speech = Speech::default();
        speech.set_rate(f64::NAN);
        assert_eq!(speech.rate(), DEFAULT_RATE);
        speech.set_rate(f64::INFINITY);
        assert_eq!(speech.rate(), DEFAULT_RATE);
    }

    #[test]
    fn empty_text_is_a_no_op() {
        let mut speech = Speech::default();
        speech.speak("   ");
        speech.speak("");
        speech.stop();
    }
}
