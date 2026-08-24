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
//! `net.httpGet` is likewise real: a raw lwip-socket HTTP/1.0 GET runs on its
//! own task and the callback fires when the response completes.

use std::ffi::c_int;

use micro_ir::{FunctionId, HostRequest};
use micro_vm::{HostAccess, Value, VmError};

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
    fn micro_esp_host_http_get(url: *const u8, url_len: usize) -> c_int;
    fn micro_esp_host_http_request(
        method: *const u8,
        method_len: usize,
        url: *const u8,
        url_len: usize,
        body: *const u8,
        body_len: usize,
    ) -> c_int;
    fn micro_esp_host_http_take_result(buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_gpio_setup(pin: u32, mode: *const u8, mode_len: usize) -> c_int;
    fn micro_esp_host_gpio_write(pin: u32, level: u32) -> c_int;
    fn micro_esp_host_gpio_read(pin: u32) -> c_int;
    fn micro_esp_host_app_name(index: u32, buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_app_icon(index: u32, buf: *mut u8, cap: usize) -> c_int;
    fn micro_esp_host_set_launch_index(index: u32);
    fn micro_esp_host_set_back_intent();
    fn micro_esp_host_uptime_ms() -> u32;
}

/// Reads a NUL-terminated string produced by a `micro_esp_host_*` export.
fn read_c_string(read: unsafe extern "C" fn(*mut u8, usize) -> c_int) -> String {
    let mut buffer = [0_u8; 128];
    if unsafe { read(buffer.as_mut_ptr(), buffer.len()) } != 0 {
        return String::new();
    }
    let len = buffer.iter().position(|&byte| byte == 0).unwrap_or(buffer.len());
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
    let len = buffer.iter().position(|&byte| byte == 0).unwrap_or(buffer.len());
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

pub struct EspHost {
    pending: Vec<(FunctionId, Value)>,
    /// `os.delay` callbacks waiting for their deadline (uptime ms).
    pending_delays: Vec<(u32, FunctionId)>,
    /// `net.scanWifi` callback waiting for the async radio scan to finish.
    scan_callback: Option<FunctionId>,
    /// `net.httpGet` callback waiting for the async HTTP request to finish.
    http_callback: Option<FunctionId>,
}

impl EspHost {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            pending_delays: Vec::new(),
            scan_callback: None,
            http_callback: None,
        }
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
            DeviceResetReason => {
                Some(Value::String(read_c_string(micro_esp_host_device_reset_reason)))
            }
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
                Some(Value::Number(if level < 0 { 0.0 } else { f64::from(level) }))
            }
            NetWifiState => Some(Value::String(read_c_string(micro_esp_host_wifi_state))),
            NetWifiSsid => Some(Value::String(read_c_string(micro_esp_host_wifi_ssid))),
            NetWifiConnect => {
                let ssid = string_arg(args, 0);
                let pass = string_arg(args, 1);
                unsafe {
                    micro_esp_host_wifi_connect(ssid.as_ptr(), ssid.len(), pass.as_ptr(), pass.len());
                }
                None
            }
            NetWifiDisconnect => {
                unsafe { micro_esp_host_wifi_disconnect() };
                None
            }
            NetScanWifi => {
                let callback = request.callback.ok_or_else(|| {
                    VmError::Host("net.scanWifi has no callback".into())
                })?;
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
                let callback = request.callback.ok_or_else(|| {
                    VmError::Host("net.httpGet has no callback".into())
                })?;
                let url = string_arg(args, 0);
                /* Kick a real HTTP/1.0 GET over the connected STA link;
                 * drain_results delivers "HTTP <status>\n<body>" to the
                 * callback once the request completes. A rejected request
                 * (busy / bad URL) still fires the callback immediately. */
                if unsafe { micro_esp_host_http_get(url.as_ptr(), url.len()) } == 0 {
                    self.http_callback = Some(callback);
                } else {
                    self.pending.push((
                        callback,
                        Value::String("HTTP 0\nrequest rejected (busy or bad URL)".into()),
                    ));
                }
                None
            }
            NetHttpRequest => {
                let callback = request.callback.ok_or_else(|| {
                    VmError::Host("net.httpRequest has no callback".into())
                })?;
                let method = string_arg(args, 0);
                let url = string_arg(args, 1);
                let body = string_arg(args, 2);
                /* Real HTTP/1.0 with an explicit method over the STA link; the
                 * GET path above shares the same in-flight slot. */
                let accepted = unsafe {
                    micro_esp_host_http_request(
                        method.as_ptr(),
                        method.len(),
                        url.as_ptr(),
                        url.len(),
                        body.as_ptr(),
                        body.len(),
                    )
                } == 0;
                if accepted {
                    self.http_callback = Some(callback);
                } else {
                    self.pending.push((
                        callback,
                        Value::String("HTTP 0\nrequest rejected (busy or bad request)".into()),
                    ));
                }
                None
            }
            OsAppName => {
                let index = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                Some(Value::String(read_c_string_arg(micro_esp_host_app_name, index)))
            }
            OsAppIcon => {
                let index = numeric_arg(args, 0).unwrap_or(0.0) as u32;
                Some(Value::String(read_c_string_arg(micro_esp_host_app_icon, index)))
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
                let len = buffer.iter().position(|&byte| byte == 0).unwrap_or(buffer.len());
                out.push((
                    callback,
                    Value::String(String::from_utf8_lossy(&buffer[..len]).into_owned()),
                ));
            } else {
                /* Scan still running; check again on the next tick. */
                self.scan_callback = Some(callback);
            }
        }
        if let Some(callback) = self.http_callback.take() {
            let mut buffer = [0_u8; 512];
            let fresh =
                unsafe { micro_esp_host_http_take_result(buffer.as_mut_ptr(), buffer.len()) } == 1;
            if fresh {
                let len = buffer.iter().position(|&byte| byte == 0).unwrap_or(buffer.len());
                out.push((
                    callback,
                    Value::String(String::from_utf8_lossy(&buffer[..len]).into_owned()),
                ));
            } else {
                /* Request still in flight; check again on the next tick. */
                self.http_callback = Some(callback);
            }
        }
        out
    }
}
