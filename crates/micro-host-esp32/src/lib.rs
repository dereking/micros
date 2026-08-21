//! ESP32 ownership layer for the shared Micro Runtime and OS reducer.

mod bridge;
#[cfg(target_os = "espidf")]
mod ffi;

use std::fmt;

use micro_core::{Event, Runtime, RuntimeError};
use micro_ir::{DecodeError, FunctionId, decode};
use micro_lvgl::{LvglRenderer, NativeUi};

pub use bridge::{
    DispatchError, MicroAction, MicroActionKind, MicroAppId, MicroErrorCode, MicroEvent,
    MicroEventKind, MicroFailureReason, MicroResult, MicroState, MicroWifiFailure,
    decode_action_batch, encode_action_batch,
};

pub fn write_diagnostic(buffer: &mut [u8], diagnostic: &str) {
    if buffer.is_empty() {
        return;
    }
    buffer.fill(0);
    let capacity = buffer.len() - 1;
    let mut copied = capacity.min(diagnostic.len());
    while copied != 0 && !diagnostic.is_char_boundary(copied) {
        copied -= 1;
    }
    buffer[..copied].copy_from_slice(&diagnostic.as_bytes()[..copied]);
}

pub fn validate_region_length(length: usize, element_size: usize) -> Result<(), MicroErrorCode> {
    let bytes = length
        .checked_mul(element_size)
        .ok_or(MicroErrorCode::InvalidArgument)?;
    if bytes > isize::MAX as usize {
        return Err(MicroErrorCode::InvalidArgument);
    }
    Ok(())
}

pub struct OsHost {
    os: micro_os_core::MicroOs,
}

impl Default for OsHost {
    fn default() -> Self {
        Self::new()
    }
}

impl OsHost {
    #[must_use]
    pub fn new() -> Self {
        Self {
            os: micro_os_core::MicroOs::new(),
        }
    }

    pub fn dispatch(&mut self, event: MicroEvent) -> Result<Vec<MicroAction>, MicroErrorCode> {
        let event = event.try_into_core()?;
        Ok(bridge::encode_action_batch(&self.os.dispatch(event)))
    }

    pub fn dispatch_into(
        &mut self,
        event: MicroEvent,
        output: &mut [MicroAction],
    ) -> Result<usize, DispatchError> {
        let event = event
            .try_into_core()
            .map_err(|code| DispatchError { code, required: 0 })?;
        let mut next = self.os.clone();
        let action = next.dispatch(event);
        let written = bridge::encode_action_into(&action, output)?;
        self.os = next;
        Ok(written)
    }

    #[must_use]
    pub fn state(&self) -> MicroState {
        bridge::map_state(self.os.state())
    }
}

#[derive(Debug)]
pub struct HostError {
    code: MicroErrorCode,
    diagnostic: String,
}

impl HostError {
    #[must_use]
    pub fn code(&self) -> MicroErrorCode {
        self.code
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.diagnostic)
    }
}

impl std::error::Error for HostError {}

impl From<DecodeError> for HostError {
    fn from(error: DecodeError) -> Self {
        Self {
            code: MicroErrorCode::Mbc,
            diagnostic: format!("invalid MBC: {error}"),
        }
    }
}

impl From<RuntimeError> for HostError {
    fn from(error: RuntimeError) -> Self {
        Self {
            code: MicroErrorCode::Runtime,
            diagnostic: error.to_string(),
        }
    }
}

pub struct RuntimeHost<B: NativeUi> {
    runtime: Runtime<LvglRenderer<B>>,
    stopped: bool,
}

impl<B: NativeUi> RuntimeHost<B> {
    pub fn new(mbc: &[u8], bridge: B, event_budget: u64) -> Result<Self, HostError> {
        Self::from_owned_mbc(mbc.to_vec(), bridge, event_budget)
    }

    pub fn from_owned_mbc(
        owned_mbc: Vec<u8>,
        bridge: B,
        event_budget: u64,
    ) -> Result<Self, HostError> {
        let image = decode(&owned_mbc)?;
        let runtime = Runtime::new(image, LvglRenderer::new(bridge), event_budget)?;
        Ok(Self {
            runtime,
            stopped: false,
        })
    }

    pub fn activate(&mut self, handler: FunctionId) -> Result<(), HostError> {
        if self.stopped {
            return Err(HostError {
                code: MicroErrorCode::Stopped,
                diagnostic: "runtime is stopped".into(),
            });
        }
        self.runtime.enqueue(Event::Activate(handler));
        Ok(())
    }

    pub fn tick(&mut self) -> Result<bool, HostError> {
        if self.stopped {
            return Err(HostError {
                code: MicroErrorCode::Stopped,
                diagnostic: "runtime is stopped".into(),
            });
        }
        self.runtime.tick().map_err(Into::into)
    }

    pub fn stop(&mut self) -> Result<(), HostError> {
        if !self.stopped {
            self.runtime
                .renderer_mut()
                .bridge_mut()
                .destroy_app_root()
                .map_err(|diagnostic| HostError {
                    code: MicroErrorCode::Ui,
                    diagnostic,
                })?;
            self.stopped = true;
        }
        Ok(())
    }

    #[must_use]
    pub fn bridge(&self) -> &B {
        self.runtime.renderer().bridge()
    }
}
