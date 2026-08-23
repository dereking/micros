//! ESP32 host capabilities for the app SDK (`device.*` / `net.*`).
//!
//! Device reads call the `micro_esp_host_*` C exports in
//! `micro_runtime_ffi/micro_esp_host.c`, which read real IDF values
//! (`esp_flash_get_size`, `esp_psram_get_size`, `esp_reset_reason`) and plain
//! C globals mirrored by the OS shell (`main.c`). Wi-Fi connect/disconnect set
//! pending intents that the OS shell drains into the reducer on its next tick.
//!
//! Async requests (`net.scanWifi` / `net.httpGet`) complete one platform tick
//! later with a simulated result: the demo does not wire a real radio or HTTP
//! client, so the OS shell's Wi-Fi state is what `net.*` reports.

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

fn string_arg(args: &[Value], index: usize) -> String {
    match args.get(index) {
        Some(Value::String(value)) => value.clone(),
        _ => String::new(),
    }
}

pub struct EspHost {
    pending: Vec<(FunctionId, Value)>,
}

impl EspHost {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
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
                self.pending.push((
                    callback,
                    Value::String("micro-demo\nguest\nmicro-os (sim)".into()),
                ));
                None
            }
            NetHttpGet => {
                let callback = request.callback.ok_or_else(|| {
                    VmError::Host("net.httpGet has no callback".into())
                })?;
                self.pending.push((
                    callback,
                    Value::String(
                        "HTTP 200\nHello from ESP32 (simulated; no network configured)".into(),
                    ),
                ));
                None
            }
        })
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        std::mem::take(&mut self.pending)
    }
}
