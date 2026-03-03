//! iOS platform implementation for GPUI.
//!
//! iOS uses UIKit instead of AppKit, so the platform implementation differs
//! significantly from macOS despite sharing many underlying technologies:
//! - Grand Central Dispatch (GCD) for threading
//! - CoreText for text rendering
//! - Metal for GPU rendering via the Blade renderer
//! - CoreFoundation for many utilities
//!
//! ## Integration with GPUI
//!
//! This module depends on the `gpui` crate from the Zed repository for all
//! core types: `Platform`, `PlatformWindow`, `PlatformDisplay`, `Pixels`,
//! `DevicePixels`, `Size`, `Point`, `Bounds`, event types, text system traits,
//! etc.  The local stub types that were previously declared here have been
//! removed in favour of the canonical `gpui::*` equivalents.
//!
//! This module is only compiled when targeting `target_os = "ios"`.

pub mod demos;
pub mod dispatcher;
pub mod display;
pub mod events;
pub mod ffi;
pub mod platform;
pub mod text_input;
pub mod window;

// Re-use CoreText-based text system (requires font-kit feature flag)
#[cfg(feature = "font-kit")]
pub mod text_system;

// ── public re-exports ────────────────────────────────────────────────────────

pub use dispatcher::IosDispatcher;
pub use display::IosDisplay;
pub use ffi::{
    gpui_ios_did_become_active, gpui_ios_did_enter_background, gpui_ios_did_finish_launching,
    gpui_ios_get_window, gpui_ios_handle_key_event, gpui_ios_handle_text_input,
    gpui_ios_handle_touch, gpui_ios_hide_keyboard, gpui_ios_initialize, gpui_ios_request_frame,
    gpui_ios_run_demo, gpui_ios_show_keyboard, gpui_ios_will_enter_foreground,
    gpui_ios_will_resign_active, gpui_ios_will_terminate,
};
pub use platform::IosPlatform;
pub use window::IosWindow;

#[cfg(feature = "font-kit")]
pub use text_system::IosTextSystem;

// ── platform entry-point (mirrors gpui_linux::current_platform) ──────────────

use std::rc::Rc;

/// Returns the iOS platform implementation.
///
/// `headless` is accepted for API parity with the Linux/macOS equivalents
/// but is currently ignored — iOS always uses a UIKit-backed window.
pub fn current_platform(_headless: bool) -> Rc<dyn gpui::Platform> {
    Rc::new(IosPlatform::new())
}

// ── shared helpers ────────────────────────────────────────────────────────────

use objc::runtime::{BOOL, NO, YES};

/// Extension trait to convert a Rust `bool` to an Objective-C `BOOL`.
pub(crate) trait BoolExt {
    fn to_objc(self) -> BOOL;
}

impl BoolExt for bool {
    #[inline]
    fn to_objc(self) -> BOOL {
        if self {
            YES
        } else {
            NO
        }
    }
}

/// `NSRange` — used when bridging CoreText / UITextInput APIs.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct NSRange {
    pub location: usize,
    pub length: usize,
}

impl NSRange {
    /// Sentinel value meaning "no range" (equivalent to `NSNotFound`).
    #[inline]
    pub fn invalid() -> Self {
        Self {
            location: usize::MAX,
            length: 0,
        }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.location != usize::MAX
    }

    pub fn to_range(self) -> Option<std::ops::Range<usize>> {
        if self.is_valid() {
            let start = self.location;
            let end = start + self.length;
            Some(start..end)
        } else {
            None
        }
    }
}

impl From<std::ops::Range<usize>> for NSRange {
    fn from(range: std::ops::Range<usize>) -> Self {
        NSRange {
            location: range.start,
            length: range.len(),
        }
    }
}

unsafe impl objc::Encode for NSRange {
    fn encode() -> objc::Encoding {
        let encoding = format!(
            "{{NSRange={}{}}}",
            usize::encode().as_str(),
            usize::encode().as_str()
        );
        unsafe { objc::Encoding::from_str(&encoding) }
    }
}

// ── CoreGraphics ↔ GPUI geometry conversions ─────────────────────────────────
//
// We cannot implement `From<CGSize> for gpui::Size<gpui::Pixels>` etc. due to
// the orphan rule (both the trait and the types are from external crates).
// Instead we provide helper functions that sub-modules can call.

use gpui::{px, size, DevicePixels, Pixels, Size};

/// Convert a `CGSize` to `Size<Pixels>`.
#[inline]
pub(crate) fn cgsize_to_size_pixels(value: core_graphics::geometry::CGSize) -> Size<Pixels> {
    size(px(value.width as f32), px(value.height as f32))
}

/// Convert a `CGRect`'s size to `Size<Pixels>`.
#[inline]
pub(crate) fn cgrect_to_size_pixels(rect: core_graphics::geometry::CGRect) -> Size<Pixels> {
    size(px(rect.size.width as f32), px(rect.size.height as f32))
}

/// Convert a `CGRect`'s size to `Size<DevicePixels>`.
#[inline]
pub(crate) fn cgrect_to_size_device_pixels(
    rect: core_graphics::geometry::CGRect,
) -> Size<DevicePixels> {
    size(
        DevicePixels(rect.size.width as i32),
        DevicePixels(rect.size.height as i32),
    )
}
