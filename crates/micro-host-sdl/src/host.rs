//! Simulated host capabilities for the native SDL demo.
//!
//! The desktop host is a simulator: `device.*` and `net.*` return fixed,
//! plausible values rather than touching real hardware. Async requests
//! complete on the next platform tick with a canned result.
//!
//! `os.*` navigation intents (launch an installed app / go back to the shell)
//! are written into a shared [`ShellState`] that the main loop polls, mirroring
//! the pending-intent pattern the ESP32 host uses on the C side.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use micro_ir::{FunctionId, HostRequest};
use micro_vm::{HostAccess, Value, VmError};

/// Shared OS-shell state, polled by the SDL main loop each tick. The app
/// registry lives here too so the main loop can inject the decoded app list
/// without reaching through the boxed `NativeHost`.
#[derive(Default)]
pub struct ShellState {
    /// The installed-app index requested via `os.launchIndex`.
    pub pending_launch: Option<u32>,
    /// Whether the current app requested `os.goBack`.
    pub pending_back: bool,
    /// Installed-app registry: `(name, icon)` per index.
    pub apps: Vec<(String, String)>,
}

#[derive(Default)]
pub struct NativeHost {
    backlight: u8,
    pending: Vec<(FunctionId, Value)>,
    /// Simulated GPIO output registers (pin → level) so read-after-write works.
    gpio: BTreeMap<u32, u8>,
    /// Shared OS-shell state, drained by the SDL main loop.
    pub nav: Rc<RefCell<ShellState>>,
}

impl NativeHost {
    pub fn new() -> Self {
        Self::default()
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
                Some(Value::Number(f64::from(self.gpio.get(&pin).copied().unwrap_or(0))))
            }
            NetWifiState => Some(Value::String("connected".into())),
            NetWifiSsid => Some(Value::String("micro-demo".into())),
            NetWifiConnect | NetWifiDisconnect => None,
            NetScanWifi => {
                let callback = request.callback.ok_or_else(|| {
                    VmError::Host("net.scanWifi has no callback".into())
                })?;
                self.pending.push((
                    callback,
                    Value::String("micro-demo\nguest\nmicro-os".into()),
                ));
                None
            }
            NetHttpGet => {
                let callback = request.callback.ok_or_else(|| {
                    VmError::Host("net.httpGet has no callback".into())
                })?;
                self.pending.push((
                    callback,
                    Value::String("HTTP 200\nHello from the native simulator".into()),
                ));
                None
            }
            NetHttpRequest => {
                let callback = request.callback.ok_or_else(|| {
                    VmError::Host("net.httpRequest has no callback".into())
                })?;
                let method = string_arg(args, 0);
                let url = string_arg(args, 1);
                self.pending.push((
                    callback,
                    Value::String(format!("HTTP 200\n{method} {url} (native simulator)")),
                ));
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
                // Sim: complete on the next tick (real timing wired in the SDL
                // main loop's tick budget).
                self.pending.push((callback, Value::String(String::new())));
                None
            }
        })
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        std::mem::take(&mut self.pending)
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
