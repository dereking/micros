//! ESP32 host capabilities for the app SDK (`device.*` / `net.*`).
//!
//! Device reads call the `micro_esp_host_*` C exports in
//! `micro_runtime_ffi/micro_esp_host.c`, which read real IDF values
//! (`esp_flash_get_size`, `esp_psram_get_size`, `esp_reset_reason`) and a plain
//! C backlight global mirrored by `main.c`. Wi-Fi state/SSID/scan read the real
//! STA radio through the `micro_wifi` component; connect/disconnect set pending
//! intents that `main.c` drains into the radio on its next tick.
//!
//! `net.scanWifi` is genuinely async: it starts a real radio scan and the
//! callback fires via `drain_results` once `WIFI_EVENT_SCAN_DONE` completes it.
//! Every `net.*` HTTP call now runs the in-process `esp_http_client` transport
//! (HTTPS + certificate bundle + custom headers) on a Rust worker thread, so
//! the LVGL task never blocks and the firmware never shells out to an external
//! command. `net.lookupWords` uses the same transport for its dictionary batch.

use std::ffi::c_int;
use std::ptr;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use micro_ir::{FunctionId, HostRequest};
use micro_net::{HttpRequest, HttpResponse, Transport, envelope};
use micro_store::{KvValue, ScopedFiles, ScopedKv};
use micro_vm::{HostAccess, Value, VmError};

use crate::nvs::{NvsConfigKv, NvsKv};

/* The C `micro_esp_host_*` functions take `char*`; on the espidf target
 * Rust's `c_char` is `u8`, so the byte pointers line up directly. */
unsafe extern "C" {
    fn micro_esp_host_device_name(buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_device_chip(buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_device_flash_bytes(out: *mut u32) -> c_int;
    fn micro_esp_host_device_psram_bytes(out: *mut u32) -> c_int;
    fn micro_esp_host_device_reset_reason(buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_backlight(out: *mut u32) -> c_int;
    fn micro_esp_host_set_backlight(level: u32) -> c_int;
    fn micro_esp_host_wifi_state(buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_wifi_ssid(buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_wifi_connect(
        ssid: *const u8,
        ssid_len: usize,
        pass: *const u8,
        pass_len: usize,
    ) -> c_int;
    fn micro_esp_host_wifi_disconnect() -> c_int;
    fn micro_esp_host_scan_start() -> c_int;
    fn micro_esp_host_take_scan_result(buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_wifi_ap_name(index: u32, buf: *mut u8, cap: usize) -> c_int;
    #[allow(clippy::too_many_arguments)]
    fn micro_esp_host_http_request(
        method: *const u8,
        method_len: usize,
        url: *const u8,
        url_len: usize,
        headers: *const u8,
        headers_len: usize,
        body: *const u8,
        body_len: usize,
        status: *mut c_int,
        response: *mut *mut u8,
        response_len: *mut usize,
    ) -> c_int;
    fn micro_esp_host_http_free(response: *mut u8);
    fn micro_esp_host_gpio_setup(pin: u32, mode: *const u8, mode_len: usize) -> c_int;
    fn micro_esp_host_gpio_write(pin: u32, level: u32) -> c_int;
    fn micro_esp_host_gpio_read(pin: u32) -> c_int;
    fn micro_esp_host_app_name(index: u32, buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_app_icon(index: u32, buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_set_launch_index(index: u32);
    fn micro_esp_host_set_back_intent();
    fn micro_esp_host_set_title(title: *const u8, len: usize);
    fn micro_esp_host_uptime_ms() -> u32;
    fn micro_clock_wall_ms() -> u64;
}

/// Reads a NUL-terminated string produced by a `micro_esp_host_*` export.
fn read_c_string(read: unsafe extern "C" fn(*mut u8, usize) -> c_int) -> String {
    let mut buffer = [0_u8; 128];
    if unsafe { read(buffer.as_mut_ptr(), buffer.len()) } != 0 {
        return String::new();
    }
    let len = buffer
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..len]).into_owned()
}

/// Reads a NUL-terminated string from an indexed `micro_esp_host_*` export.
fn read_c_string_arg(
    read: unsafe extern "C" fn(u32, *mut u8, usize) -> c_int,
    index: u32,
) -> String {
    let mut buffer = [0_u8; 64];
    if unsafe { read(index, buffer.as_mut_ptr(), buffer.len()) } != 0 {
        return String::new();
    }
    let len = buffer
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..len]).into_owned()
}

fn string_arg(args: &[Value], index: usize) -> String {
    match args.get(index) {
        Some(Value::String(value)) => value.clone(),
        _ => String::new(),
    }
}

fn numeric_arg(args: &[Value], index: usize) -> Option<f64> {
    match args.get(index) {
        Some(Value::Number(value)) => Some(*value),
        _ => None,
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

/// The firmware's esp_http_client bridge. Each call blocks, so the host runs it
/// on a dedicated Rust thread (see `start_http`).
struct EspHttpTransport;

impl Transport for EspHttpTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        let mut headers = String::new();
        for (name, value) in &request.headers {
            headers.push_str(name);
            headers.push_str(": ");
            headers.push_str(value);
            headers.push('\n');
        }

        let mut status: c_int = 0;
        let mut response: *mut u8 = ptr::null_mut();
        let mut response_len: usize = 0;
        let accepted = unsafe {
            micro_esp_host_http_request(
                request.method.as_ptr(),
                request.method.len(),
                request.url.as_ptr(),
                request.url.len(),
                headers.as_ptr(),
                headers.len(),
                request.body.as_ptr(),
                request.body.len(),
                &mut status,
                &mut response,
                &mut response_len,
            )
        };
        if accepted != 0 {
            return Err("the firmware rejected the HTTP request".into());
        }

        let body = if response.is_null() {
            String::new()
        } else {
            let bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
            let text = String::from_utf8_lossy(bytes).into_owned();
            unsafe { micro_esp_host_http_free(response) };
            text
        };

        if !(0..=i32::from(u16::MAX)).contains(&status) {
            return Err("the firmware returned an invalid HTTP status".into());
        }
        Ok(HttpResponse::new(status as u16, body))
    }
}

pub struct EspHost {
    /// The current App's KV namespace, backed by NVS through `micro-store`.
    /// NVS survives reboots and is isolated by App id.
    kv: Option<Box<dyn ScopedKv>>,
    /// The current App's config document, one NVS string in the shared
    /// `micro_config` namespace.
    config: Option<Box<dyn ScopedKv>>,
    /// The current App's private directory on the external SD card.
    files: Option<Box<dyn ScopedFiles>>,
    pending: Vec<(FunctionId, Value)>,
    /// `os.delay` callbacks waiting for their deadline (uptime ms).
    pending_delays: Vec<(u32, FunctionId)>,
    /// `net.scanWifi` callback waiting for the async radio scan to finish.
    scan_callback: Option<FunctionId>,
    /// Receiver for the one in-flight HTTP request.
    http_rx: Option<(FunctionId, Receiver<String>)>,
    /// Receiver for the one in-flight dictionary batch.
    dict_rx: Option<(FunctionId, Receiver<String>)>,
}

impl EspHost {
    pub fn new() -> Self {
        Self {
            kv: None,
            config: None,
            files: None,
            pending: Vec::new(),
            pending_delays: Vec::new(),
            scan_callback: None,
            http_rx: None,
            dict_rx: None,
        }
    }

    /// Open the App's NVS namespace (the App id) for persistent KV data, its
    /// config document in `micro_config`, and its private SD directory under
    /// `/sdcard/micros/apps/<app-id>/`.
    pub fn new_for_app(app_id: &str) -> Self {
        let app_id = if app_id.is_empty() { "app" } else { app_id };
        Self {
            kv: Some(Box::new(NvsKv::new(app_id))),
            config: Some(Box::new(NvsConfigKv::new(app_id))),
            files: crate::sd::SdFiles::new(app_id)
                .map(|files| Box::new(files) as Box<dyn ScopedFiles>),
            ..Self::new()
        }
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

    fn start_http_raw(
        &mut self,
        callback: FunctionId,
        method: &str,
        url: &str,
        headers: &str,
        body: &str,
    ) {
        match HttpRequest::parse(method, url, headers, body) {
            Ok(request) => self.start_http(callback, request),
            Err(error) => self
                .pending
                .push((callback, Value::String(envelope(0, &error)))),
        }
    }

    fn start_http(&mut self, callback: FunctionId, request: HttpRequest) {
        if self.http_rx.is_some() || self.dict_rx.is_some() {
            self.pending.push((
                callback,
                Value::String(envelope(0, "another network request is still running")),
            ));
            return;
        }
        let (sender, receiver) = channel();
        self.http_rx = Some((callback, receiver));
        let spawned = std::thread::Builder::new()
            .name("micro-http".into())
            .stack_size(32 * 1024)
            .spawn(move || {
                let response = micro_net::perform_request(&EspHttpTransport, &request);
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
        if self.http_rx.is_some() || self.dict_rx.is_some() {
            self.pending.push((
                callback,
                Value::String(envelope(0, "another network request is still running")),
            ));
            return;
        }
        let words = words.to_owned();
        let (sender, receiver) = channel();
        self.dict_rx = Some((callback, receiver));
        let spawned = std::thread::Builder::new()
            .name("micro-dictionary".into())
            .stack_size(32 * 1024)
            .spawn(move || {
                let response = micro_net::dictionary::lookup_words(&EspHttpTransport, &words);
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

impl HostAccess for EspHost {
    fn call(&mut self, request: &HostRequest, args: &[Value]) -> Result<Option<Value>, VmError> {
        use micro_ir::HostCallKind::*;
        Ok(match request.kind {
            DeviceName => Some(Value::String(read_c_string(micro_esp_host_device_name))),
            DeviceChip => Some(Value::String(read_c_string(micro_esp_host_device_chip))),
            DeviceFlashBytes => {
                let mut bytes = 0u32;
                let ok = unsafe { micro_esp_host_device_flash_bytes(&mut bytes) } == 0;
                Some(Value::Number(if ok { f64::from(bytes) } else { 0.0 }))
            }
            DevicePsramBytes => {
                let mut bytes = 0u32;
                let ok = unsafe { micro_esp_host_device_psram_bytes(&mut bytes) } == 0;
                Some(Value::Number(if ok { f64::from(bytes) } else { 0.0 }))
            }
            DeviceResetReason => Some(Value::String(read_c_string(
                micro_esp_host_device_reset_reason,
            ))),
            DeviceBacklight => {
                let mut level = 0u32;
                let ok = unsafe { micro_esp_host_backlight(&mut level) } == 0;
                Some(Value::Number(if ok { f64::from(level) } else { 0.0 }))
            }
            DeviceSetBacklight => {
                if let Some(Value::Number(level)) = args.first() {
                    unsafe { micro_esp_host_set_backlight(level.clamp(0.0, 4.0) as u32) };
                }
                None
            }
            DeviceGpioSetup => {
                let pin = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                let mode = string_arg(args, 1);
                unsafe { micro_esp_host_gpio_setup(pin, mode.as_ptr(), mode.len()) };
                None
            }
            DeviceGpioWrite => {
                let pin = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                let level = numeric_arg(args, 1).unwrap_or(0.0) as u32;
                unsafe { micro_esp_host_gpio_write(pin, level) };
                None
            }
            DeviceGpioRead => {
                let pin = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                let level = unsafe { micro_esp_host_gpio_read(pin) };
                Some(Value::Number(if level < 0 {
                    0.0
                } else {
                    f64::from(level)
                }))
            }
            NetWifiState => Some(Value::String(read_c_string(micro_esp_host_wifi_state))),
            NetWifiSsid => Some(Value::String(read_c_string(micro_esp_host_wifi_ssid))),
            NetWifiConnect => {
                let ssid = string_arg(args, 0);
                let pass = string_arg(args, 1);
                unsafe {
                    micro_esp_host_wifi_connect(
                        ssid.as_ptr(),
                        ssid.len(),
                        pass.as_ptr(),
                        pass.len(),
                    );
                }
                None
            }
            NetWifiDisconnect => {
                unsafe { micro_esp_host_wifi_disconnect() };
                None
            }
            NetScanWifi => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("net.scanWifi has no callback".into()))?;
                /* Kick the real STA scan; drain_results delivers the AP list
                 * to the callback once WIFI_EVENT_SCAN_DONE completes it. */
                unsafe { micro_esp_host_scan_start() };
                self.scan_callback = Some(callback);
                None
            }
            NetWifiApName => {
                let index = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                Some(Value::String(read_c_string_arg(
                    micro_esp_host_wifi_ap_name,
                    index,
                )))
            }
            NetHttpGet => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("net.httpGet has no callback".into()))?;
                let url = string_arg(args, 0);
                self.start_http_raw(callback, "GET", &url, "", "");
                None
            }
            NetHttpRequest => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("net.httpRequest has no callback".into()))?;
                let method = string_arg(args, 0);
                let url = string_arg(args, 1);
                let body = string_arg(args, 2);
                self.start_http_raw(callback, &method, &url, "", &body);
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
                self.start_http_raw(callback, &method, &url, &headers, &body);
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
            ClockNow => {
                /* Prefer SNTP wall time. Before the first sync, fall back to
                 * uptime so the app still schedules relative to boot instead of
                 * seeing a 1970 timestamp. */
                let wall = unsafe { micro_clock_wall_ms() };
                if wall > 0 {
                    Some(Value::Number(wall as f64))
                } else {
                    Some(Value::Number(f64::from(unsafe {
                        micro_esp_host_uptime_ms()
                    })))
                }
            }
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
                /* Speaker output is a board-profile capability; the V1.2 board
                 * has no codec wired yet, so pronunciation is a no-op here. */
                let _ = string_arg(args, 0);
                None
            }
            AudioStop | AudioSetRate => {
                /* Paired with the no-op above until an I2S/codec sink exists.
                 * Keeping the arms explicit means a future board only has to
                 * replace the `AudioSpeak` body. */
                None
            }
            AudioPlayUrl => {
                /* URL audio also needs the codec sink; a 0 handle tells the
                 * App nothing started, matching the silent `AudioSpeak`. */
                let _ = string_arg(args, 0);
                Some(Value::Number(0.0))
            }
            AudioPlayBackground => {
                /* No codec yet: accepting the call keeps the API uniform, but
                 * nothing is buffered or played in the background. */
                let _ = string_arg(args, 0);
                None
            }
            AudioStopUrl | AudioStopAllUrls | AudioStopBackground => None,
            OsAppName => {
                let index = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                Some(Value::String(read_c_string_arg(
                    micro_esp_host_app_name,
                    index,
                )))
            }
            OsAppIcon => {
                let index = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                Some(Value::String(read_c_string_arg(
                    micro_esp_host_app_icon,
                    index,
                )))
            }
            OsLaunchIndex => {
                let index = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                unsafe { micro_esp_host_set_launch_index(index) };
                None
            }
            OsGoBack => {
                unsafe { micro_esp_host_set_back_intent() };
                None
            }
            OsSetTitle => {
                let title = string_arg(args, 0);
                unsafe { micro_esp_host_set_title(title.as_ptr(), title.len()) };
                None
            }
            OsDelay => {
                let callback = request
                    .callback
                    .ok_or_else(|| VmError::Host("os.delay has no callback".into()))?;
                let ms = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                let now = unsafe { micro_esp_host_uptime_ms() };
                self.pending_delays.push((now.wrapping_add(ms), callback));
                None
            }
        })
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        let mut out = std::mem::take(&mut self.pending);
        let now = unsafe { micro_esp_host_uptime_ms() };
        self.pending_delays.retain(|(deadline, callback)| {
            if now >= *deadline {
                out.push((*callback, Value::String(String::new())));
                false
            } else {
                true
            }
        });
        if let Some(callback) = self.scan_callback.take() {
            let mut buffer = [0_u8; 512];
            let fresh =
                unsafe { micro_esp_host_take_scan_result(buffer.as_mut_ptr(), buffer.len()) } == 1;
            if fresh {
                let len = buffer
                    .iter()
                    .position(|&byte| byte == 0)
                    .unwrap_or(buffer.len());
                out.push((
                    callback,
                    Value::String(String::from_utf8_lossy(&buffer[..len]).into_owned()),
                ));
            } else {
                /* Scan still running; check again on the next tick. */
                self.scan_callback = Some(callback);
            }
        }
        drain_worker(&mut self.http_rx, &mut out, "HTTP worker stopped");
        drain_worker(&mut self.dict_rx, &mut out, "dictionary worker stopped");
        out
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
