//! Animation Playground — bouncing balls with physics simulation and particle effects.
//!
//! This demo showcases GPUI's animation and rendering capabilities on iOS:
//!
//! - **Tap**  → burst of coloured particles at the touch point
//! - **Drag** → throw a ball with velocity proportional to the swipe speed
//! - Balls obey gravity, bounce off walls with damping, and leave colour trails
//! - Up to [`MAX_BALLS`] balls and [`MAX_PARTICLES`] particles are alive at once
//!
//! The demo is self-contained: it only depends on the types defined in the
//! sibling `demos` modules and the standard library.  No GPUI workspace dep
//! is required — when integrated, replace the local geometry types with the
//! real `gpui::*` equivalents.

use super::{random_color, BACKGROUND, TEXT};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

// ── Physics constants ─────────────────────────────────────────────────────────

/// Gravitational acceleration in logical pixels per second².
const GRAVITY: f32 = 980.0;
/// Fraction of velocity retained after bouncing off a wall.
const BOUNCE_DAMPING: f32 = 0.70;
/// Per-frame velocity multiplier (air friction).
const FRICTION: f32 = 0.995;
/// Maximum number of live balls.
const MAX_BALLS: usize = 30;
/// Number of trail positions stored per ball.
const TRAIL_LENGTH: usize = 12;
/// Radius of each ball in logical pixels.
const BALL_RADIUS: f32 = 20.0;

// ── Particle constants ────────────────────────────────────────────────────────

/// Number of particles spawned per burst.
const PARTICLE_BURST: usize = 12;
/// Maximum number of live particles.
const MAX_PARTICLES: usize = MAX_BALLS * PARTICLE_BURST;
/// How long a single particle lives.
const PARTICLE_DURATION: Duration = Duration::from_millis(600);

// ── Local geometry types ──────────────────────────────────────────────────────
// (Replace with gpui::Point / gpui::Size when integrating into GPUI proper.)

/// A 2-D point with `f32` components.
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

    /// Euclidean distance to another point.
    #[inline]
    pub fn dist(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// An axis-aligned bounding box.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Pt,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    #[inline]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Pt::new(x, y),
            width,
            height,
        }
    }
}

// ── Colour (RGBA f32) ─────────────────────────────────────────────────────────

/// A colour in linear RGBA space with `f32` components in `[0, 1]`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Construct from a packed `0xRRGGBB` integer with explicit alpha.
    pub fn from_rgb_u32(packed: u32, a: f32) -> Self {
        let r = ((packed >> 16) & 0xff) as f32 / 255.0;
        let g = ((packed >> 8) & 0xff) as f32 / 255.0;
        let b = (packed & 0xff) as f32 / 255.0;
        Self { r, g, b, a }
    }

    /// Return the same colour with a different alpha value.
    #[inline]
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }
}

// ── Ball ──────────────────────────────────────────────────────────────────────

/// A single bouncing ball.
#[derive(Clone, Debug)]
pub struct Ball {
    /// Current position in logical pixels.
    pub pos: Pt,
    /// Current velocity in logical pixels per second.
    pub vel: Pt,
    /// Ball radius in logical pixels.
    pub radius: f32,
    /// Packed RGB colour (`0xRRGGBB`).
    pub color_rgb: u32,
    /// Ring buffer of recent positions used to draw the motion trail.
    pub trail: VecDeque<Pt>,
    /// Unique identifier (used to pick a colour from the palette).
    pub id: usize,
}

impl Ball {
    /// Create a new ball at `pos` with `vel`.
    pub fn new(id: usize, pos: Pt, vel: Pt) -> Self {
        Self {
            pos,
            vel,
            radius: BALL_RADIUS,
            color_rgb: random_color(id),
            trail: VecDeque::with_capacity(TRAIL_LENGTH),
            id,
        }
    }

    /// Advance the ball by `dt` seconds inside `bounds`.
    pub fn update(&mut self, dt: f32, bounds: &Rect) {
        // Record trail position before moving.
        if self.trail.len() >= TRAIL_LENGTH {
            self.trail.pop_front();
        }
        self.trail.push_back(self.pos);

        // Gravity.
        self.vel.y += GRAVITY * dt;
        // Air friction.
        self.vel.x *= FRICTION;
        self.vel.y *= FRICTION;
        // Integrate.
        self.pos.x += self.vel.x * dt;
        self.pos.y += self.vel.y * dt;

        // Bounce off edges.
        let min_x = bounds.origin.x + self.radius;
        let max_x = bounds.origin.x + bounds.width - self.radius;
        let min_y = bounds.origin.y + self.radius;
        let max_y = bounds.origin.y + bounds.height - self.radius;

        if self.pos.x < min_x {
            self.pos.x = min_x;
            self.vel.x = self.vel.x.abs() * BOUNCE_DAMPING;
        } else if self.pos.x > max_x {
            self.pos.x = max_x;
            self.vel.x = -self.vel.x.abs() * BOUNCE_DAMPING;
        }

        if self.pos.y < min_y {
            self.pos.y = min_y;
            self.vel.y = self.vel.y.abs() * BOUNCE_DAMPING;
        } else if self.pos.y > max_y {
            self.pos.y = max_y;
            self.vel.y = -self.vel.y.abs() * BOUNCE_DAMPING;
        }
    }

    /// Returns the ball colour as [`Rgba`] with full opacity.
    pub fn color(&self) -> Rgba {
        Rgba::from_rgb_u32(self.color_rgb, 1.0)
    }
}

// ── Particle ──────────────────────────────────────────────────────────────────

/// A single short-lived particle in a burst effect.
#[derive(Clone, Debug)]
pub struct Particle {
    /// Current position.
    pub pos: Pt,
    /// Velocity in px/s.
    vel: Pt,
    /// When the particle was born.
    born: Instant,
    /// Packed RGB colour.
    color_rgb: u32,
    /// Radius in logical pixels.
    size: f32,
}

impl Particle {
    /// Spawn a particle at `pos` moving in direction `angle` (radians) at `speed` px/s.
    pub fn new(pos: Pt, angle: f32, speed: f32, color_rgb: u32) -> Self {
        Self {
            pos,
            vel: Pt::new(angle.cos() * speed, angle.sin() * speed),
            born: Instant::now(),
            color_rgb,
            size: 8.0,
        }
    }

    /// Fraction of lifetime consumed, in `[0.0, 1.0]`.
    pub fn progress(&self) -> f32 {
        let elapsed = self.born.elapsed().as_secs_f32();
        (elapsed / PARTICLE_DURATION.as_secs_f32()).clamp(0.0, 1.0)
    }

    /// Whether the particle is still alive.
    pub fn is_alive(&self) -> bool {
        self.born.elapsed() < PARTICLE_DURATION
    }

    /// Advance particle physics by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        self.pos.x += self.vel.x * dt;
        self.pos.y += self.vel.y * dt;
        // Decelerate.
        self.vel.x *= 0.98;
        self.vel.y *= 0.98;
    }

    /// Current colour with alpha fading to 0 as progress → 1.
    pub fn color(&self) -> Rgba {
        let alpha = 1.0 - self.progress();
        Rgba::from_rgb_u32(self.color_rgb, alpha)
    }

    /// Current radius, shrinking toward 50 % of its birth size.
    pub fn current_size(&self) -> f32 {
        self.size * (1.0 - self.progress() * 0.5)
    }
}

// ── Drag state ────────────────────────────────────────────────────────────────

/// Records the start of a touch drag so we can compute throw velocity.
#[derive(Clone, Debug)]
pub struct DragState {
    pub start_pos: Pt,
    pub start_time: Instant,
    pub current_pos: Pt,
}

// ── AnimationPlayground ───────────────────────────────────────────────────────

/// The animation playground demo.
///
/// # Integration
///
/// 1. Create with [`AnimationPlayground::new`].
/// 2. On every `touchesBegan`, call [`on_touch_down`](Self::on_touch_down).
/// 3. On every `touchesMoved`, call [`on_touch_move`](Self::on_touch_move).
/// 4. On every `touchesEnded` / `touchesCancelled`, call
///    [`on_touch_up`](Self::on_touch_up).
/// 5. On every CADisplayLink tick call [`tick`](Self::tick) with `dt`.
/// 6. Call [`render_frame`](Self::render_frame) to obtain a snapshot of all
///    draw calls for the current frame.
pub struct AnimationPlayground {
    balls: Vec<Ball>,
    particles: Vec<Particle>,
    drag: Option<DragState>,
    next_id: usize,
    /// Physics simulation bounds (updated from the Metal layer size).
    pub bounds: Rect,
    last_tick: Instant,
}

impl AnimationPlayground {
    /// Create a new, empty playground.
    pub fn new() -> Self {
        Self {
            balls: Vec::new(),
            particles: Vec::new(),
            drag: None,
            next_id: 0,
            bounds: Rect::new(0.0, 0.0, 390.0, 844.0), // iPhone 14 logical size
            last_tick: Instant::now(),
        }
    }

    // ── Touch callbacks ───────────────────────────────────────────────────────

    /// Call when a new touch begins.
    pub fn on_touch_down(&mut self, pos: Pt) {
        self.drag = Some(DragState {
            start_pos: pos,
            start_time: Instant::now(),
            current_pos: pos,
        });
    }

    /// Call when the touch moves.
    pub fn on_touch_move(&mut self, pos: Pt) {
        if let Some(ref mut d) = self.drag {
            d.current_pos = pos;
        }
    }

    /// Call when the touch ends.
    ///
    /// Short taps (< 200 ms, < 20 px travel) spawn a particle burst; longer
    /// drags throw a ball.
    pub fn on_touch_up(&mut self, pos: Pt) {
        if let Some(drag) = self.drag.take() {
            let elapsed = drag.start_time.elapsed();
            let distance = pos.dist(drag.start_pos);

            if elapsed < Duration::from_millis(200) && distance < 20.0 {
                // Short tap → particle burst.
                self.spawn_particles(pos, random_color(self.next_id));
                self.next_id += 1;
            } else {
                // Drag → throw a ball.
                let dt = elapsed.as_secs_f32().max(0.01);
                let vel = Pt::new(
                    (pos.x - drag.start_pos.x) / dt * 0.5,
                    (pos.y - drag.start_pos.y) / dt * 0.5,
                );
                self.spawn_ball(drag.start_pos, vel);
            }
        }
    }

    // ── Spawning helpers ──────────────────────────────────────────────────────

    /// Spawn a ball at `pos` with `vel`.  Evicts the oldest ball when the
    /// cap is reached.
    pub fn spawn_ball(&mut self, pos: Pt, vel: Pt) {
        if self.balls.len() >= MAX_BALLS {
            self.balls.remove(0);
        }
        self.balls.push(Ball::new(self.next_id, pos, vel));
        self.next_id += 1;
    }

    /// Spawn [`PARTICLE_BURST`] particles radiating from `pos`.
    pub fn spawn_particles(&mut self, pos: Pt, color_rgb: u32) {
        let step = std::f32::consts::TAU / PARTICLE_BURST as f32;
        for i in 0..PARTICLE_BURST {
            if self.particles.len() >= MAX_PARTICLES {
                self.particles.remove(0);
            }
            let angle = step * i as f32;
            let speed = 200.0 + i as f32 * 20.0;
            self.particles
                .push(Particle::new(pos, angle, speed, color_rgb));
        }
    }

    // ── Physics tick ─────────────────────────────────────────────────────────

    /// Advance physics by `dt` seconds.
    ///
    /// `dt` is clamped to 50 ms to prevent large jumps after the app resumes
    /// from the background.
    ///
    /// If you pass `None` for `dt`, the method computes it from the last call.
    pub fn tick(&mut self, dt: Option<f32>) {
        let dt = dt.unwrap_or_else(|| {
            let now = Instant::now();
            let d = now.duration_since(self.last_tick).as_secs_f32();
            self.last_tick = now;
            d
        });
        let dt = dt.min(0.05);

        for ball in &mut self.balls {
            ball.update(dt, &self.bounds);
        }
        for particle in &mut self.particles {
            particle.update(dt);
        }
        self.particles.retain(|p| p.is_alive());
    }

    // ── Frame snapshot ────────────────────────────────────────────────────────

    /// Return a [`FrameSnapshot`] describing everything that should be drawn
    /// this frame.
    ///
    /// The snapshot is a cheap clone of the current simulation state and is
    /// safe to hand off to a renderer on any thread.
    pub fn render_frame(&self) -> FrameSnapshot {
        FrameSnapshot {
            balls: self.balls.clone(),
            particles: self.particles.clone(),
            drag_start: self.drag.as_ref().map(|d| d.start_pos),
            drag_current: self.drag.as_ref().map(|d| d.current_pos),
            background_rgb: BACKGROUND,
            text_rgb: TEXT,
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Number of currently live balls.
    pub fn ball_count(&self) -> usize {
        self.balls.len()
    }

    /// Number of currently live particles.
    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    /// Whether a touch drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Update the physics bounds to match the current Metal layer size.
    pub fn set_bounds(&mut self, width: f32, height: f32) {
        self.bounds = Rect::new(0.0, 0.0, width, height);
    }
}

impl Default for AnimationPlayground {
    fn default() -> Self {
        Self::new()
    }
}

// ── FrameSnapshot ─────────────────────────────────────────────────────────────

/// A read-only snapshot of one animation frame.
///
/// Renderers consume this value to produce GPU draw calls.
#[derive(Clone, Debug)]
pub struct FrameSnapshot {
    /// All live balls (including their trails).
    pub balls: Vec<Ball>,
    /// All live particles.
    pub particles: Vec<Particle>,
    /// Touch start position (for drawing the drag line), if a drag is active.
    pub drag_start: Option<Pt>,
    /// Current touch position (for drawing the drag line), if a drag is active.
    pub drag_current: Option<Pt>,
    /// Background colour (`0xRRGGBB`).
    pub background_rgb: u32,
    /// UI text colour (`0xRRGGBB`).
    pub text_rgb: u32,
}

impl FrameSnapshot {
    /// Iterate over every circle (ball body + trail + particles) that needs
    /// to be drawn, yielding `(center_x, center_y, radius, Rgba)` tuples in
    /// painter's order (back to front).
    pub fn circles(&self) -> impl Iterator<Item = (f32, f32, f32, Rgba)> + '_ {
        let trail_circles = self.balls.iter().flat_map(|ball| {
            let n = ball.trail.len();
            ball.trail.iter().enumerate().map(move |(i, &pos)| {
                let frac = (i + 1) as f32 / TRAIL_LENGTH as f32;
                let alpha = frac * 0.4;
                let radius = ball.radius * (0.3 + 0.7 * frac);
                let color = Rgba::from_rgb_u32(ball.color_rgb, alpha);
                (pos.x, pos.y, radius, color)
            })
        });

        let particle_circles = self
            .particles
            .iter()
            .map(|p| (p.pos.x, p.pos.y, p.current_size(), p.color()));

        let ball_circles = self.balls.iter().flat_map(|ball| {
            let body = (ball.pos.x, ball.pos.y, ball.radius, ball.color());
            // Specular highlight dot.
            let highlight = Rgba::from_rgb_u32(0xffffff, 0.5);
            let hx = ball.pos.x - ball.radius * 0.3;
            let hy = ball.pos.y - ball.radius * 0.3;
            let hr = ball.radius * 0.3;
            [body, (hx, hy, hr, highlight)]
        });

        trail_circles.chain(particle_circles).chain(ball_circles)
    }

    /// Returns the drag line as `(x0, y0, x1, y1)` if a drag is active.
    pub fn drag_line(&self) -> Option<(f32, f32, f32, f32)> {
        match (self.drag_start, self.drag_current) {
            (Some(a), Some(b)) => Some((a.x, a.y, b.x, b.y)),
            _ => None,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Ball physics ──────────────────────────────────────────────────────────

    #[test]
    fn ball_falls_due_to_gravity() {
        let bounds = Rect::new(0.0, 0.0, 400.0, 800.0);
        let mut ball = Ball::new(0, Pt::new(200.0, 100.0), Pt::new(0.0, 0.0));
        let y_before = ball.pos.y;
        ball.update(0.1, &bounds);
        assert!(ball.pos.y > y_before, "ball should fall under gravity");
    }

    #[test]
    fn ball_bounces_off_floor() {
        let bounds = Rect::new(0.0, 0.0, 400.0, 800.0);
        // Place ball just below the floor with downward velocity.
        let mut ball = Ball::new(0, Pt::new(200.0, 790.0), Pt::new(0.0, 500.0));
        for _ in 0..20 {
            ball.update(0.016, &bounds);
        }
        // Ball should stay within bounds after bouncing.
        assert!(
            ball.pos.y <= 800.0 - ball.radius,
            "ball should not escape through floor"
        );
    }

    #[test]
    fn ball_bounces_off_left_wall() {
        let bounds = Rect::new(0.0, 0.0, 400.0, 800.0);
        let mut ball = Ball::new(0, Pt::new(5.0, 400.0), Pt::new(-500.0, 0.0));
        ball.update(0.1, &bounds);
        assert!(
            ball.vel.x > 0.0,
            "velocity should reverse when bouncing off left wall"
        );
    }

    #[test]
    fn ball_trail_grows_up_to_limit() {
        let bounds = Rect::new(0.0, 0.0, 400.0, 800.0);
        let mut ball = Ball::new(0, Pt::new(200.0, 200.0), Pt::new(0.0, 0.0));
        for _ in 0..(TRAIL_LENGTH + 5) {
            ball.update(0.016, &bounds);
        }
        assert!(
            ball.trail.len() <= TRAIL_LENGTH,
            "trail should not exceed TRAIL_LENGTH"
        );
    }

    // ── Particle lifecycle ────────────────────────────────────────────────────

    #[test]
    fn particle_progress_starts_at_zero() {
        let p = Particle::new(Pt::new(0.0, 0.0), 0.0, 100.0, 0xff0000);
        assert!(p.progress() < 0.01);
    }

    #[test]
    fn particle_is_alive_when_new() {
        let p = Particle::new(Pt::new(0.0, 0.0), 0.0, 100.0, 0xff0000);
        assert!(p.is_alive());
    }

    #[test]
    fn particle_moves_in_correct_direction() {
        let mut p = Particle::new(Pt::new(0.0, 0.0), 0.0, 100.0, 0x00ff00); // angle = 0 → +x
        let x_before = p.pos.x;
        p.update(0.1);
        assert!(p.pos.x > x_before, "particle should move in +x direction");
    }

    #[test]
    fn particle_alpha_decreases_over_time() {
        let mut p = Particle::new(Pt::new(0.0, 0.0), 0.0, 10.0, 0x0000ff);
        let alpha_start = p.color().a;
        // Simulate ~half the particle's lifetime.
        std::thread::sleep(Duration::from_millis(300));
        let alpha_later = p.color().a;
        assert!(
            alpha_later < alpha_start,
            "alpha should decrease as particle ages"
        );
    }

    // ── AnimationPlayground ───────────────────────────────────────────────────

    #[test]
    fn new_playground_is_empty() {
        let pg = AnimationPlayground::new();
        assert_eq!(pg.ball_count(), 0);
        assert_eq!(pg.particle_count(), 0);
        assert!(!pg.is_dragging());
    }

    #[test]
    fn spawn_ball_adds_one_ball() {
        let mut pg = AnimationPlayground::new();
        pg.spawn_ball(Pt::new(100.0, 100.0), Pt::new(0.0, 0.0));
        assert_eq!(pg.ball_count(), 1);
    }

    #[test]
    fn spawn_ball_evicts_oldest_when_at_cap() {
        let mut pg = AnimationPlayground::new();
        for i in 0..(MAX_BALLS + 5) {
            pg.spawn_ball(Pt::new(i as f32, 0.0), Pt::new(0.0, 0.0));
        }
        assert_eq!(pg.ball_count(), MAX_BALLS);
    }

    #[test]
    fn spawn_particles_adds_burst() {
        let mut pg = AnimationPlayground::new();
        pg.spawn_particles(Pt::new(200.0, 400.0), 0xff0000);
        assert_eq!(pg.particle_count(), PARTICLE_BURST);
    }

    #[test]
    fn short_tap_spawns_particles_not_ball() {
        let mut pg = AnimationPlayground::new();
        pg.on_touch_down(Pt::new(200.0, 300.0));
        // End touch immediately at same position — counts as a tap.
        pg.on_touch_up(Pt::new(200.0, 300.0));
        assert_eq!(pg.ball_count(), 0, "tap should not spawn a ball");
        assert_eq!(
            pg.particle_count(),
            PARTICLE_BURST,
            "tap should spawn a particle burst"
        );
    }

    #[test]
    fn drag_spawns_ball() {
        let mut pg = AnimationPlayground::new();
        pg.on_touch_down(Pt::new(50.0, 400.0));
        // Simulate moving the touch — not strictly required but matches real use.
        pg.on_touch_move(Pt::new(100.0, 350.0));
        // Sleep a little so elapsed > 200 ms threshold.
        std::thread::sleep(Duration::from_millis(250));
        pg.on_touch_up(Pt::new(300.0, 200.0)); // large travel distance
        assert_eq!(pg.ball_count(), 1, "drag should spawn exactly one ball");
    }

    #[test]
    fn tick_advances_physics() {
        let mut pg = AnimationPlayground::new();
        pg.spawn_ball(Pt::new(200.0, 100.0), Pt::new(0.0, 0.0));
        let y_before = pg.balls[0].pos.y;
        pg.tick(Some(0.1));
        assert!(
            pg.balls[0].pos.y > y_before,
            "tick should advance ball position under gravity"
        );
    }

    #[test]
    fn tick_with_none_dt_does_not_panic() {
        let mut pg = AnimationPlayground::new();
        pg.spawn_ball(Pt::new(200.0, 400.0), Pt::new(10.0, -10.0));
        // Should not panic even without an explicit dt.
        pg.tick(None);
    }

    #[test]
    fn set_bounds_updates_physics_area() {
        let mut pg = AnimationPlayground::new();
        pg.set_bounds(1170.0, 2532.0);
        assert_eq!(pg.bounds.width, 1170.0);
        assert_eq!(pg.bounds.height, 2532.0);
    }

    // ── FrameSnapshot ─────────────────────────────────────────────────────────

    #[test]
    fn frame_snapshot_circles_includes_balls() {
        let mut pg = AnimationPlayground::new();
        pg.spawn_ball(Pt::new(200.0, 400.0), Pt::new(0.0, 0.0));
        let snap = pg.render_frame();
        // With no trail yet, there should be at least 2 circles:
        // the ball body + the specular highlight.
        let count = snap.circles().count();
        assert!(count >= 2, "expected at least 2 circles for one ball");
    }

    #[test]
    fn frame_snapshot_drag_line_absent_when_no_drag() {
        let pg = AnimationPlayground::new();
        let snap = pg.render_frame();
        assert!(snap.drag_line().is_none());
    }

    #[test]
    fn frame_snapshot_drag_line_present_during_drag() {
        let mut pg = AnimationPlayground::new();
        pg.on_touch_down(Pt::new(10.0, 10.0));
        pg.on_touch_move(Pt::new(50.0, 80.0));
        let snap = pg.render_frame();
        let line = snap.drag_line();
        assert!(line.is_some(), "drag line should be present during a drag");
        let (x0, y0, x1, y1) = line.unwrap();
        assert_eq!(x0, 10.0);
        assert_eq!(y0, 10.0);
        assert_eq!(x1, 50.0);
        assert_eq!(y1, 80.0);
    }

    // ── Rgba helpers ─────────────────────────────────────────────────────────

    #[test]
    fn rgba_from_u32_parses_correctly() {
        let c = Rgba::from_rgb_u32(0xff8800, 1.0);
        assert!((c.r - 1.0).abs() < 1e-3);
        assert!((c.g - (0x88 as f32 / 255.0)).abs() < 1e-3);
        assert!((c.b - 0.0).abs() < 1e-3);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn rgba_with_alpha_keeps_rgb() {
        let c = Rgba::new(0.2, 0.4, 0.6, 1.0).with_alpha(0.5);
        assert_eq!(c.r, 0.2);
        assert_eq!(c.g, 0.4);
        assert_eq!(c.b, 0.6);
        assert_eq!(c.a, 0.5);
    }
}
