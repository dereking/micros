//! Versioned Micro Bytecode data model and validated binary codec.

mod codec;
mod model;
mod opcode;

pub use codec::{DecodeError, EncodeError, decode, encode};
pub use model::*;
pub use opcode::Instruction;
