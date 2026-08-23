//! Host-call interface between the VM and the platform.

use micro_ir::{FunctionId, HostRequest};

use crate::{Value, VmError};

/// Access to host-provided capabilities (`device.*` / `net.*`).
///
/// `call` is invoked synchronously while the VM runs: a sync read returns
/// `Some(value)` which the VM pushes; actions and async requests return `None`.
/// Async requests record their callback internally and the platform later
/// drains completions via [`HostAccess::drain_results`] and re-enqueues them as
/// `Event::HostResult` so the 1-arg callback handler runs with the value.
pub trait HostAccess {
    fn call(&mut self, request: &HostRequest, args: &[Value]) -> Result<Option<Value>, VmError>;

    /// Async completions produced since the last call. Default: none.
    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        Vec::new()
    }
}

/// A host that rejects every host call. Used when an App that calls a host API
/// runs on a host that has not wired one in.
pub struct NullHost;

impl HostAccess for NullHost {
    fn call(&mut self, request: &HostRequest, _args: &[Value]) -> Result<Option<Value>, VmError> {
        Err(VmError::Host(format!(
            "host call {:?} is not supported by this runtime",
            request.kind
        )))
    }
}
