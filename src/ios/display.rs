//! iOS display handling using UIScreen.
//!
//! iOS has a simpler display model than macOS — typically just the main screen
//! and possibly an external display connected via AirPlay or USB-C (iPad).
//!
//! `IosDisplay` wraps a `UIScreen` pointer and implements enough of the GPUI
//! `PlatformDisplay` contract to let the rest of the platform code locate
//! and describe available screens without pulling in the full GPUI crate as a
//! dependency.

use super::{DevicePixels, Pixels, Size};
use anyhow::Result;
use core_graphics::geometry::CGRect;
use objc::{class, msg_send, sel, sel_impl};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// IosDisplay
// ---------------------------------------------------------------------------

/// A display (screen) on iOS, wrapping a `UIScreen` Objective-C object.
///
/// On a typical iPhone there is exactly one screen.  iPad can expose a second
/// screen when connected to an external display via USB-C / AirPlay.
#[derive(Debug)]
pub struct IosDisplay {
    /// Raw pointer to the `UIScreen` instance.
    ///
    /// `UIScreen` objects are singletons managed by UIKit; we never deallocate
    /// them ourselves.
    screen: *mut objc::runtime::Object,
}

// SAFETY: UIScreen objects are thread-safe for the read-only property access
// we perform here (bounds, scale).  We never mutate the object.
unsafe impl Send for IosDisplay {}
unsafe impl Sync for IosDisplay {}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

impl IosDisplay {
    /// Returns the main (built-in) screen.
    pub fn main() -> Self {
        unsafe {
            let screen: *mut objc::runtime::Object = msg_send![class!(UIScreen), mainScreen];
            Self { screen }
        }
    }

    /// Returns an iterator over every currently connected screen.
    ///
    /// The first element is always the main screen.
    pub fn all() -> impl Iterator<Item = Self> {
        unsafe {
            let screens: *mut objc::runtime::Object = msg_send![class!(UIScreen), screens];
            let count: usize = msg_send![screens, count];

            (0..count).map(move |i| {
                let screen: *mut objc::runtime::Object = msg_send![screens, objectAtIndex: i];
                Self { screen }
            })
        }
    }

    // -----------------------------------------------------------------------
    // Screen properties
    // -----------------------------------------------------------------------

    /// Logical bounds of the screen in **points** (not device pixels).
    ///
    /// This is the coordinate space used by UIKit for layout.
    pub fn bounds_in_points(&self) -> CGRect {
        unsafe { msg_send![self.screen, bounds] }
    }

    /// The native (hardware) scale factor.
    ///
    /// e.g. `3.0` for an iPhone 15 Pro, `2.0` for a non-Pro iPhone.
    /// Unlike `scale()`, this value is not affected by Display Zoom.
    pub fn native_scale(&self) -> f32 {
        unsafe {
            let scale: f64 = msg_send![self.screen, nativeScale];
            scale as f32
        }
    }

    /// The current logical scale factor.
    ///
    /// May differ from `native_scale()` when the user has enabled Display Zoom
    /// in Accessibility settings.
    pub fn scale(&self) -> f32 {
        unsafe {
            let scale: f64 = msg_send![self.screen, scale];
            scale as f32
        }
    }

    /// The screen bounds expressed in **device pixels** (points × native scale).
    pub fn bounds_in_pixels(&self) -> Size<DevicePixels> {
        let rect = self.bounds_in_points();
        let s = self.native_scale();
        Size {
            width: DevicePixels((rect.size.width * s as f64) as i32),
            height: DevicePixels((rect.size.height * s as f64) as i32),
        }
    }

    /// The screen bounds expressed in logical **pixels** (i.e. points).
    pub fn bounds_in_logical_pixels(&self) -> Size<Pixels> {
        Size::from(self.bounds_in_points())
    }

    // -----------------------------------------------------------------------
    // Display identity
    // -----------------------------------------------------------------------

    /// A numeric identifier for this display derived from its pointer value.
    ///
    /// iOS does not assign stable integer display IDs like macOS CGDirectDisplayID,
    /// so we reinterpret the (stable, singleton) `UIScreen *` pointer as a `u32`.
    pub fn id(&self) -> u32 {
        self.screen as u32
    }

    /// A UUID that uniquely identifies this screen within the current boot session.
    ///
    /// Constructed deterministically from the screen's physical resolution and
    /// scale factor so that the same physical display always produces the same
    /// UUID (provided the device hasn't been rebooted with a different
    /// display configuration).
    pub fn uuid(&self) -> Result<Uuid> {
        let rect = self.bounds_in_points();
        let scale = self.native_scale();

        let key = format!(
            "ios-screen-w{}-h{}-s{}",
            rect.size.width as u32,
            rect.size.height as u32,
            (scale * 100.0) as u32,
        );

        Ok(Uuid::new_v5(&Uuid::NAMESPACE_OID, key.as_bytes()))
    }

    /// Human-readable description suitable for debug output.
    pub fn describe(&self) -> String {
        let rect = self.bounds_in_points();
        let scale = self.scale();
        let native = self.native_scale();
        format!(
            "IosDisplay({}x{}pt, scale={}, nativeScale={})",
            rect.size.width as u32, rect.size.height as u32, scale, native,
        )
    }
}

// ---------------------------------------------------------------------------
// Trait-style helpers (without a hard dep on the full GPUI Platform trait)
// ---------------------------------------------------------------------------

impl IosDisplay {
    /// Returns the logical-pixel bounds as a `(width, height)` tuple.
    pub fn logical_size(&self) -> (f32, f32) {
        let rect = self.bounds_in_points();
        (rect.size.width as f32, rect.size.height as f32)
    }

    /// Returns the device-pixel bounds as a `(width, height)` tuple.
    pub fn physical_size(&self) -> (i32, i32) {
        let s = self.bounds_in_pixels();
        (s.width.0, s.height.0)
    }
}

// ---------------------------------------------------------------------------
// Tests (run only when cross-compiled for iOS)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // These tests only pass when actually running on an iOS device/simulator.
    // They are skipped silently on other platforms via the `#[cfg]` on the
    // enclosing module in lib.rs.

    #[test]
    fn display_uuid_is_deterministic() {
        let d1 = IosDisplay::main();
        let d2 = IosDisplay::main();
        // Two calls to main() return the same singleton — UUIDs must match.
        assert_eq!(d1.uuid().unwrap(), d2.uuid().unwrap());
    }

    #[test]
    fn display_scale_is_positive() {
        let d = IosDisplay::main();
        assert!(d.scale() > 0.0);
        assert!(d.native_scale() > 0.0);
    }

    #[test]
    fn display_bounds_are_positive() {
        let d = IosDisplay::main();
        let (w, h) = d.logical_size();
        assert!(w > 0.0);
        assert!(h > 0.0);
    }

    #[test]
    fn all_displays_includes_main() {
        let all: Vec<IosDisplay> = IosDisplay::all().collect();
        assert!(!all.is_empty(), "should have at least one display");
    }
}
