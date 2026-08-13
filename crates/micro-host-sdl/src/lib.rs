#[cfg(feature = "native")]
mod native;

#[cfg(feature = "native")]
pub use native::NativeBridge;
