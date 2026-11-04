//! Shared engine startup and script/entity frame ownership.
//! This initial runtime slice does not yet run complete original game programs.
pub mod clock;
mod config;
mod filesystem;
mod frame;
mod host;
mod window_settings;
pub use window_settings::WindowSettings;
pub mod model;
pub use config::Startup;
pub use filesystem::{GameDirectory, GameSource, ScriptEncoding};
pub use frame::{Engine, FrameResult, Phase, Section, Service, ServiceFrame};
pub use host::NativeHost;
#[cfg(test)]
mod tests;
