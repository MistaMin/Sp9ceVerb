//! Sp9ceVerb: the unified reverb engine as a plugin on OpenPluginSDK, with a
//! GoodLookinDri editor and a software-rendered 3D decay surface.

#[cfg(target_os = "macos")]
pub mod clap_host;
#[cfg(target_os = "macos")]
pub mod cocoa;
pub mod gui;
pub mod plugin;
pub mod presets;
pub mod shell_host;

pub use plugin::{OnePlugin, Shared};

