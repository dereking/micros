//! Manifest-declared host capability enforcement.
//!
//! The gate wraps whatever `HostAccess` the host installed and checks every
//! `HostCallKind` against the App's declared allow-list before delegating.
//! Default-deny: an App that declares no capabilities gets an empty grant.

use std::collections::BTreeSet;

use micro_ir::{Capability, FunctionId, HostRequest};
use micro_vm::{HostAccess, Value, VmError};

pub(crate) struct CapabilityGate<H> {
    inner: H,
    granted: BTreeSet<Capability>,
}

impl<H> CapabilityGate<H> {
    pub(crate) fn new(inner: H, granted: BTreeSet<Capability>) -> Self {
        Self { inner, granted }
    }
}

impl<H: HostAccess> HostAccess for CapabilityGate<H> {
    fn call(&mut self, request: &HostRequest, args: &[Value]) -> Result<Option<Value>, VmError> {
        let capability = request.kind.capability();
        if !self.granted.contains(&capability) {
            return Err(VmError::Host(format!(
                "capability `{capability}` is not granted by the app manifest"
            )));
        }
        self.inner.call(request, args)
    }

    fn drain_results(&mut self) -> Vec<(FunctionId, Value)> {
        self.inner.drain_results()
    }

    fn dispose(&mut self) {
        self.inner.dispose();
    }
}
