//! GPUI Mobile Platform Support
//!
//! This crate provides mobile platform support for GPUI, enabling GPUI applications
//! to run on iOS and Android devices with native performance.
//!
//! ## Platform Architecture
//!
//! The crate mirrors the structure used by `gpui_linux`:
//! - A thin top-level `current_platform()` dispatcher that selects the right backend
//! - An `ios` module for the iOS platform (Metal renderer via `gpui_wgpu`, UIKit, CoreText)
//! - An `android` module for the Android platform (Vulkan renderer via `gpui_wgpu`, NDK)
//!
//! ## Integration with GPUI
//!
//! This crate depends on the `gpui` crate from the Zed repository for all
//! core types: `Platform`, `PlatformWindow`, `PlatformDisplay`, `Pixels`,
//! `DevicePixels`, `Size`, `Point`, `Bounds`, event types, text system traits,
//! etc.  It also depends on `gpui_wgpu` for the GPU renderer (`WgpuRenderer`)
//! and text system (`CosmicTextSystem`) on both platforms.
//!
//! ## iOS
//!
//! The iOS implementation uses UIKit for windowing and `gpui_wgpu` for Metal
//! rendering.  Key modules:
//!
//! - `platform`   — `IosPlatform` implementing the GPUI `Platform` trait
//! - `window`     — `IosWindow` backed by `UIWindow` + `CAMetalLayer` + `gpui_wgpu`
//! - `display`    — `IosDisplay` wrapping `UIScreen`
//! - `dispatcher` — `IosDispatcher` using Grand Central Dispatch
//! - `events`     — Touch-to-mouse event translation
//! - `ffi`        — C-ABI bridge for Objective-C app-delegate integration
//! - `text_input` — External-keyboard HID key-code mapping
//! - `text_system`— CoreText-based text shaping (requires `font-kit` feature)
//!
//! ## Android
//!
//! The Android implementation uses the NDK for windowing and `gpui_wgpu` for
//! Vulkan rendering.  Key modules:
//!
//! - `platform`   — `AndroidPlatform` implementing the GPUI `Platform` trait
//! - `window`     — `AndroidWindow` backed by `ANativeWindow` + `gpui_wgpu`
//! - `display`    — `AndroidDisplay` wrapping NDK display info
//! - `dispatcher` — `AndroidDispatcher` using `ALooper` + thread pool
//! - `keyboard`   — Android NDK key code → GPUI `Keystroke` mapping
//! - `jni_entry`  — `ANativeActivity_onCreate` / JNI entry points + event loop
//!
//! ## Example — iOS
//!
//! ```rust,no_run
//! # #[cfg(target_os = "ios")]
//! # {
//! use gpui_mobile::current_platform;
//! let platform = current_platform(false);
//! // Hand `platform` to GPUI's Application initialiser.
//! # }
//! ```
//!
//! ## Example — Android
//!
//! ```rust,no_run
//! # #[cfg(target_os = "android")]
//! # {
//! use gpui_mobile::current_platform;
//! let platform = current_platform(false);
//! # }
//! ```

// ── Re-export the gpui crate so consumers can access types through us ────────

pub use gpui;

// ── shared modules ───────────────────────────────────────────────────────────

pub mod components;
pub mod momentum;

// ── platform modules ─────────────────────────────────────────────────────────

#[cfg(target_os = "ios")]
pub mod ios;

#[cfg(target_os = "android")]
pub mod android;

// ── public re-exports ────────────────────────────────────────────────────────

#[cfg(target_os = "ios")]
pub use ios::{current_platform, IosPlatform};

#[cfg(target_os = "android")]
pub use android::{current_platform, AndroidPlatform};

// ── fallback for non-mobile host builds (e.g. documentation / CI) ────────────

/// Returns the platform implementation for the current mobile OS.
///
/// On host builds (documentation, CI) this always panics — the caller must
/// compile for `aarch64-apple-ios` or an Android target.
///
/// When compiled for iOS, returns an `Rc<dyn gpui::Platform>` backed by `IosPlatform`.
/// When compiled for Android, returns an `Rc<dyn gpui::Platform>` backed by `AndroidPlatform`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn current_platform(_headless: bool) -> ! {
    panic!(
        "gpui-mobile: `current_platform` is only available when compiled for \
         `target_os = \"ios\"` or `target_os = \"android\"`."
    );
}
