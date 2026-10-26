#[cfg(feature = "native")]
mod app_icon;
#[cfg(feature = "native")]
pub mod external_apps;
#[cfg(feature = "native")]
mod native;
#[cfg(feature = "native")]
pub mod single_instance;
mod speech;
mod url_audio;

pub mod host;

#[cfg(feature = "native")]
pub use native::{HostAction, HostState, NativeBridge};
