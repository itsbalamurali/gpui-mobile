//! Utility packages for common mobile operations.
//!
//! Each package is feature-gated and provides a shared API with
//! platform-specific implementations for iOS and Android.

#[cfg(feature = "package_info")]
pub mod package_info;

#[cfg(feature = "device_info")]
pub mod device_info;

#[cfg(feature = "path_provider")]
pub mod path_provider;

#[cfg(feature = "shared_preferences")]
pub mod shared_preferences;

#[cfg(feature = "url_launcher")]
pub mod url_launcher;

#[cfg(feature = "vibration")]
pub mod vibration;

#[cfg(feature = "connectivity")]
pub mod connectivity;

#[cfg(feature = "network_info")]
pub mod network_info;

#[cfg(feature = "battery")]
pub mod battery;

#[cfg(feature = "share")]
pub mod share;

#[cfg(feature = "sensors")]
pub mod sensors;

#[cfg(feature = "webview")]
pub mod webview;
