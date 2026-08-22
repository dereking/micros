use std::fmt;

use micro_ir::{FunctionId, StateId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    OutOfRange(StateId),
    TypeMismatch {
        expected: &'static str,
        found: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmError {
    FunctionOutOfRange(FunctionId),
    InvalidReference(&'static str),
    BudgetExceeded {
        function: FunctionId,
        executed: u64,
    },
    StackUnderflow,
    LocalOutOfRange(u16),
    TypeMismatch {
        expected: &'static str,
        found: &'static str,
    },
    DivisionByZero,
    MissingArgument,
    State(StateError),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for VmError {}

impl From<StateError> for VmError {
    fn from(value: StateError) -> Self {
        Self::State(value)
    }
}
