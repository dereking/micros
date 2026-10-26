//! Foreground URL audio and the single background-music stream.
//!
//! The macOS bridge owns the actual `AVPlayer` instances; this module validates
//! URLs, allocates stable handles, and decides what survives an App exit:
//! foreground streams stop with the host, the background stream does not.

/// The longest URL accepted by the SDK. Keeps a bad App string from allocating
/// arbitrarily large AVPlayer/HTMLAudio resources.
const MAX_URL_LEN: usize = 4096;

pub struct UrlAudio {
    enabled: bool,
    next_handle: i32,
    active: Vec<i32>,
}

impl Default for UrlAudio {
    fn default() -> Self {
        Self {
            enabled: true,
            next_handle: 1,
            active: Vec::new(),
        }
    }
}

impl UrlAudio {
    /// Start a foreground stream. Returns a positive handle, or `0` when the
    /// URL is not an HTTP(S) URL or the host has audio disabled.
    pub fn play(&mut self, url: &str) -> i32 {
        if !self.enabled {
            return 0;
        }
        let Some(url) = validated_url(url) else {
            return 0;
        };
        let handle = self.next_handle;
        if !platform::play(url, handle) {
            return 0;
        }
        self.next_handle = handle.checked_add(1).filter(|next| *next > 0).unwrap_or(1);
        self.active.push(handle);
        handle
    }

    /// Stop one foreground stream. Unknown or already-finished handles are a
    /// no-op, so an App can stop a sound twice without an error.
    pub fn stop(&mut self, handle: i32) {
        if !self.enabled || handle <= 0 {
            return;
        }
        if let Some(index) = self.active.iter().position(|active| *active == handle) {
            self.active.swap_remove(index);
            platform::stop(handle);
        }
    }

    /// Stop every foreground stream. Used by `audio.stopAllAudio()` and when
    /// the App leaves the foreground; the background stream is untouched.
    pub fn stop_all(&mut self) {
        if !self.enabled || self.active.is_empty() {
            return;
        }
        self.active.clear();
        platform::stop_all();
    }

    /// Play or replace the one background music stream. It loops and is owned
    /// by the host process, so it survives the App host being dropped.
    pub fn play_background(&mut self, url: &str) {
        if !self.enabled {
            return;
        }
        let Some(url) = validated_url(url) else {
            return;
        };
        platform::play_background(url);
    }

    /// Stop the background music stream. Safe when nothing is playing.
    pub fn stop_background(&mut self) {
        if self.enabled {
            platform::stop_background();
        }
    }

    /// Disable audio for headless runs. Disabling also stops everything,
    /// including the background stream, so a smoke test is silent.
    pub fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.stop_all();
            self.stop_background();
        }
        self.enabled = enabled;
    }
}

impl Drop for UrlAudio {
    fn drop(&mut self) {
        // Leaving an App stops its foreground sounds. The background stream is
        // deliberately left running; that is the "background" part of the API.
        self.stop_all();
    }
}

/// Only HTTP(S) media URLs are accepted. `file://` is rejected so an App
/// cannot use the player as a local-file reader.
fn validated_url(url: &str) -> Option<&str> {
    let url = url.trim();
    if url.is_empty() || url.len() > MAX_URL_LEN {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    if url
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return None;
    }
    Some(url)
}

#[cfg(all(target_os = "macos", feature = "native", not(test)))]
mod platform {
    use std::ffi::{CString, c_char};

    unsafe extern "C" {
        fn micro_audio_play_url(url: *const c_char, handle: i32) -> i32;
        fn micro_audio_stop_url(handle: i32);
        fn micro_audio_stop_all_urls();
        fn micro_audio_play_background(url: *const c_char);
        fn micro_audio_stop_background();
    }

    pub fn play(url: &str, handle: i32) -> bool {
        let Ok(url) = CString::new(url) else {
            return false;
        };
        unsafe { micro_audio_play_url(url.as_ptr(), handle) != 0 }
    }

    pub fn stop(handle: i32) {
        unsafe { micro_audio_stop_url(handle) }
    }

    pub fn stop_all() {
        unsafe { micro_audio_stop_all_urls() }
    }

    pub fn play_background(url: &str) {
        let Ok(url) = CString::new(url) else {
            return;
        };
        unsafe { micro_audio_play_background(url.as_ptr()) }
    }

    pub fn stop_background() {
        unsafe { micro_audio_stop_background() }
    }
}

#[cfg(not(all(target_os = "macos", feature = "native", not(test))))]
mod platform {
    /* Unit tests exercise handle allocation without touching an audio device;
     * a non-native build has no player, so it reports that playback failed. */
    pub fn play(_url: &str, _handle: i32) -> bool {
        cfg!(test)
    }

    pub fn stop(_handle: i32) {}

    pub fn stop_all() {}

    pub fn play_background(_url: &str) {}

    pub fn stop_background() {}
}

#[cfg(test)]
mod tests {
    use super::UrlAudio;

    #[test]
    fn accepts_only_http_and_https_urls() {
        assert_eq!(UrlAudio::default().play(""), 0);
        assert_eq!(UrlAudio::default().play("   "), 0);
        assert_eq!(UrlAudio::default().play("file:///tmp/a.mp3"), 0);
        assert_eq!(UrlAudio::default().play("ftp://example.com/a.mp3"), 0);
        assert_eq!(UrlAudio::default().play("https://example.com/a b.mp3"), 0);
        assert_eq!(UrlAudio::default().play("https://example.com/a.mp3"), 1);
        assert_eq!(UrlAudio::default().play("HTTP://example.com/a.mp3"), 1);
    }

    #[test]
    fn foreground_handles_increase_and_stop_is_idempotent() {
        let mut audio = UrlAudio::default();
        let first = audio.play("https://example.com/one.mp3");
        let second = audio.play("https://example.com/two.mp3");
        assert!(first > 0);
        assert!(second > first);
        audio.stop(first);
        audio.stop(first);
        let third = audio.play("https://example.com/three.mp3");
        assert!(third > second);
    }

    #[test]
    fn disabled_audio_ignores_play_and_stops_everything() {
        let mut audio = UrlAudio::default();
        assert!(audio.play("https://example.com/one.mp3") > 0);
        audio.set_enabled(false);
        assert_eq!(audio.play("https://example.com/two.mp3"), 0);
        audio.play_background("https://example.com/bgm.mp3");
        assert_eq!(audio.active.len(), 0);
    }

    #[test]
    fn background_requires_a_valid_url() {
        let mut audio = UrlAudio::default();
        audio.play_background("");
        audio.play_background("file:///tmp/bgm.mp3");
        audio.play_background("https://example.com/bgm.mp3");
    }
}
