//! Budgeted Micro Bytecode virtual machine.

mod error;
mod host;
mod value;
mod vm;

pub use error::{StateError, VmError};
pub use host::{HostAccess, NullHost};
pub use value::Value;
pub use vm::{Execution, StateAccess, Vm};
