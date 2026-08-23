//! Simulated host capabilities for the native SDL demo.
//!
//! The desktop host is a simulator: `device.*` and `net.*` return fixed,
//! plausible values rather than touching real hardware. Async requests
//! complete on the next platform tick with a canned result.

use micro_ir::{FunctionId, HostRequest};
use micro_vm::{HostAccess, Value, VmError};

#[derive(Default)]
pub struct NativeHost {
    backlight: u8,
    pending: Vec<(FunctionId, Value)>,
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
        })
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        std::mem::take(&mut self.pending)
    }
}
