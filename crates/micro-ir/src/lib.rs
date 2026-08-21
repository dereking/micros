//! Versioned Micro Bytecode data model and validated binary codec.

mod codec;
mod font_manifest;
mod model;
mod opcode;

pub use codec::{DecodeError, EncodeError, decode, encode};
pub use font_manifest::{REPLACEMENT_GLYPH, sanitize_ui_text};
pub use model::*;
pub use opcode::Instruction;
