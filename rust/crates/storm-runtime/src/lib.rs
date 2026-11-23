//! Shared engine startup and script/entity frame ownership.
//! This initial runtime slice does not yet run complete original game programs.
pub mod clock;
mod config;
mod controls;
mod filesystem;
mod frame;
mod host;
pub mod interface;
mod locator;
mod particles;
mod rain;
mod random;
mod script_libraries;
pub mod sea;
mod shared_headers;
pub mod ship_lights;
mod sound;
mod teleport;
pub mod weather;
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
