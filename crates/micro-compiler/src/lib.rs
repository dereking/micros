//! Restricted TypeScript to MBC compiler.

mod diagnostic;
mod lower;
mod parse;

pub use diagnostic::Diagnostic;
pub use lower::compile_source;
pub use parse::{ParsedProgram, parse_validated, validate_source};
