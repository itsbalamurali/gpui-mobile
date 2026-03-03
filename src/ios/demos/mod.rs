//! iOS demo modules — interactive showcases of GPUI rendering on iOS.
//!
//! This module re-exports the three demo components and provides the shared
//! Catppuccin Mocha color palette used across all of them.
//!
//! # Demos
//!
//! | Demo                  | Description                                      |
//! |-----------------------|--------------------------------------------------|
//! | [`AnimationPlayground`] | Bouncing balls with physics + particle effects |
//! | [`ShaderShowcase`]      | Dynamic gradients, floating orbs, ripple FX    |
//! | [`DemoApp`]             | Root navigation menu wiring the two demos       |
//!
//! # Color palette
//!
//! All demos share the [Catppuccin Mocha] palette defined as `u32` RGB
//! constants so they can be passed directly to GPUI's `rgb()` helper.
//!
//! [Catppuccin Mocha]: https://github.com/catppuccin/catppuccin

mod animation_playground;
mod menu;
mod shader_showcase;

pub use animation_playground::AnimationPlayground;
pub use menu::{back_button, DemoApp};
pub use shader_showcase::ShaderShowcase;

// ── Catppuccin Mocha palette ──────────────────────────────────────────────────

/// Base background — `#1e1e2e`
pub const BACKGROUND: u32 = 0x1e1e2e;
/// Slightly elevated surface — `#313244`
pub const SURFACE: u32 = 0x313244;
/// Overlay / muted surface — `#45475a`
pub const OVERLAY: u32 = 0x45475a;
/// Primary text — `#cdd6f4`
pub const TEXT: u32 = 0xcdd6f4;
/// Secondary / subdued text — `#a6adc8`
pub const SUBTEXT: u32 = 0xa6adc8;

// Accent colours
/// Red — `#f38ba8`
pub const RED: u32 = 0xf38ba8;
/// Green — `#a6e3a1`
pub const GREEN: u32 = 0xa6e3a1;
/// Blue — `#89b4fa`
pub const BLUE: u32 = 0x89b4fa;
/// Yellow — `#f9e2af`
pub const YELLOW: u32 = 0xf9e2af;
/// Pink — `#f5c2e7`
pub const PINK: u32 = 0xf5c2e7;
/// Mauve / purple — `#cba6f7`
pub const MAUVE: u32 = 0xcba6f7;
/// Peach — `#fab387`
pub const PEACH: u32 = 0xfab387;
/// Teal — `#94e2d5`
pub const TEAL: u32 = 0x94e2d5;
/// Sky — `#89dceb`
pub const SKY: u32 = 0x89dceb;
/// Lavender — `#b4befe`
pub const LAVENDER: u32 = 0xb4befe;

/// The full vibrant accent palette used by [`random_color`].
const ACCENT_PALETTE: [u32; 10] = [
    RED, GREEN, BLUE, YELLOW, PINK, MAUVE, PEACH, TEAL, SKY, LAVENDER,
];

/// Return a deterministic accent colour for the given `seed`.
///
/// Cycles through the ten Catppuccin Mocha accent colours so that successive
/// calls with `seed = 0, 1, 2, …` produce a visually distinct sequence.
///
/// ```
/// use gpui_mobile::ios::demos::{random_color, RED};
///
/// assert_eq!(random_color(0), RED);
/// // After 10 items the sequence repeats.
/// assert_eq!(random_color(10), RED);
/// ```
pub fn random_color(seed: usize) -> u32 {
    ACCENT_PALETTE[seed % ACCENT_PALETTE.len()]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_color_cycles_over_palette() {
        // Every seed in [0, N) should return a value from the palette.
        for i in 0..30 {
            let color = random_color(i);
            assert!(
                ACCENT_PALETTE.contains(&color),
                "random_color({i}) = {color:#010x} not in palette"
            );
        }
    }

    #[test]
    fn random_color_is_periodic() {
        let n = ACCENT_PALETTE.len();
        for i in 0..n {
            assert_eq!(
                random_color(i),
                random_color(i + n),
                "random_color should be periodic with period {n}"
            );
        }
    }

    #[test]
    fn palette_constants_are_nonzero() {
        for &c in &ACCENT_PALETTE {
            assert_ne!(c, 0, "palette colour must not be zero / black");
        }
    }

    #[test]
    fn background_is_darker_than_text() {
        // Rough luminance check: TEXT should be a lighter grey than BACKGROUND.
        // Compare the sum of RGB components as a proxy for brightness.
        let bg_brightness =
            ((BACKGROUND >> 16) & 0xff) + ((BACKGROUND >> 8) & 0xff) + (BACKGROUND & 0xff);
        let text_brightness = ((TEXT >> 16) & 0xff) + ((TEXT >> 8) & 0xff) + (TEXT & 0xff);
        assert!(
            text_brightness > bg_brightness,
            "TEXT should be brighter than BACKGROUND"
        );
    }
}
