//! Native host capabilities for the desktop SDL demo.
//!
//! `device.*` and the Wi-Fi reads remain simulator values. All `net.*` HTTP
//! calls (`httpGet`, `httpRequest`, `httpRequestWithHeaders`) and
//! `net.lookupWords` are real: they run the in-process [`micro_net`] transport
//! (ureq + rustls) on a worker thread, never an external `curl` process, so the
//! LVGL loop stays responsive and the host does not depend on a shell.
//!
//! `os.*` navigation intents (launch an installed app / go back to the shell)
//! are written into a shared [`ShellState`] that the main loop polls, mirroring
//! the pending-intent pattern the ESP32 host uses on the C side.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use micro_ir::{FunctionId, HostRequest};
use micro_net::native::NativeTransport;
use micro_net::{HttpRequest, envelope};
use micro_store::{
    FileStore, KvStore, KvValue, NativeConfigStore, NativeFileStore, NativeStore, ScopedFiles,
    ScopedKv,
};
use micro_vm::{HostAccess, Value, VmError};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use crate::speech::Speech;
use crate::url_audio::UrlAudio;

/// Shared OS-shell state, polled by the SDL main loop each tick. The app
/// registry lives here too so the main loop can inject the decoded app list
/// without reaching through the boxed `NativeHost`.
#[derive(Default)]
pub struct ShellState {
    /// The installed-app index requested via `os.launchIndex`.
    pub pending_launch: Option<u32>,
    /// Whether the current app requested `os.goBack`.
    pub pending_back: bool,
    /// Title requested via `os.setTitle`, applied by the main loop.
    pub pending_title: Option<String>,
    /// Installed-app registry: `(name, icon)` per index.
    pub apps: Vec<(String, String)>,
    /// Suppress system speech for headless smoke runs.
    pub silent: bool,
}

#[derive(Default)]
pub struct NativeHost {
    /// The current App's KV namespace, opened from the shared `micro-store`
    /// backend (`<root>/kv/<app>.json`).
    kv: Option<Box<dyn ScopedKv>>,
    /// The current App's config document (`<root>/<app>/config.json`).
    config: Option<Box<dyn ScopedKv>>,
    /// The current App's private directory on the simulated SD volume
    /// (`<root>/sdcard/micros/apps/<app>`).
    files: Option<Box<dyn ScopedFiles>>,
    backlight: u8,
    pending: Vec<(FunctionId, Value)>,
    /// Simulated GPIO output registers (pin → level) so read-after-write works.
    gpio: BTreeMap<u32, u8>,
    /// SSIDs from the most recent `net.scanWifi`, indexed by `net.wifiApName`.
    scan_aps: Vec<String>,
    /// Shared OS-shell state, drained by the SDL main loop.
    pub nav: Rc<RefCell<ShellState>>,
    /// Result receiver for the one in-flight HTTP request.
    http_rx: Option<(FunctionId, Receiver<String>)>,
    /// Result receiver for the one in-flight dictionary batch.
    dict_rx: Option<(FunctionId, Receiver<String>)>,
    /// System speech synthesizer state (`audio.*`).
    speech: Speech,
    /// Foreground URL audio and the background-music stream.
    url_audio: UrlAudio,
}

impl NativeHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the App's stores under the native root (`$MICROS_HOME` or
    /// `~/.micros`): KV at `<root>/kv/<app>.json`, the config document at
    /// `<root>/<app>/config.json`, and private files under
    /// `<root>/sdcard/micros/apps/<app>/`. A pre-`~/.micros` install is
    /// migrated from `~/.micrapp` first.
    pub fn new_for_app(app_id: &str) -> Self {
        let _ = micro_store::migrate_legacy_root();
        Self::new_for_app_at(micro_store::default_root(), app_id)
    }

    /// Like [`Self::new_for_app`] but rooted at an explicit store directory.
    /// Useful for tests and portable/CI deployments that must not touch HOME.
    pub fn new_for_app_at(root: impl Into<std::path::PathBuf>, app_id: &str) -> Self {
        let root = root.into();
        let app_id = if app_id.is_empty() { "app" } else { app_id };
        let kv = NativeStore::new(&root).open(app_id).ok();
        let config = NativeConfigStore::new(&root).open(app_id).ok();
        let files = NativeFileStore::new(root.join("sdcard")).open(app_id).ok();
        Self {
            kv,
            config,
            files,
            /* The firmware boots the panel at level 3
             * (`micro_backlight_apply(3)` in main.c); the desktop host mirrors
             * that so `device.backlight()` reports a lit panel instead of 0
             * (which on the real board means "panel off"). */
            backlight: 3,
            ..Self::default()
        }
    }

    /// Silence system speech and URL audio for headless tests and `--smoke`
    /// runs. Keeps the same host API but never opens an audio device.
    pub fn silent(mut self) -> Self {
        self.speech.set_enabled(false);
        self.url_audio.set_enabled(false);
        self
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
        // Pre-v22 Apps kept config-like values (momo's token and goal) in
        // `kv`. Read those values until the first config write moves them.
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

    /// Validates `net.*` arguments on the UI thread, then runs the request on
    /// a worker. Validation errors are delivered through the normal callback
    /// queue instead of blocking or panicking.
    fn start_http(
        &mut self,
        callback: FunctionId,
        method: &str,
        url: &str,
        headers: &str,
        body: &str,
    ) {
        if self.http_rx.is_some() {
            self.pending.push((
                callback,
                Value::String(envelope(0, "another request is still running")),
            ));
            return;
        }
        let request = match HttpRequest::parse(method, url, headers, body) {
            Ok(request) => request,
            Err(error) => {
                self.pending
                    .push((callback, Value::String(envelope(0, &error))));
                return;
            }
        };
        let (sender, receiver) = channel();
        self.http_rx = Some((callback, receiver));
        let spawned = std::thread::Builder::new()
            .name("micro-http".into())
            .spawn(move || {
                let response = micro_net::perform_request(&NativeTransport::new(), &request);
                let _ = sender.send(response);
            });
        if let Err(error) = spawned {
            self.http_rx = None;
            self.pending.push((
                callback,
                Value::String(envelope(0, &format!("cannot start HTTP worker: {error}"))),
            ));
        }
    }

    fn start_dictionary(&mut self, callback: FunctionId, words: &str) {
        if self.dict_rx.is_some() {
            self.pending.push((
                callback,
                Value::String(envelope(0, "a dictionary lookup is still running")),
            ));
            return;
        }
        let words = words.to_owned();
        let (sender, receiver) = channel();
        self.dict_rx = Some((callback, receiver));
        let spawned = std::thread::Builder::new()
            .name("micro-dictionary".into())
            .spawn(move || {
                let response = micro_net::dictionary::lookup_words(&NativeTransport::new(), &words);
                let _ = sender.send(response);
            });
        if let Err(error) = spawned {
            self.dict_rx = None;
            self.pending.push((
                callback,
                Value::String(envelope(
                    0,
                    &format!("cannot start dictionary worker: {error}"),
                )),
            ));
        }
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

impl HostAccess for NativeHost {
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
                let url = string_arg(args, 0);
                self.start_http(callback, "GET", &url, "", "");
                None
            }
            NetHttpRequest => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("net.httpRequest has no callback".into()))?;
                let method = string_arg(args, 0);
                let url = string_arg(args, 1);
                let body = string_arg(args, 2);
                self.start_http(callback, &method, &url, "", &body);
                None
            }
            NetHttpRequestWithHeaders => {
                let callback = request.callback.ok_or_else(|| {
                    VmError::Host("net.httpRequestWithHeaders has no callback".into())
                })?;
                let method = string_arg(args, 0);
                let url = string_arg(args, 1);
                let headers = string_arg(args, 2);
                let body = string_arg(args, 3);
                self.start_http(callback, &method, &url, &headers, &body);
                None
            }
            NetLookupWords => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("net.lookupWords has no callback".into()))?;
                let words = string_arg(args, 0);
                self.start_dictionary(callback, &words);
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
            ClockNow => Some(Value::Number(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis() as f64)
                    .unwrap_or(0.0),
            )),
            KvGetNumber => {
                let key = string_arg(args, 0);
                Some(as_number(self.kv_get(&key)))
            }
            KvGetText => {
                let key = string_arg(args, 0);
                Some(as_text(self.kv_get(&key)))
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
                self.speech.speak(&string_arg(args, 0));
                None
            }
            AudioStop => {
                self.speech.stop();
                None
            }
            AudioSetRate => {
                self.speech.set_rate(numeric_arg(args, 0).unwrap_or(0.9));
                None
            }
            AudioPlayUrl => {
                let handle = self.url_audio.play(&string_arg(args, 0));
                Some(Value::Number(f64::from(handle)))
            }
            AudioStopUrl => {
                self.url_audio
                    .stop(numeric_arg(args, 0).unwrap_or(0.0) as i32);
                None
            }
            AudioStopAllUrls => {
                self.url_audio.stop_all();
                None
            }
            AudioPlayBackground => {
                self.url_audio.play_background(&string_arg(args, 0));
                None
            }
            AudioStopBackground => {
                self.url_audio.stop_background();
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
            OsSetTitle => {
                self.nav.borrow_mut().pending_title = Some(string_arg(args, 0));
                None
            }
            OsDelay => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("os.delay has no callback".into()))?;
                // Sim: complete on the next tick (real timing wired in the SDL
                // main loop's tick budget).
                self.pending.push((callback, Value::String(String::new())));
                None
            }
        })
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        let mut results = std::mem::take(&mut self.pending);
        drain_worker(&mut self.http_rx, &mut results, "HTTP worker stopped");
        drain_worker(&mut self.dict_rx, &mut results, "dictionary worker stopped");
        results
    }
}

fn drain_worker(
    slot: &mut Option<(FunctionId, Receiver<String>)>,
    results: &mut Vec<(FunctionId, Value)>,
    stopped: &str,
) {
    let Some((callback, receiver)) = slot.take() else {
        return;
    };
    match receiver.try_recv() {
        Ok(response) => results.push((callback, Value::String(response))),
        Err(TryRecvError::Empty) => *slot = Some((callback, receiver)),
        Err(TryRecvError::Disconnected) => results.push((
            callback,
            Value::String(envelope(
                0,
                &format!("{stopped} before returning a response"),
            )),
        )),
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

/// Coerce a stored scalar to the SDK's numeric read shape.
fn as_number(value: Option<KvValue>) -> Value {
    match value {
        Some(KvValue::Number(value)) => Value::Number(value),
        Some(KvValue::Bool(value)) => Value::Number(if value { 1.0 } else { 0.0 }),
        Some(KvValue::String(text)) => Value::Number(text.parse::<f64>().unwrap_or(0.0)),
        _ => Value::Number(0.0),
    }
}

/// Coerce a stored scalar to the SDK's textual read shape.
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
    use super::NativeHost;
    use micro_ir::{FunctionId, HostCallKind, HostRequest, ScalarType};
    use micro_vm::{HostAccess, Value};

    fn request(kind: HostCallKind, arg_kinds: Vec<ScalarType>) -> HostRequest {
        HostRequest::sync(kind, arg_kinds, None)
    }

    #[test]
    fn kv_survives_a_new_host_for_the_same_app() {
        let dir = std::env::temp_dir().join(format!(
            "micros-native-kv-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let mut first = NativeHost::new_for_app_at(&dir, "momo");
        let write = request(
            HostCallKind::KvSetNumber,
            vec![ScalarType::String, ScalarType::Number],
        );
        first
            .call(&write, &[Value::String("s0".into()), Value::Number(3.0)])
            .expect("write");

        let mut second = NativeHost::new_for_app_at(&dir, "momo");
        let read = request(HostCallKind::KvGetNumber, vec![ScalarType::String]);
        let value = second
            .call(&read, &[Value::String("s0".into())])
            .expect("read");
        assert_eq!(value, Some(Value::Number(3.0)));
        drop(first);
        drop(second);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_survives_a_new_host_and_writes_the_app_config_file() {
        let dir = std::env::temp_dir().join(format!(
            "micros-native-config-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let mut first = NativeHost::new_for_app_at(&dir, "momo");
        first
            .call(
                &request(
                    HostCallKind::ConfigSetText,
                    vec![ScalarType::String, ScalarType::String],
                ),
                &[
                    Value::String("maimemoToken".into()),
                    Value::String("secret".into()),
                ],
            )
            .expect("write token");
        first
            .call(
                &request(
                    HostCallKind::ConfigSetNumber,
                    vec![ScalarType::String, ScalarType::Number],
                ),
                &[Value::String("goal".into()), Value::Number(8.0)],
            )
            .expect("write goal");

        assert!(dir.join("momo/config.json").is_file());

        let mut second = NativeHost::new_for_app_at(&dir, "momo");
        let token = second
            .call(
                &request(HostCallKind::ConfigGetText, vec![ScalarType::String]),
                &[Value::String("maimemoToken".into())],
            )
            .expect("read token");
        assert_eq!(token, Some(Value::String("secret".into())));
        let goal = second
            .call(
                &request(HostCallKind::ConfigGetNumber, vec![ScalarType::String]),
                &[Value::String("goal".into())],
            )
            .expect("read goal");
        assert_eq!(goal, Some(Value::Number(8.0)));

        drop(first);
        drop(second);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_promotes_legacy_kv_values_on_the_first_write() {
        let dir = std::env::temp_dir().join(format!(
            "micros-native-legacy-config-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        // A pre-v22 App (momo) kept its token and goal in the KV namespace.
        let mut seed = NativeHost::new_for_app_at(&dir, "momo");
        seed.call(
            &request(
                HostCallKind::KvSetText,
                vec![ScalarType::String, ScalarType::String],
            ),
            &[
                Value::String("maimemoToken".into()),
                Value::String("legacy".into()),
            ],
        )
        .expect("seed token");
        seed.call(
            &request(
                HostCallKind::KvSetNumber,
                vec![ScalarType::String, ScalarType::Number],
            ),
            &[Value::String("goal".into()), Value::Number(8.0)],
        )
        .expect("seed goal");

        // The config API reads the legacy values until the first write.
        let mut host = NativeHost::new_for_app_at(&dir, "momo");
        let token = host
            .call(
                &request(HostCallKind::ConfigGetText, vec![ScalarType::String]),
                &[Value::String("maimemoToken".into())],
            )
            .expect("read legacy token");
        assert_eq!(token, Some(Value::String("legacy".into())));
        let goal = host
            .call(
                &request(HostCallKind::ConfigGetNumber, vec![ScalarType::String]),
                &[Value::String("goal".into())],
            )
            .expect("read legacy goal");
        assert_eq!(goal, Some(Value::Number(8.0)));

        // Writing a key moves it from KV into the App's config document.
        host.call(
            &request(
                HostCallKind::ConfigSetText,
                vec![ScalarType::String, ScalarType::String],
            ),
            &[
                Value::String("maimemoToken".into()),
                Value::String("fresh".into()),
            ],
        )
        .expect("write token");
        let kv = std::fs::read_to_string(dir.join("kv/momo.json")).expect("kv file");
        assert!(!kv.contains("maimemoToken"), "{kv}");
        let config = std::fs::read_to_string(dir.join("momo/config.json")).expect("config file");
        assert!(config.contains("fresh"), "{config}");

        let mut reloaded = NativeHost::new_for_app_at(&dir, "momo");
        let token = reloaded
            .call(
                &request(HostCallKind::ConfigGetText, vec![ScalarType::String]),
                &[Value::String("maimemoToken".into())],
            )
            .expect("reload token");
        assert_eq!(token, Some(Value::String("fresh".into())));
        let goal = reloaded
            .call(
                &request(HostCallKind::ConfigGetNumber, vec![ScalarType::String]),
                &[Value::String("goal".into())],
            )
            .expect("reload goal");
        assert_eq!(
            goal,
            Some(Value::Number(8.0)),
            "keys that were not rewritten still read the legacy value"
        );

        drop(seed);
        drop(host);
        drop(reloaded);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn audio_calls_are_safe_when_silent() {
        let mut host = NativeHost::new().silent();
        host.call(
            &request(HostCallKind::AudioSetRate, vec![ScalarType::Number]),
            &[Value::Number(9.0)],
        )
        .expect("set rate");
        host.call(&request(HostCallKind::AudioStop, vec![]), &[])
            .expect("stop");
        host.call(
            &request(HostCallKind::AudioSpeak, vec![ScalarType::String]),
            &[Value::String("abandon".into())],
        )
        .expect("speak");
        let played = host
            .call(
                &request(HostCallKind::AudioPlayUrl, vec![ScalarType::String]),
                &[Value::String("https://example.com/word.mp3".into())],
            )
            .expect("play url");
        assert_eq!(played, Some(Value::Number(0.0)));
        host.call(
            &request(HostCallKind::AudioStopUrl, vec![ScalarType::Number]),
            &[Value::Number(1.0)],
        )
        .expect("stop url");
        host.call(&request(HostCallKind::AudioStopAllUrls, vec![]), &[])
            .expect("stop all urls");
        host.call(
            &request(HostCallKind::AudioPlayBackground, vec![ScalarType::String]),
            &[Value::String("https://example.com/bgm.mp3".into())],
        )
        .expect("play background");
        host.call(&request(HostCallKind::AudioStopBackground, vec![]), &[])
            .expect("stop background");
    }

    #[test]
    fn header_aware_http_runs_in_process() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 4096];
            let mut request = String::new();
            loop {
                let read = stream.read(&mut buffer).expect("read");
                if read == 0 {
                    break;
                }
                request.push_str(&String::from_utf8_lossy(&buffer[..read]));
                if request.contains("{\"probe\":true}") {
                    break;
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
                .expect("write");
            request
        });

        let mut host = NativeHost::new();
        let call = HostRequest::async_request(
            HostCallKind::NetHttpRequestWithHeaders,
            vec![
                ScalarType::String,
                ScalarType::String,
                ScalarType::String,
                ScalarType::String,
            ],
            FunctionId(1),
        );
        host.call(
            &call,
            &[
                Value::String("POST".into()),
                Value::String(format!("http://{address}/probe")),
                Value::String("Authorization: Bearer test-token\nAccept: application/json".into()),
                Value::String("{\"probe\":true}".into()),
            ],
        )
        .expect("request");

        let mut response = None;
        for _ in 0..200 {
            if let Some((_, value)) = host.drain_results().into_iter().next() {
                response = Some(value);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let Some(Value::String(response)) = response else {
            panic!("HTTP request did not complete");
        };
        let request = server.join().expect("server");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token"),
            "{request}"
        );
        assert!(request.contains("{\"probe\":true}"), "{request}");
        assert_eq!(response, "HTTP 200\nOK");
    }
}
