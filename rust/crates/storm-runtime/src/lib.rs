//! Shared engine startup and script/entity frame ownership.
//! This initial runtime slice does not yet run complete original game programs.
pub mod clock;
mod config;
mod controls;
mod filesystem;
mod frame;
mod host;
mod locator;
mod script_libraries;
mod shared_headers;
mod window_settings;
pub use window_settings::WindowSettings;
pub mod model;
pub use config::Startup;
pub use controls::ControlInput;
pub use filesystem::{GameDirectory, GameSource, ScriptEncoding};
pub use frame::{Engine, FrameResult, Phase, Section, Service, ServiceFrame};
pub use host::NativeHost;
#[cfg(test)]
mod tests;
