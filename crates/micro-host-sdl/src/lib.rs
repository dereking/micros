#[cfg(feature = "native")]
mod native;

pub mod host;

#[cfg(feature = "native")]
pub use native::NativeBridge;
