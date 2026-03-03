//! Android platform implementation for GPUI.
//!
//! Re-exports core GPUI geometry types so sub-modules can `use super::*`.
//!
//! This module wires together all Android-specific sub-modules and exposes the
//! single public entry-point `current_platform()`, mirroring the structure of
//! `gpui_linux::linux`.
//!
//! ## Architecture
//!
//! ```text
//! AndroidPlatform          (platform.rs)
//!   ├── AndroidDispatcher  (dispatcher.rs)
//!   ├── AndroidWindow      (window.rs)
//!   │     └── WgpuRenderer (renderer.rs)
//!   ├── AndroidDisplay     (display.rs)
//!   ├── CosmicTextSystem   (text.rs)
//!   └── WgpuAtlas          (atlas.rs)
//! ```
//!
//! The JNI / ANativeActivity entry-points live in `jni_entry.rs` and are the
//! first Rust code that executes when the Android runtime loads the `.so`.
//!
//! ## Threading model
//!
//! Android's `ALooper` is used as the run-loop.  A dedicated "main" thread
//! created by `ANativeActivity_onCreate` drives the foreground executor, while
//! a Rust `ThreadPool` backs the background executor — matching the two-queue
//! model used on Linux.
//!
//! ## Integration with GPUI
//!
//! This module depends on the `gpui` crate from the Zed repository for all
//! core types: `Platform`, `PlatformWindow`, `PlatformDisplay`, `Pixels`,
//! `DevicePixels`, `Size`, `Point`, `Bounds`, event types, text system traits,
//! etc.  It also depends on `gpui_wgpu` for the wgpu-based renderer and
//! cosmic-text system.
//!
//! ## Feature flags
//!
//! | flag       | effect                                        |
//! |------------|-----------------------------------------------|
//! | `font-kit` | enables `font-kit` based font matching        |
//!
//! This module is only compiled when `target_os = "android"`.

// ── geometry types ───────────────────────────────────────────────────────────
//
// The real `gpui::Pixels` has a `pub(crate)` inner field that is inaccessible
// outside the `gpui` crate, so the rendering code cannot use `.0` on it.
// We define local geometry stubs here with fully public inner fields.
// Sub-modules import them via `use super::*`.
// Conversions between local stubs and canonical GPUI types happen at the
// platform boundary (`platform.rs`).

/// A logical pixel value (CSS px).  Public inner field unlike `gpui::Pixels`.
#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Pixels(pub f32);

impl Pixels {
    pub const ZERO: Self = Self(0.0);
}

impl From<f32> for Pixels {
    fn from(v: f32) -> Self {
        Self(v)
    }
}

impl From<Pixels> for f32 {
    fn from(p: Pixels) -> Self {
        p.0
    }
}

impl std::ops::Mul<f32> for Pixels {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self(self.0 * rhs)
    }
}

impl std::ops::Div<f32> for Pixels {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self(self.0 / rhs)
    }
}

impl std::ops::Add for Pixels {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Pixels {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl std::ops::Neg for Pixels {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

/// Physical / device pixels.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DevicePixels(pub i32);

impl From<i32> for DevicePixels {
    fn from(v: i32) -> Self {
        Self(v)
    }
}

impl From<DevicePixels> for i32 {
    fn from(dp: DevicePixels) -> Self {
        dp.0
    }
}

/// A 2-D size.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}

/// A 2-D point.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

/// An axis-aligned rectangle.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Bounds<T> {
    pub origin: Point<T>,
    pub size: Size<T>,
}

/// Convenience constructors (mirror `gpui::point` / `gpui::size`).
pub fn point<T>(x: T, y: T) -> Point<T> {
    Point { x, y }
}

pub fn size<T>(width: T, height: T) -> Size<T> {
    Size { width, height }
}

// ── sub-modules ──────────────────────────────────────────────────────────────

pub mod atlas;
pub mod dispatcher;
pub mod display;
pub mod jni_entry;
pub mod keyboard;
pub mod platform;
pub mod renderer;
pub mod text;
pub mod window;

// ── public re-exports ─────────────────────────────────────────────────────────

pub use dispatcher::AndroidDispatcher;
pub use display::AndroidDisplay;
pub use keyboard::*;
pub use platform::{AndroidPlatform, SharedPlatform};
pub use renderer::WgpuRenderer;
pub use text::AndroidTextSystem;
pub use window::{AndroidPlatformWindow, AndroidWindow, SafeAreaInsets};

// ── platform entry-point (mirrors gpui_linux::current_platform) ───────────────

use std::rc::Rc;

/// Returns the Android platform implementation.
///
/// `headless` is accepted for API parity with the Linux / iOS equivalents.
/// When `true` the platform is constructed without an active `ANativeWindow`,
/// which is useful for off-screen rendering and testing.
///
/// # Panics
///
/// Panics if the NDK context has not been initialised (i.e. if called before
/// `ANativeActivity_onCreate` has run).
pub fn current_platform(headless: bool) -> Rc<dyn gpui::Platform> {
    Rc::new(AndroidPlatform::new(headless))
}

// ── helper: Android-specific surface kind ─────────────────────────────────────

/// Which GPU back-end to use for the wgpu surface.
///
/// On Android the Vulkan back-end is strongly preferred; GL-ES is the
/// fallback for devices that don't expose Vulkan 1.1.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AndroidBackend {
    Vulkan,
    Gles,
}

impl Default for AndroidBackend {
    fn default() -> Self {
        Self::Vulkan
    }
}

impl std::fmt::Display for AndroidBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vulkan => write!(f, "Vulkan"),
            Self::Gles => write!(f, "OpenGL ES"),
        }
    }
}

// ── helper: key / input event types ──────────────────────────────────────────

/// A minimal key event representation.
///
/// Maps the subset of Android `KeyEvent` fields that GPUI needs.
#[derive(Clone, Debug)]
pub struct AndroidKeyEvent {
    /// Android key-code (`android.view.KeyEvent.KEYCODE_*`).
    pub key_code: i32,
    /// `ACTION_DOWN = 0`, `ACTION_UP = 1`.
    pub action: i32,
    /// Modifier bitmask (`META_SHIFT_ON`, `META_CTRL_ON`, …).
    pub meta_state: i32,
    /// Unicode character produced (0 if none).
    pub unicode_char: u32,
}

/// A single touch point from `AInputEvent`.
#[derive(Clone, Debug)]
pub struct TouchPoint {
    pub id: i32,
    pub x: f32,
    pub y: f32,
    /// `AMOTION_EVENT_ACTION_*` action masked to a single pointer.
    pub action: u32,
}

// ── shared logging helper ─────────────────────────────────────────────────────

/// Initialise `android_logger` so that `log::*` macros route to logcat.
///
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn init_logger() {
    use std::sync::OnceLock;
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Debug)
                .with_tag("gpui-android"),
        );
        log::info!("gpui-android logger initialised");
    });
}
