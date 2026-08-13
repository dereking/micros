//! ESP32 ownership layer for the shared Micro Runtime and OS reducer.

mod bridge;
#[cfg(target_os = "espidf")]
mod ffi;

use std::fmt;

use micro_core::{Event, Runtime, RuntimeError};
use micro_ir::{DecodeError, FunctionId, decode};
use micro_lvgl::{LvglRenderer, NativeUi};

pub use bridge::{MicroAction, MicroErrorCode, MicroEvent, MicroEventKind, MicroState};

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

    pub fn dispatch(&mut self, event: MicroEvent) -> MicroAction {
        bridge::dispatch(&mut self.os, event)
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
        let owned_mbc = mbc.to_vec();
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
