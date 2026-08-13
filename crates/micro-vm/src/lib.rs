//! Budgeted Micro Bytecode virtual machine.

mod error;
mod value;
mod vm;

pub use error::{StateError, VmError};
pub use value::Value;
pub use vm::{Execution, StateAccess, Vm};
