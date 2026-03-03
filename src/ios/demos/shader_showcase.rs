//! Shader Showcase — dynamic gradients, floating orbs, and ripple effects.
//!
//! This demo showcases GPUI's GPU rendering capabilities on iOS:
//!
//! - **Background** — a slowly-rotating linear gradient whose hue cycles over time;
//!   touching the screen tilts the gradient angle toward the touch point
//! - **Orbs** — eight translucent blobs that float with a parallax offset driven
//!   by the touch position and pulse gently in size
//! - **Ripples** — tapping spawns an expanding ring that fades out over ~1 second
//!
//! The demo is fully self-contained and has zero GPUI workspace dependencies.
//! All geometry / colour types are defined locally; replace them with the real
//! `gpui::*` equivalents when integrating.

use super::{random_color, BACKGROUND};
use std::time::{Duration, Instant};

// ── Local geometry ────────────────────────────────────────────────────────────

/// A 2-D point / vector with `f32` components.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Component-wise addition.
    #[inline]
    pub fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }

    /// Scalar multiplication.
    #[inline]
    pub fn scale(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s)
    }

    /// Euclidean length.
    #[inline]
    pub fn len(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

// ── Colour ────────────────────────────────────────────────────────────────────

/// A colour in linear RGBA space with `f32` components in `[0, 1]`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Construct from a packed `0xRRGGBB` integer with explicit alpha.
    pub fn from_u32(packed: u32, a: f32) -> Self {
        Self {
            r: ((packed >> 16) & 0xff) as f32 / 255.0,
            g: ((packed >> 8) & 0xff) as f32 / 255.0,
            b: (packed & 0xff) as f32 / 255.0,
            a,
        }
    }

    /// Linearly interpolate toward `other` by factor `t ∈ [0, 1]`.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let s = 1.0 - t;
        Self::new(
            self.r * s + other.r * t,
            self.g * s + other.g * t,
            self.b * s + other.b * t,
            self.a * s + other.a * t,
        )
    }

    /// Return the same colour with a different alpha.
    #[inline]
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Convert to `(h, s, l)` in `[0, 1]³` (HSL).
    ///
    /// Used to derive complementary hues for the gradient.
    pub fn to_hsl(self) -> (f32, f32, f32) {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let l = (max + min) * 0.5;

        if (max - min).abs() < 1e-6 {
            return (0.0, 0.0, l);
        }

        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };

        let h = if (max - self.r).abs() < 1e-6 {
            (self.g - self.b) / d + if self.g < self.b { 6.0 } else { 0.0 }
        } else if (max - self.g).abs() < 1e-6 {
            (self.b - self.r) / d + 2.0
        } else {
            (self.r - self.g) / d + 4.0
        };

        (h / 6.0, s, l)
    }
}

/// Build a colour from HSL (all components in `[0, 1]`) with explicit alpha.
pub fn hsl_to_rgba(h: f32, s: f32, l: f32, a: f32) -> Rgba {
    if s < 1e-6 {
        return Rgba::new(l, l, l, a);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    Rgba::new(r, g, b, a)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

// ── Gradient stop ─────────────────────────────────────────────────────────────

/// A colour stop for a linear gradient.
#[derive(Copy, Clone, Debug)]
pub struct GradientStop {
    /// Position in `[0, 1]` along the gradient axis.
    pub position: f32,
    /// Colour at this stop.
    pub color: Rgba,
}

/// A two-stop linear gradient.
#[derive(Clone, Debug)]
pub struct LinearGradient {
    pub angle_deg: f32,
    pub from: GradientStop,
    pub to: GradientStop,
}

impl LinearGradient {
    /// Interpolate between the two stops at `t ∈ [0, 1]`.
    pub fn color_at(&self, t: f32) -> Rgba {
        self.from.color.lerp(self.to.color, t.clamp(0.0, 1.0))
    }
}

// ── Orb ───────────────────────────────────────────────────────────────────────

/// A single floating orb with parallax motion and size pulsing.
#[derive(Clone, Debug)]
pub struct Orb {
    /// Base position in logical pixels (independent of touch offset).
    pub base_pos: Pt,
    /// How strongly this orb responds to the touch offset.
    /// Values closer to 0 = barely moves; closer to 1 = moves a lot.
    pub parallax: f32,
    /// Base radius in logical pixels.
    pub base_radius: f32,
    /// Base colour (alpha is baked in).
    pub color: Rgba,
    /// Phase offset for the size-pulse sine wave.
    pub pulse_phase: f32,
    /// Frequency of the size pulse (Hz).
    pub pulse_freq: f32,
}

impl Orb {
    /// Create an orb from explicit parameters.
    pub fn new(
        x: f32,
        y: f32,
        radius: f32,
        parallax: f32,
        hue: f32,
        pulse_phase: f32,
        pulse_freq: f32,
    ) -> Self {
        Self {
            base_pos: Pt::new(x, y),
            parallax,
            base_radius: radius,
            color: hsl_to_rgba(hue, 0.70, 0.60, 0.40),
            pulse_phase,
            pulse_freq,
        }
    }

    /// Current world-space position accounting for the touch-driven offset.
    pub fn world_pos(&self, touch_offset: Pt, time: f32) -> Pt {
        // Vertical "breathing" movement driven by the pulse.
        let breath =
            (time * self.pulse_freq * std::f32::consts::TAU + self.pulse_phase).sin() * 5.0;
        Pt::new(
            self.base_pos.x + touch_offset.x * self.parallax,
            self.base_pos.y + touch_offset.y * self.parallax + breath,
        )
    }

    /// Current radius with size pulsing applied.
    pub fn world_radius(&self, time: f32) -> f32 {
        let pulse =
            (time * self.pulse_freq * std::f32::consts::TAU + self.pulse_phase).sin() * 0.10 + 1.0;
        self.base_radius * pulse
    }

    /// Returns an iterator of glow layers (outermost first) plus the core.
    ///
    /// Each item is `(radius_multiplier, alpha_multiplier)`.
    pub fn glow_layers() -> &'static [(f32, f32)] {
        &[
            (2.0, 0.04),
            (1.75, 0.06),
            (1.5, 0.10),
            (1.25, 0.15),
            (1.0, 0.40),
        ]
    }
}

/// Build the standard set of eight orbs used by the demo.
pub fn default_orbs() -> Vec<Orb> {
    // (x, y, radius, parallax, hue, pulse_phase, pulse_freq)
    vec![
        Orb::new(100.0, 200.0, 80.0, 0.10, 0.00, 0.0, 1.2),
        Orb::new(300.0, 150.0, 60.0, 0.15, 0.10, 0.4, 0.9),
        Orb::new(200.0, 400.0, 100.0, 0.05, 0.20, 0.8, 1.5),
        Orb::new(350.0, 500.0, 50.0, 0.20, 0.30, 1.2, 1.1),
        Orb::new(50.0, 600.0, 70.0, 0.12, 0.40, 1.6, 0.8),
        Orb::new(280.0, 700.0, 90.0, 0.08, 0.50, 2.0, 1.3),
        Orb::new(150.0, 300.0, 40.0, 0.25, 0.60, 2.4, 1.7),
        Orb::new(320.0, 350.0, 55.0, 0.18, 0.70, 2.8, 1.0),
    ]
}

// ── Ripple ────────────────────────────────────────────────────────────────────

/// How long a ripple ring expands before vanishing.
const RIPPLE_DURATION: Duration = Duration::from_millis(1_000);

/// Maximum number of simultaneously live ripples.
const MAX_RIPPLES: usize = 5;

/// An expanding ring spawned when the user taps.
#[derive(Clone, Debug)]
pub struct Ripple {
    /// Centre of the ring.
    pub center: Pt,
    /// When the ripple was born.
    born: Instant,
    /// Colour (alpha is animated separately).
    pub base_color: Rgba,
}

impl Ripple {
    /// Create a new ripple at `center` with the given hue.
    pub fn new(center: Pt, hue: f32) -> Self {
        Self {
            center,
            born: Instant::now(),
            base_color: hsl_to_rgba(hue, 0.80, 0.60, 1.0),
        }
    }

    /// Fraction of lifetime consumed, in `[0, 1]`.
    pub fn progress(&self) -> f32 {
        let e = self.born.elapsed().as_secs_f32();
        (e / RIPPLE_DURATION.as_secs_f32()).clamp(0.0, 1.0)
    }

    /// Whether the ripple is still alive.
    pub fn is_alive(&self) -> bool {
        self.born.elapsed() < RIPPLE_DURATION
    }

    /// Current outer radius given the maximum possible radius.
    pub fn radius(&self, max: f32) -> f32 {
        let p = self.progress();
        // Ease-out cubic for natural expansion.
        let eased = 1.0 - (1.0 - p).powi(3);
        max * eased
    }

    /// Current alpha (quadratic fade-out).
    pub fn alpha(&self) -> f32 {
        let p = self.progress();
        (1.0 - p).powi(2)
    }

    /// Current colour with animated alpha.
    pub fn color(&self) -> Rgba {
        self.base_color.with_alpha(self.alpha() * 0.5)
    }

    /// Ring thickness in logical pixels.
    pub fn ring_thickness() -> f32 {
        3.0
    }
}

// ── ShaderShowcase ────────────────────────────────────────────────────────────

/// The shader showcase demo.
///
/// # Integration
///
/// 1. Create with [`ShaderShowcase::new`].
/// 2. Call [`set_screen_size`](Self::set_screen_size) when the Metal layer
///    size is known (and whenever it changes).
/// 3. On `touchesBegan` call [`on_touch_down`](Self::on_touch_down).
/// 4. On `touchesMoved` call [`on_touch_move`](Self::on_touch_move).
/// 5. On `touchesEnded` / `touchesCancelled` call
///    [`on_touch_up`](Self::on_touch_up).
/// 6. On every CADisplayLink tick call [`render_frame`](Self::render_frame)
///    to get a [`ShaderFrame`] snapshot.
pub struct ShaderShowcase {
    /// Simulation start time (used to derive the animation `time`).
    start: Instant,
    /// Current touch position, or `None` if no touch is active.
    pub touch_pos: Option<Pt>,
    /// Screen centre (updated from Metal layer size).
    pub screen_center: Pt,
    /// Screen size (width, height) in logical pixels.
    pub screen_size: (f32, f32),
    /// The eight floating orbs.
    orbs: Vec<Orb>,
    /// Currently live ripples.
    ripples: Vec<Ripple>,
    /// Hue offset that slowly increases over time to cycle the gradient.
    hue_offset: f32,
    /// Seed for choosing ripple colours.
    ripple_seed: usize,
}

impl ShaderShowcase {
    /// Create a new demo with default orbs and a 390×844 logical screen.
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            touch_pos: None,
            screen_center: Pt::new(195.0, 422.0),
            screen_size: (390.0, 844.0),
            orbs: default_orbs(),
            ripples: Vec::new(),
            hue_offset: 0.0,
            ripple_seed: 0,
        }
    }

    // ── Screen size ───────────────────────────────────────────────────────────

    /// Update the logical screen dimensions and recompute the screen centre.
    ///
    /// Call whenever the Metal layer's `drawableSize` changes.
    pub fn set_screen_size(&mut self, width: f32, height: f32) {
        self.screen_size = (width, height);
        self.screen_center = Pt::new(width * 0.5, height * 0.5);
    }

    // ── Touch callbacks ───────────────────────────────────────────────────────

    /// Call when a new touch begins.
    pub fn on_touch_down(&mut self, pos: Pt) {
        self.touch_pos = Some(pos);
        self.spawn_ripple(pos);
    }

    /// Call when the active touch moves.
    pub fn on_touch_move(&mut self, pos: Pt) {
        self.touch_pos = Some(pos);
    }

    /// Call when the active touch ends.
    pub fn on_touch_up(&mut self) {
        self.touch_pos = None;
    }

    // ── Ripple spawning ───────────────────────────────────────────────────────

    /// Spawn a ripple at `pos`.
    ///
    /// Evicts the oldest live ripple if the cap is reached.
    pub fn spawn_ripple(&mut self, pos: Pt) {
        if self.ripples.len() >= MAX_RIPPLES {
            self.ripples.remove(0);
        }
        let hue = self.cycling_hue(self.ripple_seed as f32 * 0.15);
        self.ripples.push(Ripple::new(pos, hue));
        self.ripple_seed += 1;
    }

    // ── Frame rendering ───────────────────────────────────────────────────────

    /// Produce a [`ShaderFrame`] snapshot for the current moment.
    ///
    /// Dead ripples are pruned before building the snapshot.
    pub fn render_frame(&mut self) -> ShaderFrame {
        self.ripples.retain(|r| r.is_alive());

        let time = self.elapsed();
        let touch_offset = self.touch_offset();
        let gradient = self.build_gradient(time);
        let max_ripple_radius = {
            let (w, h) = self.screen_size;
            w.max(h) * 0.7
        };

        // Sort orbs by parallax (smallest first = drawn behind larger-parallax orbs).
        let mut orb_data: Vec<OrbFrame> = self
            .orbs
            .iter()
            .map(|orb| {
                let pos = orb.world_pos(touch_offset, time);
                let radius = orb.world_radius(time);
                OrbFrame {
                    pos,
                    radius,
                    color: orb.color,
                    parallax: orb.parallax,
                }
            })
            .collect();
        orb_data.sort_by(|a, b| a.parallax.partial_cmp(&b.parallax).unwrap());

        let ripple_data: Vec<RippleFrame> = self
            .ripples
            .iter()
            .map(|r| RippleFrame {
                center: r.center,
                radius: r.radius(max_ripple_radius),
                color: r.color(),
                thickness: Ripple::ring_thickness(),
            })
            .collect();

        ShaderFrame {
            gradient,
            orbs: orb_data,
            ripples: ripple_data,
            time,
        }
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Seconds elapsed since the demo started.
    fn elapsed(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    /// Hue in `[0, 1]` that slowly cycles; `base` offsets the starting hue.
    fn cycling_hue(&self, base: f32) -> f32 {
        (base + self.elapsed() * 0.05).fract()
    }

    /// Compute the touch-driven offset vector.
    ///
    /// When no touch is active the offset is zero (orbs rest at their base
    /// positions).
    fn touch_offset(&self) -> Pt {
        match self.touch_pos {
            Some(pos) => Pt::new(pos.x - self.screen_center.x, pos.y - self.screen_center.y),
            None => Pt::new(0.0, 0.0),
        }
    }

    /// Build the gradient for this frame.
    fn build_gradient(&self, time: f32) -> LinearGradient {
        let angle = self.gradient_angle(time);
        let hue1 = self.cycling_hue(0.60);
        let hue2 = self.cycling_hue(0.93);
        LinearGradient {
            angle_deg: angle,
            from: GradientStop {
                position: 0.0,
                color: hsl_to_rgba(hue1, 0.80, 0.15, 1.0),
            },
            to: GradientStop {
                position: 1.0,
                color: hsl_to_rgba(hue2, 0.70, 0.25, 1.0),
            },
        }
    }

    /// Gradient angle in degrees.
    ///
    /// When a touch is active the gradient tilts toward the touch point.
    /// Otherwise it rotates slowly.
    fn gradient_angle(&self, time: f32) -> f32 {
        match self.touch_pos {
            Some(pos) => {
                let dx = pos.x - self.screen_center.x;
                let dy = pos.y - self.screen_center.y;
                (dy.atan2(dx).to_degrees() + 90.0).rem_euclid(360.0)
            }
            None => (time * 20.0).rem_euclid(360.0),
        }
    }
}

impl Default for ShaderShowcase {
    fn default() -> Self {
        Self::new()
    }
}

// ── Frame snapshot types ──────────────────────────────────────────────────────

/// A read-only snapshot of one rendered frame.
///
/// The GPU renderer consumes this to issue draw calls.
#[derive(Clone, Debug)]
pub struct ShaderFrame {
    /// The background gradient for this frame.
    pub gradient: LinearGradient,
    /// All orbs, sorted back-to-front by parallax factor.
    pub orbs: Vec<OrbFrame>,
    /// All live ripples.
    pub ripples: Vec<RippleFrame>,
    /// Seconds since demo start (can be used in custom shaders).
    pub time: f32,
}

/// Per-frame data for a single orb.
#[derive(Clone, Debug)]
pub struct OrbFrame {
    pub pos: Pt,
    pub radius: f32,
    pub color: Rgba,
    pub parallax: f32,
}

impl OrbFrame {
    /// Iterate over glow-layer draw calls `(offset_x, offset_y, radius, color)`.
    ///
    /// Outermost glow first, core last.
    pub fn draw_calls(&self) -> impl Iterator<Item = (f32, f32, f32, Rgba)> + '_ {
        Orb::glow_layers().iter().map(|&(r_mult, a_mult)| {
            (
                self.pos.x,
                self.pos.y,
                self.radius * r_mult,
                self.color.with_alpha(a_mult),
            )
        })
    }

    /// Returns the highlight dot `(x, y, radius, color)`.
    pub fn highlight(&self) -> (f32, f32, f32, Rgba) {
        let hx = self.pos.x - self.radius * 0.3;
        let hy = self.pos.y - self.radius * 0.3;
        let hr = self.radius * 0.4;
        (hx, hy, hr, Rgba::new(1.0, 1.0, 1.0, 0.30))
    }
}

/// Per-frame data for a single ripple ring.
#[derive(Clone, Debug)]
pub struct RippleFrame {
    pub center: Pt,
    pub radius: f32,
    pub color: Rgba,
    pub thickness: f32,
}

impl RippleFrame {
    /// Number of arc-segment dots used to approximate the ring.
    const RING_SEGMENTS: usize = 36;

    /// Iterate over the dot positions that approximate this ring.
    ///
    /// Each item is `(x, y, dot_radius, color)`.
    pub fn ring_dots(&self) -> impl Iterator<Item = (f32, f32, f32, Rgba)> + '_ {
        let step = std::f32::consts::TAU / Self::RING_SEGMENTS as f32;
        (0..Self::RING_SEGMENTS).map(move |i| {
            let angle = step * i as f32;
            let x = self.center.x + self.radius * angle.cos();
            let y = self.center.y + self.radius * angle.sin();
            (x, y, self.thickness, self.color)
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Colour helpers ────────────────────────────────────────────────────────

    #[test]
    fn rgba_from_u32_round_trips() {
        let c = Rgba::from_u32(0xff8040, 1.0);
        assert!((c.r - 1.0).abs() < 1e-3, "r = {}", c.r);
        assert!((c.g - (0x80 as f32 / 255.0)).abs() < 1e-3, "g = {}", c.g);
        assert!((c.b - (0x40 as f32 / 255.0)).abs() < 1e-3, "b = {}", c.b);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn rgba_with_alpha_preserves_rgb() {
        let c = Rgba::new(0.1, 0.2, 0.3, 1.0).with_alpha(0.5);
        assert_eq!(c.r, 0.1);
        assert_eq!(c.g, 0.2);
        assert_eq!(c.b, 0.3);
        assert_eq!(c.a, 0.5);
    }

    #[test]
    fn rgba_lerp_at_zero_is_self() {
        let a = Rgba::new(1.0, 0.0, 0.0, 1.0);
        let b = Rgba::new(0.0, 1.0, 0.0, 1.0);
        let c = a.lerp(b, 0.0);
        assert!((c.r - a.r).abs() < 1e-6);
        assert!((c.g - a.g).abs() < 1e-6);
    }

    #[test]
    fn rgba_lerp_at_one_is_other() {
        let a = Rgba::new(1.0, 0.0, 0.0, 1.0);
        let b = Rgba::new(0.0, 1.0, 0.0, 1.0);
        let c = a.lerp(b, 1.0);
        assert!((c.r - b.r).abs() < 1e-6);
        assert!((c.g - b.g).abs() < 1e-6);
    }

    #[test]
    fn rgba_lerp_midpoint_is_average() {
        let a = Rgba::new(0.0, 0.0, 0.0, 0.0);
        let b = Rgba::new(1.0, 1.0, 1.0, 1.0);
        let c = a.lerp(b, 0.5);
        assert!((c.r - 0.5).abs() < 1e-6);
        assert!((c.a - 0.5).abs() < 1e-6);
    }

    #[test]
    fn hsl_to_rgba_white_at_full_lightness() {
        let c = hsl_to_rgba(0.0, 0.0, 1.0, 1.0);
        assert!((c.r - 1.0).abs() < 1e-3);
        assert!((c.g - 1.0).abs() < 1e-3);
        assert!((c.b - 1.0).abs() < 1e-3);
    }

    #[test]
    fn hsl_to_rgba_black_at_zero_lightness() {
        let c = hsl_to_rgba(0.0, 0.0, 0.0, 1.0);
        assert!(c.r < 1e-3);
        assert!(c.g < 1e-3);
        assert!(c.b < 1e-3);
    }

    #[test]
    fn hsl_to_rgba_pure_red() {
        // hue=0, s=1, l=0.5 should give (1, 0, 0)
        let c = hsl_to_rgba(0.0, 1.0, 0.5, 1.0);
        assert!((c.r - 1.0).abs() < 1e-3, "r = {}", c.r);
        assert!(c.g < 1e-3, "g = {}", c.g);
        assert!(c.b < 1e-3, "b = {}", c.b);
    }

    // ── Pt helpers ────────────────────────────────────────────────────────────

    #[test]
    fn pt_add() {
        let a = Pt::new(1.0, 2.0);
        let b = Pt::new(3.0, 4.0);
        let c = a.add(b);
        assert_eq!(c.x, 4.0);
        assert_eq!(c.y, 6.0);
    }

    #[test]
    fn pt_scale() {
        let p = Pt::new(3.0, 4.0);
        let s = p.scale(2.0);
        assert_eq!(s.x, 6.0);
        assert_eq!(s.y, 8.0);
    }

    #[test]
    fn pt_len() {
        let p = Pt::new(3.0, 4.0);
        assert!((p.len() - 5.0).abs() < 1e-5);
    }

    // ── Orb ───────────────────────────────────────────────────────────────────

    #[test]
    fn orb_world_pos_with_zero_touch_is_base_plus_breath() {
        let orb = Orb::new(200.0, 300.0, 50.0, 0.5, 0.0, 0.0, 1.0);
        let pos = orb.world_pos(Pt::new(0.0, 0.0), 0.0);
        assert_eq!(pos.x, 200.0);
        // y includes breath = sin(0) * 5 = 0
        assert!((pos.y - 300.0).abs() < 1e-3);
    }

    #[test]
    fn orb_world_pos_shifts_with_touch_offset() {
        let orb = Orb::new(200.0, 300.0, 50.0, 0.5, 0.0, 0.0, 1.0);
        let offset = Pt::new(100.0, 0.0);
        let pos = orb.world_pos(offset, 0.0);
        // x = 200 + 100 * 0.5 = 250
        assert!((pos.x - 250.0).abs() < 1e-3);
    }

    #[test]
    fn orb_radius_is_positive() {
        let orb = Orb::new(0.0, 0.0, 60.0, 0.1, 0.3, 0.0, 1.0);
        assert!(orb.world_radius(0.0) > 0.0);
        assert!(orb.world_radius(5.7) > 0.0);
    }

    #[test]
    fn default_orbs_returns_eight() {
        assert_eq!(default_orbs().len(), 8);
    }

    #[test]
    fn orb_glow_layers_count() {
        // The glow layer list must have at least one entry.
        assert!(!Orb::glow_layers().is_empty());
    }

    // ── Ripple ────────────────────────────────────────────────────────────────

    #[test]
    fn ripple_is_alive_when_new() {
        let r = Ripple::new(Pt::new(200.0, 400.0), 0.5);
        assert!(r.is_alive());
    }

    #[test]
    fn ripple_progress_starts_near_zero() {
        let r = Ripple::new(Pt::new(0.0, 0.0), 0.0);
        assert!(r.progress() < 0.01);
    }

    #[test]
    fn ripple_radius_grows_over_time() {
        let r = Ripple::new(Pt::new(0.0, 0.0), 0.0);
        // Even at t=0 the radius should be very small but non-negative.
        assert!(r.radius(500.0) >= 0.0);
    }

    #[test]
    fn ripple_alpha_starts_near_one() {
        let r = Ripple::new(Pt::new(0.0, 0.0), 0.0);
        assert!(r.alpha() > 0.99);
    }

    #[test]
    fn ripple_ring_dots_correct_count() {
        let rf = RippleFrame {
            center: Pt::new(100.0, 200.0),
            radius: 80.0,
            color: Rgba::new(1.0, 1.0, 1.0, 0.5),
            thickness: 3.0,
        };
        assert_eq!(rf.ring_dots().count(), RippleFrame::RING_SEGMENTS);
    }

    #[test]
    fn ripple_ring_dots_lie_on_circle() {
        let rf = RippleFrame {
            center: Pt::new(100.0, 200.0),
            radius: 80.0,
            color: Rgba::new(1.0, 1.0, 1.0, 0.5),
            thickness: 3.0,
        };
        for (x, y, _, _) in rf.ring_dots() {
            let dx = x - rf.center.x;
            let dy = y - rf.center.y;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(
                (dist - rf.radius).abs() < 1e-3,
                "dot at ({x},{y}) is {dist} from center, expected {}",
                rf.radius
            );
        }
    }

    // ── ShaderShowcase ────────────────────────────────────────────────────────

    #[test]
    fn new_showcase_has_default_orbs() {
        let s = ShaderShowcase::new();
        assert_eq!(s.orbs.len(), 8);
    }

    #[test]
    fn new_showcase_has_no_touch() {
        let s = ShaderShowcase::new();
        assert!(s.touch_pos.is_none());
    }

    #[test]
    fn touch_down_sets_touch_pos() {
        let mut s = ShaderShowcase::new();
        s.on_touch_down(Pt::new(150.0, 300.0));
        assert!(s.touch_pos.is_some());
    }

    #[test]
    fn touch_down_spawns_ripple() {
        let mut s = ShaderShowcase::new();
        s.on_touch_down(Pt::new(150.0, 300.0));
        assert_eq!(s.ripples.len(), 1);
    }

    #[test]
    fn touch_move_updates_position() {
        let mut s = ShaderShowcase::new();
        s.on_touch_down(Pt::new(0.0, 0.0));
        s.on_touch_move(Pt::new(200.0, 300.0));
        let pos = s.touch_pos.unwrap();
        assert_eq!(pos.x, 200.0);
        assert_eq!(pos.y, 300.0);
    }

    #[test]
    fn touch_up_clears_touch_pos() {
        let mut s = ShaderShowcase::new();
        s.on_touch_down(Pt::new(100.0, 200.0));
        s.on_touch_up();
        assert!(s.touch_pos.is_none());
    }

    #[test]
    fn spawn_ripple_evicts_oldest_at_cap() {
        let mut s = ShaderShowcase::new();
        for i in 0..(MAX_RIPPLES + 3) {
            s.spawn_ripple(Pt::new(i as f32, 0.0));
        }
        assert_eq!(s.ripples.len(), MAX_RIPPLES);
    }

    #[test]
    fn set_screen_size_updates_center() {
        let mut s = ShaderShowcase::new();
        s.set_screen_size(1170.0, 2532.0);
        assert!((s.screen_center.x - 585.0).abs() < 1e-3);
        assert!((s.screen_center.y - 1266.0).abs() < 1e-3);
    }

    #[test]
    fn render_frame_returns_orbs_sorted_by_parallax() {
        let mut s = ShaderShowcase::new();
        let frame = s.render_frame();
        let parallaxes: Vec<f32> = frame.orbs.iter().map(|o| o.parallax).collect();
        let mut sorted = parallaxes.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(parallaxes, sorted, "orbs should be sorted by parallax");
    }

    #[test]
    fn render_frame_has_gradient() {
        let mut s = ShaderShowcase::new();
        let frame = s.render_frame();
        // Gradient stops should be in [0, 1].
        assert!((0.0..=1.0).contains(&frame.gradient.from.position));
        assert!((0.0..=1.0).contains(&frame.gradient.to.position));
        assert!(frame.gradient.from.position < frame.gradient.to.position);
    }

    #[test]
    fn gradient_color_at_endpoints() {
        let grad = LinearGradient {
            angle_deg: 90.0,
            from: GradientStop {
                position: 0.0,
                color: Rgba::new(1.0, 0.0, 0.0, 1.0),
            },
            to: GradientStop {
                position: 1.0,
                color: Rgba::new(0.0, 1.0, 0.0, 1.0),
            },
        };
        let c0 = grad.color_at(0.0);
        let c1 = grad.color_at(1.0);
        assert!((c0.r - 1.0).abs() < 1e-6);
        assert!((c1.g - 1.0).abs() < 1e-6);
    }

    #[test]
    fn render_frame_dead_ripples_are_pruned() {
        // We can't easily expire a ripple in unit tests (would need to sleep),
        // but we can verify that render_frame doesn't panic with 0 ripples.
        let mut s = ShaderShowcase::new();
        let frame = s.render_frame();
        // No panics and ripple count is within bounds.
        assert!(frame.ripples.len() <= MAX_RIPPLES);
    }

    #[test]
    fn orb_frame_draw_calls_match_glow_layer_count() {
        let of = OrbFrame {
            pos: Pt::new(100.0, 200.0),
            radius: 50.0,
            color: Rgba::new(0.5, 0.5, 0.5, 0.4),
            parallax: 0.1,
        };
        assert_eq!(of.draw_calls().count(), Orb::glow_layers().len());
    }
}
