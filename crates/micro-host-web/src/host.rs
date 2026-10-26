//! Simulated host capabilities for the browser Web Player.
//!
//! The Web Player is a simulator: `device.*` and `net.*` return fixed,
//! plausible values (mirroring the native host). Async requests complete on
//! the next `tick` with a canned result.
//!
//! `os.*` navigation intents (launch an installed app / go back to the shell)
//! are written into a shared [`ShellState`] that the JS main loop drains,
//! mirroring the pending-intent pattern the ESP32 host uses on the C side.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use micro_ir::{FunctionId, HostRequest};
use micro_store::{ConfigDoc, KvValue, ScopedFiles, ScopedKv, StoreError};
#[cfg(not(target_arch = "wasm32"))]
use micro_store::{FileStore, MemoryFileStore};
#[cfg(target_arch = "wasm32")]
use micro_store::{MAX_FILE_BYTES, MAX_LIST_ENTRIES, validate_app_id, validate_path};
use micro_vm::{HostAccess, Value, VmError};

/// Shared OS-shell state, polled by the Web Player each tick. The app registry
/// lives here too so the JS layer can inject the decoded app list without
/// reaching through the boxed `WebHost`.
#[derive(Default)]
pub struct ShellState {
    /// The installed-app index requested via `os.launchIndex`.
    pub pending_launch: Option<u32>,
    /// Whether the current app requested `os.goBack`.
    pub pending_back: bool,
    /// Installed-app registry: `(name, icon)` per index (index = `os.appName(i)`).
    pub apps: Vec<(String, String)>,
}

#[derive(Default)]
pub struct WebHost {
    /// The current App's KV namespace, opened from the shared `micro-store`
    /// contract (localStorage on Web, an in-memory map off-wasm).
    kv: Option<Box<dyn ScopedKv>>,
    /// The current App's config document (one localStorage JSON object).
    config: Option<Box<dyn ScopedKv>>,
    /// The current App's private file directory (localStorage on wasm, an
    /// in-memory store for native unit tests).
    files: Option<Box<dyn ScopedFiles>>,
    backlight: u8,
    pending: Vec<(FunctionId, Value)>,
    /// Simulated GPIO output registers (pin → level) so read-after-write works.
    gpio: BTreeMap<u32, u8>,
    /// SSIDs from the most recent `net.scanWifi`, indexed by `net.wifiApName`.
    scan_aps: Vec<String>,
    /// `audio.setRate` multiplier; `None` uses the SDK default (0.9).
    speech_rate: Option<f64>,
    /// Foreground URL audio streams owned by this App. Dropping the host
    /// pauses and releases them; the background stream lives in a static and
    /// deliberately outlives this vector.
    #[cfg(target_arch = "wasm32")]
    url_audio: Vec<(u32, web_sys::HtmlAudioElement)>,
    /// Shared OS-shell state, drained by the JS main loop.
    pub nav: Rc<RefCell<ShellState>>,
}

impl Drop for WebHost {
    fn drop(&mut self) {
        /* Leaving an App drops its host; stop speech and foreground URL sounds
         * so they do not keep playing over the shell or the next App. The
         * background music stream is global and intentionally keeps playing. */
        #[cfg(target_arch = "wasm32")]
        {
            self.release_app_audio();
        }
    }
}

impl WebHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a host bound to an App's KV namespace, config document and
    /// private file namespace.
    pub fn new_for_app(app_id: &str) -> Self {
        let app_id = if app_id.is_empty() { "app" } else { app_id };
        let mut host = Self::default();
        host.kv = Some(Box::new(WebKv::new(app_id)));
        host.config = Some(Box::new(WebConfigKv::new(app_id)));
        host.files = open_web_files(app_id);
        /* The firmware boots the panel at level 3 (`micro_backlight_apply(3)`
         * in main.c) and mirrors that level into the host, so the simulated
         * panel starts there too: an App reading `device.backlight()` must see
         * a lit panel, not 0 (which on the real board means "panel off"). */
        host.backlight = 3;
        host
    }

    fn kv_get(&self, key: &str) -> Option<KvValue> {
        store_get(&self.kv, key)
    }

    fn kv_set(&mut self, key: &str, value: KvValue) {
        store_set(&mut self.kv, key, value);
    }

    fn kv_remove(&mut self, key: &str) {
        store_remove(&mut self.kv, key);
    }

    fn config_get(&self, key: &str) -> Option<KvValue> {
        // Pre-v22 Apps kept config-like values in `kv`; keep reading them until
        // the first config write moves them.
        store_get(&self.config, key).or_else(|| store_get(&self.kv, key))
    }

    fn config_set(&mut self, key: &str, value: KvValue) {
        let migrates_legacy = store_get(&self.config, key).is_none();
        store_set(&mut self.config, key, value);
        if migrates_legacy {
            store_remove(&mut self.kv, key);
        }
    }

    fn config_remove(&mut self, key: &str) {
        store_remove(&mut self.config, key);
        store_remove(&mut self.kv, key);
    }

    /// The rate multiplier handed to the Web Speech API. Mirrors the SDK
    /// default and clamp so an App sees the same behavior on every host.
    #[cfg(target_arch = "wasm32")]
    fn speech_rate(&self) -> f64 {
        self.speech_rate.unwrap_or(0.9).clamp(0.5, 2.0)
    }

    /// Start a foreground URL stream and return its handle (`0` on invalid
    /// URL or when the browser refuses the synchronous `play` call).
    #[cfg(target_arch = "wasm32")]
    fn play_url_audio(&mut self, url: &str) -> f64 {
        let Some(url) = valid_audio_url(url) else {
            return 0.0;
        };
        // One-shots clean themselves up when the next sound starts; this keeps
        // a long session from retaining thousands of finished elements.
        self.url_audio.retain(|(_, audio)| !audio.ended());
        let Ok(audio) = web_sys::HtmlAudioElement::new_with_src(url) else {
            return 0.0;
        };
        audio.set_loop(false);
        audio.set_volume(1.0);
        let Ok(promise) = audio.play() else {
            return 0.0;
        };
        ignore_rejected_play(promise);
        let handle = next_audio_handle();
        self.url_audio.push((handle, audio));
        f64::from(handle)
    }

    #[cfg(target_arch = "wasm32")]
    fn stop_url_audio(&mut self, handle: u32) {
        let Some(index) = self
            .url_audio
            .iter()
            .position(|(active, _)| *active == handle)
        else {
            return;
        };
        let (_, audio) = self.url_audio.remove(index);
        stop_audio_element(&audio);
    }

    #[cfg(target_arch = "wasm32")]
    fn stop_all_url_audio(&mut self) {
        for (_, audio) in self.url_audio.drain(..) {
            stop_audio_element(&audio);
        }
    }

    /// Stop everything owned by this App. Background music lives in a
    /// thread-local outside the host and is deliberately left alone.
    #[cfg(target_arch = "wasm32")]
    fn release_app_audio(&mut self) {
        stop_web();
        self.stop_all_url_audio();
    }
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static NEXT_AUDIO_HANDLE: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
    static BACKGROUND_AUDIO: RefCell<Option<web_sys::HtmlAudioElement>> =
        const { RefCell::new(None) };
    static PLAY_REJECTION_NOOP: wasm_bindgen::closure::ScopedClosure<
        'static,
        dyn FnMut(wasm_bindgen::JsValue),
    > = wasm_bindgen::closure::ScopedClosure::new(|_| {});
}

#[cfg(target_arch = "wasm32")]
fn next_audio_handle() -> u32 {
    NEXT_AUDIO_HANDLE.with(|next| {
        let handle = next.get();
        let following = handle.wrapping_add(1);
        next.set(if following == 0 { 1 } else { following });
        handle
    })
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn valid_audio_url(url: &str) -> Option<&str> {
    let url = url.trim();
    if url.is_empty() || url.len() > 4096 {
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

#[cfg(target_arch = "wasm32")]
fn stop_audio_element(audio: &web_sys::HtmlAudioElement) {
    let _ = audio.pause();
    audio.set_src("");
    let _ = audio.load();
}

#[cfg(target_arch = "wasm32")]
fn ignore_rejected_play(promise: js_sys::Promise) {
    // Browsers reject `play()` for autoplay-policy violations. Attach a shared
    // no-op catch so the rejection is handled; the handle simply produces no
    // sound instead of leaving an unhandled Promise rejection in the console.
    PLAY_REJECTION_NOOP.with(|callback| {
        let _ = promise.catch(callback);
    });
}

#[cfg(target_arch = "wasm32")]
fn play_background_audio(url: &str) {
    let Some(url) = valid_audio_url(url) else {
        return;
    };
    let previous = BACKGROUND_AUDIO.with(|slot| slot.borrow_mut().take());
    if let Some(previous) = previous {
        stop_audio_element(&previous);
    }
    let Ok(audio) = web_sys::HtmlAudioElement::new_with_src(url) else {
        return;
    };
    audio.set_loop(true);
    audio.set_volume(1.0);
    let Ok(promise) = audio.play() else {
        return;
    };
    ignore_rejected_play(promise);
    BACKGROUND_AUDIO.with(|slot| *slot.borrow_mut() = Some(audio));
}

#[cfg(target_arch = "wasm32")]
fn stop_background_audio() {
    let previous = BACKGROUND_AUDIO.with(|slot| slot.borrow_mut().take());
    if let Some(previous) = previous {
        stop_audio_element(&previous);
    }
}

fn store_get(store: &Option<Box<dyn ScopedKv>>, key: &str) -> Option<KvValue> {
    store
        .as_ref()
        .and_then(|store| store.get(key).ok().flatten())
}
fn store_set(store: &mut Option<Box<dyn ScopedKv>>, key: &str, value: KvValue) {
    if let Some(store) = store.as_mut() {
        let _ = store.set(key, &value);
    }
}

fn store_remove(store: &mut Option<Box<dyn ScopedKv>>, key: &str) {
    if let Some(store) = store.as_mut() {
        let _ = store.remove(key);
    }
}

/// A `ScopedKv` backed by the spec's per-key localStorage layout
/// (`micro:kv:<app>:<key>`). Off-wasm (unit tests, native checks) it uses an
/// in-memory map so the same code path is exercised without a browser.
struct WebKv {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    app_id: String,
    #[cfg(not(target_arch = "wasm32"))]
    memory: RefCell<BTreeMap<String, KvValue>>,
}

impl WebKv {
    fn new(app_id: &str) -> Self {
        let app_id = if app_id.is_empty() { "app" } else { app_id };
        Self {
            app_id: app_id.to_owned(),
            #[cfg(not(target_arch = "wasm32"))]
            memory: RefCell::new(BTreeMap::new()),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn storage_key(&self, key: &str) -> String {
        format!("micro:kv:{}:{key}", self.app_id)
    }

    #[cfg(target_arch = "wasm32")]
    fn storage(&self) -> Result<web_sys::Storage, StoreError> {
        web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .ok_or_else(|| StoreError::Unsupported("localStorage is unavailable".into()))
    }
}

impl ScopedKv for WebKv {
    fn get(&self, key: &str) -> Result<Option<KvValue>, StoreError> {
        #[cfg(target_arch = "wasm32")]
        {
            let storage = self.storage()?;
            let raw = storage
                .get_item(&self.storage_key(key))
                .map_err(|_| StoreError::Io("localStorage.getItem failed".into()))?;
            return Ok(raw.and_then(|raw| {
                serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(KvValue::from_json)
            }));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Ok(self.memory.borrow().get(key).cloned())
        }
    }

    fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError> {
        if let KvValue::Number(value) = value
            && !value.is_finite()
        {
            return Err(StoreError::Unsupported(
                "non-finite numbers cannot be stored".into(),
            ));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let storage = self.storage()?;
            storage
                .set_item(&self.storage_key(key), &value.to_json().to_string())
                .map_err(|_| StoreError::Io("localStorage.setItem failed (quota?)".into()))?;
            return Ok(());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.memory
                .borrow_mut()
                .insert(key.to_owned(), value.clone());
            Ok(())
        }
    }

    fn remove(&mut self, key: &str) -> Result<(), StoreError> {
        #[cfg(target_arch = "wasm32")]
        {
            let storage = self.storage()?;
            storage
                .remove_item(&self.storage_key(key))
                .map_err(|_| StoreError::Io("localStorage.removeItem failed".into()))?;
            return Ok(());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.memory.borrow_mut().remove(key);
            Ok(())
        }
    }
}

/// A `ScopedKv` backed by one localStorage JSON object
/// (`micro:config:<app>`), matching the `config.json` document the native and
/// ESP32 hosts use. Off-wasm (unit tests, native checks) it keeps the same
/// document in memory so the code path is exercised without a browser.
struct WebConfigKv {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    app_id: String,
    #[cfg(not(target_arch = "wasm32"))]
    document: RefCell<ConfigDoc>,
}

impl WebConfigKv {
    fn new(app_id: &str) -> Self {
        let app_id = if app_id.is_empty() { "app" } else { app_id };
        Self {
            app_id: app_id.to_owned(),
            #[cfg(not(target_arch = "wasm32"))]
            document: RefCell::new(ConfigDoc::default()),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn storage_key(&self) -> String {
        format!("micro:config:{}", self.app_id)
    }

    #[cfg(target_arch = "wasm32")]
    fn storage(&self) -> Result<web_sys::Storage, StoreError> {
        web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .ok_or_else(|| StoreError::Unsupported("localStorage is unavailable".into()))
    }

    #[cfg(target_arch = "wasm32")]
    fn load(&self) -> Result<ConfigDoc, StoreError> {
        let storage = self.storage()?;
        let raw = storage
            .get_item(&self.storage_key())
            .map_err(|_| StoreError::Io("localStorage.getItem failed".into()))?;
        match raw {
            Some(raw) => ConfigDoc::parse(&raw),
            None => Ok(ConfigDoc::default()),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn save(&self, document: &ConfigDoc) -> Result<(), StoreError> {
        self.storage()?
            .set_item(&self.storage_key(), &document.to_json())
            .map_err(|_| StoreError::Io("localStorage.setItem failed (quota?)".into()))
    }
}

impl ScopedKv for WebConfigKv {
    fn get(&self, key: &str) -> Result<Option<KvValue>, StoreError> {
        #[cfg(target_arch = "wasm32")]
        {
            return Ok(self.load()?.get(key));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Ok(self.document.borrow().get(key))
        }
    }

    fn set(&mut self, key: &str, value: &KvValue) -> Result<(), StoreError> {
        #[cfg(target_arch = "wasm32")]
        {
            let mut document = self.load()?;
            document.set(key, value)?;
            self.save(&document)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.document.borrow_mut().set(key, value)
        }
    }

    fn remove(&mut self, key: &str) -> Result<(), StoreError> {
        #[cfg(target_arch = "wasm32")]
        {
            let mut document = self.load()?;
            document.remove(key);
            self.save(&document)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.document.borrow_mut().remove(key);
            Ok(())
        }
    }
}

fn open_web_files(app_id: &str) -> Option<Box<dyn ScopedFiles>> {
    #[cfg(target_arch = "wasm32")]
    {
        validate_app_id(app_id).ok()?;
        Some(Box::new(WebFiles::new(app_id)))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        MemoryFileStore::new().open(app_id).ok()
    }
}

/// A `ScopedFiles` backed by localStorage keys (`micro:fs:file:<app>:<path>`
/// and `micro:fs:dir:<app>:<path>`). This is a simulator backend: it has the
/// same per-App boundary, path validation and size cap as the real SD card, but
/// its capacity is the browser's localStorage quota.
#[cfg(target_arch = "wasm32")]
struct WebFiles {
    app_id: String,
}

#[cfg(target_arch = "wasm32")]
impl WebFiles {
    fn new(app_id: &str) -> Self {
        Self {
            app_id: app_id.to_owned(),
        }
    }

    fn storage(&self) -> Result<web_sys::Storage, StoreError> {
        web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .ok_or_else(|| StoreError::Unsupported("localStorage is unavailable".into()))
    }

    fn file_key(&self, path: &str) -> String {
        format!("micro:fs:file:{}:{path}", self.app_id)
    }

    fn dir_key(&self, path: &str) -> String {
        format!("micro:fs:dir:{}:{path}", self.app_id)
    }

    /// `(is_dir, path)` for every file and explicit directory owned by this App.
    fn paths(&self) -> Result<Vec<(bool, String)>, StoreError> {
        let storage = self.storage()?;
        let file_prefix = format!("micro:fs:file:{}:", self.app_id);
        let dir_prefix = format!("micro:fs:dir:{}:", self.app_id);
        let length = storage
            .length()
            .map_err(|_| StoreError::Io("localStorage.length failed".into()))?;
        let mut paths = Vec::new();
        for index in 0..length {
            let Some(key) = storage
                .key(index)
                .map_err(|_| StoreError::Io("localStorage.key failed".into()))?
            else {
                continue;
            };
            if let Some(rest) = key.strip_prefix(&file_prefix) {
                if !rest.is_empty() {
                    paths.push((false, rest.to_owned()));
                }
            } else if let Some(rest) = key.strip_prefix(&dir_prefix) {
                paths.push((true, rest.to_owned()));
            }
        }
        Ok(paths)
    }

    fn is_dir(&self, path: &str) -> Result<bool, StoreError> {
        Ok(self
            .paths()?
            .iter()
            .any(|(is_dir, candidate)| *is_dir && candidate == path))
    }

    fn is_file(&self, path: &str) -> Result<bool, StoreError> {
        Ok(self
            .paths()?
            .iter()
            .any(|(is_dir, candidate)| !*is_dir && candidate == path))
    }

    fn ensure_parents_are_dirs(&self, path: &str) -> Result<(), StoreError> {
        let mut prefix = String::new();
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                break;
            }
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if self.is_file(&prefix)? {
                return Err(StoreError::Unsupported("a path component is a file".into()));
            }
        }
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl ScopedFiles for WebFiles {
    fn available(&self) -> bool {
        self.storage().is_ok()
    }

    fn read_text(&self, path: &str) -> Result<Option<String>, StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Ok(Some(String::new()));
        }
        if self.is_dir(path)? {
            return Err(StoreError::Unsupported("path is a directory".into()));
        }
        let value = self
            .storage()?
            .get_item(&self.file_key(path))
            .map_err(|_| StoreError::Io("localStorage.getItem failed".into()))?;
        match value {
            Some(value) if value.len() > MAX_FILE_BYTES => Err(StoreError::Full),
            other => Ok(other),
        }
    }

    fn write_text(&mut self, path: &str, text: &str) -> Result<(), StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Err(StoreError::Unsupported("cannot write the App root".into()));
        }
        if text.len() > MAX_FILE_BYTES {
            return Err(StoreError::Full);
        }
        self.ensure_parents_are_dirs(path)?;
        if self.is_dir(path)? {
            return Err(StoreError::Unsupported("path is a directory".into()));
        }
        let storage = self.storage()?;
        storage
            .set_item(&self.file_key(path), text)
            .map_err(|_| StoreError::Io("localStorage.setItem failed (quota?)".into()))
    }

    fn append_text(&mut self, path: &str, text: &str) -> Result<(), StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Err(StoreError::Unsupported("cannot write the App root".into()));
        }
        let current = self.read_text(path)?.unwrap_or_default();
        if current.len() + text.len() > MAX_FILE_BYTES {
            return Err(StoreError::Full);
        }
        let mut combined = current;
        combined.push_str(text);
        self.write_text(path, &combined)
    }

    fn exists(&self, path: &str) -> Result<bool, StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Ok(self.storage().is_ok());
        }
        let prefix = format!("{path}/");
        Ok(self
            .paths()?
            .iter()
            .any(|(_, candidate)| candidate == path || candidate.starts_with(&prefix)))
    }

    fn remove(&mut self, path: &str) -> Result<bool, StoreError> {
        validate_path(path)?;
        if path.is_empty() {
            return Ok(false);
        }
        let storage = self.storage()?;
        let file_prefix = format!("micro:fs:file:{}:{path}/", self.app_id);
        let dir_prefix = format!("micro:fs:dir:{}:{path}/", self.app_id);
        let file_key = self.file_key(path);
        let dir_key = self.dir_key(path);
        let mut keys = Vec::new();
        let length = storage
            .length()
            .map_err(|_| StoreError::Io("localStorage.length failed".into()))?;
        for index in 0..length {
            let Some(key) = storage
                .key(index)
                .map_err(|_| StoreError::Io("localStorage.key failed".into()))?
            else {
                continue;
            };
            if key == file_key
                || key == dir_key
                || key.starts_with(&file_prefix)
                || key.starts_with(&dir_prefix)
            {
                keys.push(key);
            }
        }
        for key in &keys {
            storage
                .remove_item(key)
                .map_err(|_| StoreError::Io("localStorage.removeItem failed".into()))?;
        }
        Ok(!keys.is_empty())
    }

    fn list(&self, path: &str) -> Result<Vec<String>, StoreError> {
        validate_path(path)?;
        if self.is_file(path)? {
            return Err(StoreError::Unsupported("path is a file".into()));
        }
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        let mut entries = std::collections::BTreeSet::new();
        for (is_dir, candidate) in self.paths()? {
            let Some(rest) = candidate.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            let first = rest.split('/').next().unwrap_or(rest);
            if is_dir || rest.contains('/') {
                entries.insert(format!("{first}/"));
            } else {
                entries.insert(first.to_owned());
            }
        }
        Ok(entries.into_iter().take(MAX_LIST_ENTRIES).collect())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), StoreError> {
        validate_path(path)?;
        self.ensure_parents_are_dirs(path)?;
        if self.is_file(path)? {
            return Err(StoreError::Unsupported("path is a file".into()));
        }
        let storage = self.storage()?;
        let mut prefix = String::new();
        storage
            .set_item(&self.dir_key(""), "")
            .map_err(|_| StoreError::Io("localStorage.setItem failed (quota?)".into()))?;
        if path.is_empty() {
            return Ok(());
        }
        for component in path.split('/') {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            storage
                .set_item(&self.dir_key(&prefix), "")
                .map_err(|_| StoreError::Io("localStorage.setItem failed (quota?)".into()))?;
        }
        Ok(())
    }

    fn size(&self, path: &str) -> Result<Option<u64>, StoreError> {
        validate_path(path)?;
        if let Some(value) = self
            .storage()?
            .get_item(&self.file_key(path))
            .map_err(|_| StoreError::Io("localStorage.getItem failed".into()))?
        {
            return Ok(Some(value.len() as u64));
        }
        if path.is_empty() || self.is_dir(path)? {
            return Ok(Some(0));
        }
        Ok(None)
    }
}

impl HostAccess for WebHost {
    fn dispose(&mut self) {
        #[cfg(target_arch = "wasm32")]
        self.release_app_audio();
    }

    fn call(&mut self, request: &HostRequest, args: &[Value]) -> Result<Option<Value>, VmError> {
        use micro_ir::HostCallKind::*;
        Ok(match request.kind {
            DeviceName => Some(Value::String("micro-os".into())),
            DeviceChip => Some(Value::String("ESP32-S3 (sim)".into())),
            DeviceFlashBytes | DevicePsramBytes => Some(Value::Number(8.0 * 1024.0 * 1024.0)),
            DeviceResetReason => Some(Value::String("power-on (sim)".into())),
            DeviceBacklight => Some(Value::Number(f64::from(self.backlight))),
            DeviceSetBacklight => {
                if let Some(Value::Number(level)) = args.first() {
                    self.backlight = level.clamp(0.0, 4.0) as u8;
                }
                None
            }
            DeviceGpioSetup => {
                let pin = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                self.gpio.entry(pin).or_insert(0);
                None
            }
            DeviceGpioWrite => {
                let pin = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                let level = numeric_arg(args, 1).unwrap_or(0.0) as u8;
                self.gpio.insert(pin, level);
                None
            }
            DeviceGpioRead => {
                let pin = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                Some(Value::Number(f64::from(
                    self.gpio.get(&pin).copied().unwrap_or(0),
                )))
            }
            NetWifiState => Some(Value::String("connected".into())),
            NetWifiSsid => Some(Value::String("micro-demo".into())),
            NetWifiConnect | NetWifiDisconnect => None,
            NetScanWifi => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("net.scanWifi has no callback".into()))?;
                let list = "micro-demo\nguest\nmicro-os";
                self.scan_aps = list.split('\n').map(str::to_owned).collect();
                self.pending.push((callback, Value::String(list.into())));
                None
            }
            NetWifiApName => {
                let index = numeric_arg(args, 0).unwrap_or(0.0) as usize;
                Some(Value::String(
                    self.scan_aps.get(index).cloned().unwrap_or_default(),
                ))
            }
            NetHttpGet => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("net.httpGet has no callback".into()))?;
                self.pending.push((
                    callback,
                    Value::String("HTTP 200\nHello from the web simulator".into()),
                ));
                None
            }
            NetHttpRequest => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("net.httpRequest has no callback".into()))?;
                let method = string_arg(args, 0);
                let url = string_arg(args, 1);
                self.pending.push((
                    callback,
                    Value::String(format!("HTTP 200\n{method} {url} (web simulator)")),
                ));
                None
            }
            NetHttpRequestWithHeaders => {
                let callback = request.callback.ok_or_else(|| {
                    VmError::Host("net.httpRequestWithHeaders has no callback".into())
                })?;
                self.pending.push((
                    callback,
                    Value::String(
                        "HTTP 501\n{\"errors\":[{\"msg\":\"此宿主暂不支持自定义请求头\"}]}".into(),
                    ),
                ));
                None
            }
            NetLookupWords => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("net.lookupWords has no callback".into()))?;
                self.pending.push((
                    callback,
                    Value::String(
                        "HTTP 501\n{\"errors\":[{\"msg\":\"此宿主暂不支持在线词典\"}]}".into(),
                    ),
                ));
                None
            }
            JsonGetText => {
                let text = string_arg(args, 0);
                let path = string_arg(args, 1);
                Some(Value::String(micro_json::get_text(&text, &path)))
            }
            JsonGetNumber => {
                let text = string_arg(args, 0);
                let path = string_arg(args, 1);
                Some(Value::Number(micro_json::get_number(&text, &path)))
            }
            JsonLength => {
                let text = string_arg(args, 0);
                let path = string_arg(args, 1);
                Some(Value::Number(micro_json::length(&text, &path)))
            }
            ClockNow => {
                #[cfg(target_arch = "wasm32")]
                {
                    Some(Value::Number(js_sys::Date::now()))
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    Some(Value::Number(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|duration| duration.as_millis() as f64)
                            .unwrap_or(0.0),
                    ))
                }
            }
            KvGetNumber => {
                let key = string_arg(args, 0);
                Some(match self.kv_get(&key) {
                    Some(KvValue::Number(value)) => Value::Number(value),
                    Some(KvValue::Bool(value)) => Value::Number(if value { 1.0 } else { 0.0 }),
                    Some(KvValue::String(text)) => {
                        Value::Number(text.parse::<f64>().unwrap_or(0.0))
                    }
                    _ => Value::Number(0.0),
                })
            }
            KvGetText => {
                let key = string_arg(args, 0);
                Some(match self.kv_get(&key) {
                    Some(KvValue::String(text)) => Value::String(text),
                    Some(KvValue::Number(value)) => Value::String(value.to_string()),
                    Some(KvValue::Bool(value)) => Value::String(value.to_string()),
                    _ => Value::String(String::new()),
                })
            }
            KvSetNumber => {
                let key = string_arg(args, 0);
                let value = numeric_arg(args, 1).unwrap_or(0.0);
                self.kv_set(&key, KvValue::Number(value));
                None
            }
            KvSetText => {
                let key = string_arg(args, 0);
                let value = string_arg(args, 1);
                self.kv_set(&key, KvValue::String(value));
                None
            }
            KvRemove => {
                let key = string_arg(args, 0);
                self.kv_remove(&key);
                None
            }
            ConfigGetNumber => {
                let key = string_arg(args, 0);
                Some(as_number(self.config_get(&key)))
            }
            ConfigGetText => {
                let key = string_arg(args, 0);
                Some(as_text(self.config_get(&key)))
            }
            ConfigSetNumber => {
                let key = string_arg(args, 0);
                let value = numeric_arg(args, 1).unwrap_or(0.0);
                self.config_set(&key, KvValue::Number(value));
                None
            }
            ConfigSetText => {
                let key = string_arg(args, 0);
                let value = string_arg(args, 1);
                self.config_set(&key, KvValue::String(value));
                None
            }
            ConfigRemove => {
                let key = string_arg(args, 0);
                self.config_remove(&key);
                None
            }
            StorageAvailable => Some(Value::Bool(
                self.files.as_ref().is_some_and(|files| files.available()),
            )),
            StorageExists => {
                let path = string_arg(args, 0);
                Some(Value::Bool(
                    self.files
                        .as_ref()
                        .and_then(|files| files.exists(&path).ok())
                        .unwrap_or(false),
                ))
            }
            StorageReadText => {
                let path = string_arg(args, 0);
                Some(Value::String(
                    self.files
                        .as_ref()
                        .and_then(|files| files.read_text(&path).ok().flatten())
                        .unwrap_or_default(),
                ))
            }
            StorageWriteText => {
                let path = string_arg(args, 0);
                let text = string_arg(args, 1);
                let written = self
                    .files
                    .as_mut()
                    .is_some_and(|files| files.write_text(&path, &text).is_ok());
                Some(Value::Bool(written))
            }
            StorageAppendText => {
                let path = string_arg(args, 0);
                let text = string_arg(args, 1);
                let appended = self
                    .files
                    .as_mut()
                    .is_some_and(|files| files.append_text(&path, &text).is_ok());
                Some(Value::Bool(appended))
            }
            StorageList => {
                let path = string_arg(args, 0);
                let listing = self
                    .files
                    .as_ref()
                    .and_then(|files| files.list(&path).ok())
                    .map(|entries| entries.join("\n"))
                    .unwrap_or_default();
                Some(Value::String(listing))
            }
            StorageRemove => {
                let path = string_arg(args, 0);
                let removed = self
                    .files
                    .as_mut()
                    .and_then(|files| files.remove(&path).ok())
                    .unwrap_or(false);
                Some(Value::Bool(removed))
            }
            StorageMkdir => {
                let path = string_arg(args, 0);
                let created = self
                    .files
                    .as_mut()
                    .is_some_and(|files| files.mkdir(&path).is_ok());
                Some(Value::Bool(created))
            }
            StorageSize => {
                let path = string_arg(args, 0);
                let size = self
                    .files
                    .as_ref()
                    .and_then(|files| files.size(&path).ok().flatten())
                    .map_or(-1.0, |bytes| bytes as f64);
                Some(Value::Number(size))
            }
            AudioSpeak => {
                let text = string_arg(args, 0);
                #[cfg(target_arch = "wasm32")]
                speak_web(&text, self.speech_rate());
                let _ = text;
                None
            }
            AudioStop => {
                #[cfg(target_arch = "wasm32")]
                stop_web();
                None
            }
            AudioSetRate => {
                let rate = numeric_arg(args, 0).unwrap_or(0.9);
                if rate.is_finite() {
                    self.speech_rate = Some(rate.clamp(0.5, 2.0));
                }
                None
            }
            AudioPlayUrl => {
                let url = string_arg(args, 0);
                #[cfg(target_arch = "wasm32")]
                return Ok(Some(Value::Number(self.play_url_audio(&url))));
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = url;
                    Some(Value::Number(0.0))
                }
            }
            AudioStopUrl => {
                let handle = numeric_arg(args, 0).unwrap_or(0.0);
                #[cfg(target_arch = "wasm32")]
                if handle > 0.0 {
                    self.stop_url_audio(handle as u32);
                }
                #[cfg(not(target_arch = "wasm32"))]
                let _ = handle;
                None
            }
            AudioStopAllUrls => {
                #[cfg(target_arch = "wasm32")]
                self.stop_all_url_audio();
                None
            }
            AudioPlayBackground => {
                let url = string_arg(args, 0);
                #[cfg(target_arch = "wasm32")]
                play_background_audio(&url);
                let _ = url;
                None
            }
            AudioStopBackground => {
                #[cfg(target_arch = "wasm32")]
                stop_background_audio();
                None
            }
            OsAppName => {
                let index = numeric_arg(args, 0).unwrap_or(0.0) as usize;
                Some(Value::String(
                    self.nav
                        .borrow()
                        .apps
                        .get(index)
                        .map_or("", |app| &app.0)
                        .to_owned(),
                ))
            }
            OsAppIcon => {
                let index = numeric_arg(args, 0).unwrap_or(0.0) as usize;
                Some(Value::String(
                    self.nav
                        .borrow()
                        .apps
                        .get(index)
                        .map_or("", |app| &app.1)
                        .to_owned(),
                ))
            }
            OsLaunchIndex => {
                self.nav.borrow_mut().pending_launch =
                    Some(numeric_arg(args, 0).unwrap_or(0.0) as u32);
                None
            }
            OsGoBack => {
                self.nav.borrow_mut().pending_back = true;
                None
            }
            OsDelay => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("os.delay has no callback".into()))?;
                // Sim: complete on the next tick (real timing wired via JS
                // setTimeout in the Web Player integration).
                self.pending.push((callback, Value::String(String::new())));
                None
            }
        })
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        std::mem::take(&mut self.pending)
    }
}

/// The browser's speech synthesizer, or `None` when the API is unavailable.
#[cfg(target_arch = "wasm32")]
fn web_speech() -> Option<web_sys::SpeechSynthesis> {
    web_sys::window()?.speech_synthesis().ok()
}

/// Prefer an installed en-US voice (Samantha keeps the desktop voice), then
/// any English voice. Chrome returns an empty list until `voiceschanged`, in
/// which case the utterance's `lang` still selects a sensible default.
#[cfg(target_arch = "wasm32")]
fn english_voice(synth: &web_sys::SpeechSynthesis) -> Option<web_sys::SpeechSynthesisVoice> {
    use wasm_bindgen::JsCast;

    let mut fallback = None;
    for value in synth.get_voices().iter() {
        let Ok(voice) = value.dyn_into::<web_sys::SpeechSynthesisVoice>() else {
            continue;
        };
        let lang = voice.lang().to_ascii_lowercase();
        if lang == "en-us" {
            if voice.name().to_ascii_lowercase().contains("samantha") {
                return Some(voice);
            }
            if fallback.is_none() {
                fallback = Some(voice);
            }
        } else if fallback.is_none() && lang.starts_with("en") {
            fallback = Some(voice);
        }
    }
    fallback
}

/// Replace any utterance in flight with `text`, matching the native host's
/// "the word on screen is the word you hear" behavior.
#[cfg(target_arch = "wasm32")]
fn speak_web(text: &str, rate: f64) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let Some(synth) = web_speech() else {
        return;
    };
    /* Chrome can leave the queue paused after a cancelled utterance. */
    if synth.paused() {
        synth.resume();
    }
    if synth.speaking() || synth.pending() {
        synth.cancel();
    }
    let Ok(utterance) = web_sys::SpeechSynthesisUtterance::new_with_text(text) else {
        return;
    };
    utterance.set_lang("en-US");
    utterance.set_rate(rate as f32);
    utterance.set_pitch(1.0);
    utterance.set_volume(1.0);
    if let Some(voice) = english_voice(&synth) {
        utterance.set_voice(Some(&voice));
    }
    synth.speak(&utterance);
}

#[cfg(target_arch = "wasm32")]
fn stop_web() {
    if let Some(synth) = web_speech() {
        synth.cancel();
    }
}

fn numeric_arg(args: &[Value], index: usize) -> Option<f64> {
    match args.get(index) {
        Some(Value::Number(value)) => Some(*value),
        _ => None,
    }
}

fn string_arg(args: &[Value], index: usize) -> String {
    match args.get(index) {
        Some(Value::String(value)) => value.clone(),
        _ => String::new(),
    }
}

fn as_number(value: Option<KvValue>) -> Value {
    match value {
        Some(KvValue::Number(value)) => Value::Number(value),
        Some(KvValue::Bool(value)) => Value::Number(if value { 1.0 } else { 0.0 }),
        Some(KvValue::String(text)) => Value::Number(text.parse::<f64>().unwrap_or(0.0)),
        _ => Value::Number(0.0),
    }
}

fn as_text(value: Option<KvValue>) -> Value {
    match value {
        Some(KvValue::String(text)) => Value::String(text),
        Some(KvValue::Number(value)) => Value::String(value.to_string()),
        Some(KvValue::Bool(value)) => Value::String(value.to_string()),
        _ => Value::String(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::{WebKv, valid_audio_url};
    use micro_store::{KvValue, ScopedKv};

    #[test]
    fn audio_urls_are_limited_to_http_and_https() {
        assert_eq!(valid_audio_url(""), None);
        assert_eq!(valid_audio_url("   "), None);
        assert_eq!(valid_audio_url("file:///tmp/a.mp3"), None);
        assert_eq!(valid_audio_url("https://example.com/a b.mp3"), None);
        assert_eq!(
            valid_audio_url("https://example.com/a.mp3"),
            Some("https://example.com/a.mp3")
        );
        assert_eq!(
            valid_audio_url("HTTP://example.com/a.mp3"),
            Some("HTTP://example.com/a.mp3")
        );
    }

    #[test]
    fn scoped_kv_keeps_types_and_removes_keys() {
        let mut kv = WebKv::new("momo");
        assert_eq!(kv.get("s0").unwrap(), None);
        kv.set("s0", &KvValue::Number(3.0)).unwrap();
        kv.set("word", &KvValue::String("abandon".into())).unwrap();
        assert_eq!(kv.get("s0").unwrap(), Some(KvValue::Number(3.0)));
        assert_eq!(
            kv.get("word").unwrap(),
            Some(KvValue::String("abandon".into()))
        );
        kv.remove("s0").unwrap();
        assert_eq!(kv.get("s0").unwrap(), None);
    }

    #[test]
    fn scoped_kv_rejects_non_finite_numbers() {
        let mut kv = WebKv::new("momo");
        assert!(kv.set("bad", &KvValue::Number(f64::NAN)).is_err());
    }
}
