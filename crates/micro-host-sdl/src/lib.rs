#[cfg(feature = "native")]
mod native;
mod speech;
mod url_audio;

pub mod host;

#[cfg(feature = "native")]
pub use native::{HostAction, HostState, NativeBridge};
