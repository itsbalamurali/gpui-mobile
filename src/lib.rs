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

// ── System chrome (status bar / navigation bar) styling ──────────────────────

/// Controls the appearance of the device status bar text and icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusBarContentStyle {
    /// White text/icons — use on dark backgrounds.
    Light,
    /// Dark text/icons — use on light backgrounds.
    #[default]
    Dark,
}

/// Configures the system chrome (status bar and navigation bar) appearance.
///
/// Use [`set_system_chrome`] to apply a style. Colors are specified as
/// 0xRRGGBB values (no alpha). Pass `None` for a color field to leave it
/// unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemChromeStyle {
    /// Background color for the top safe area (behind the status bar).
    pub status_bar_color: Option<u32>,
    /// Whether the status bar content (text/icons) should be light or dark.
    pub status_bar_style: StatusBarContentStyle,
    /// Background color for the bottom safe area (behind the home indicator / nav bar).
    pub navigation_bar_color: Option<u32>,
}

impl Default for SystemChromeStyle {
    fn default() -> Self {
        Self {
            status_bar_color: None,
            status_bar_style: StatusBarContentStyle::Dark,
            navigation_bar_color: None,
        }
    }
}

/// Apply system chrome styling (status bar style, navigation bar color).
///
/// On iOS this updates `preferredStatusBarStyle` on the root view controller.
/// On Android this calls `Window.setStatusBarColor()`,
/// `Window.setNavigationBarColor()`, and configures light/dark status bar icons.
///
/// On unsupported platforms this is a no-op.
pub fn set_system_chrome(style: &SystemChromeStyle) {
    #[cfg(target_os = "ios")]
    {
        ios::set_status_bar_style(style.status_bar_style);
    }
    #[cfg(target_os = "android")]
    {
        android::jni_entry::set_system_chrome(style);
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let _ = style;
    }
}

// ── Text input callback ──────────────────────────────────────────────────────

use std::cell::RefCell;

thread_local! {
    /// Global text input callback — set by the active text input component.
    /// When the software keyboard sends text, this callback is invoked.
    static TEXT_INPUT_CALLBACK: RefCell<Option<Box<dyn FnMut(&str)>>> = RefCell::new(None);
}

/// Register a callback that receives text from the software keyboard.
///
/// Only one callback can be active at a time. Call with `None` to clear it.
/// This is typically called by the text input component when it gains focus.
pub fn set_text_input_callback(callback: Option<Box<dyn FnMut(&str)>>) {
    TEXT_INPUT_CALLBACK.with(|cb| {
        *cb.borrow_mut() = callback;
    });
}

/// Dispatch text input to the registered callback.
///
/// Called internally by the platform layer when keyboard text is received.
/// Returns true if a callback handled the text.
pub fn dispatch_text_input(text: &str) -> bool {
    TEXT_INPUT_CALLBACK.with(|cb| {
        if let Some(callback) = cb.borrow_mut().as_mut() {
            callback(text);
            true
        } else {
            false
        }
    })
}

// ── Software keyboard control ────────────────────────────────────────────────

/// Show the software keyboard.
///
/// On iOS this makes the hidden text input view the first responder.
/// On Android this opens the input method editor.
/// On unsupported platforms this is a no-op.
pub fn show_keyboard() {
    #[cfg(target_os = "ios")]
    {
        if let Some(wrapper) = ios::ffi::IOS_WINDOW_LIST.get() {
            unsafe {
                let windows = &*wrapper.0.get();
                if let Some(&window) = windows.last() {
                    (*window).show_keyboard();
                }
            }
        }
    }
    #[cfg(not(target_os = "ios"))]
    {}
}

/// Hide the software keyboard.
///
/// On iOS this resigns first responder from the text input view.
/// On Android this closes the input method editor.
/// On unsupported platforms this is a no-op.
pub fn hide_keyboard() {
    #[cfg(target_os = "ios")]
    {
        if let Some(wrapper) = ios::ffi::IOS_WINDOW_LIST.get() {
            unsafe {
                let windows = &*wrapper.0.get();
                if let Some(&window) = windows.last() {
                    (*window).hide_keyboard();
                }
            }
        }
    }
    #[cfg(not(target_os = "ios"))]
    {}
}

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
