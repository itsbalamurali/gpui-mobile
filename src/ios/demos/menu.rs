//! Demo app root navigation menu for iOS.
//!
//! Provides the top-level [`DemoApp`] view that lets the user navigate between
//! the two interactive demos:
//!
//! - **Animation Playground** — bouncing balls with physics + particle effects
//! - **Shader Showcase** — dynamic gradients, floating orbs, and ripple effects
//!
//! Navigation is modelled as a simple enum-driven state machine so it
//! compiles without a dependency on the full GPUI workspace crate.
//!
//! # Integration
//!
//! ```rust,no_run
//! # #[cfg(target_os = "ios")]
//! # {
//! use gpui_mobile::ios::demos::DemoApp;
//!
//! let mut app = DemoApp::new();
//! // On every CADisplayLink tick:
//! let frame = app.current_view_mut().render_frame_if_animation();
//! // … hand `frame` to the Metal renderer …
//! # }
//! ```

use super::{
    animation_playground::{AnimationPlayground, FrameSnapshot as AnimFrame, Pt as AnimPt},
    shader_showcase::{Pt as ShaderPt, ShaderFrame, ShaderShowcase},
    BACKGROUND, BLUE, MAUVE, OVERLAY, SUBTEXT, SURFACE, TEXT,
};

// ── ActiveView ────────────────────────────────────────────────────────────────

/// Which view is currently displayed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActiveView {
    /// The top-level menu.
    #[default]
    Menu,
    /// Animation Playground demo.
    AnimationPlayground,
    /// Shader Showcase demo.
    ShaderShowcase,
}

// ── MenuFrame ─────────────────────────────────────────────────────────────────

/// A snapshot of the menu's visual state for one render frame.
///
/// Renderers use this to draw the menu without holding a mutable reference
/// to [`DemoApp`].
#[derive(Clone, Debug)]
pub struct MenuFrame {
    /// Background fill colour (`0xRRGGBB`).
    pub background_rgb: u32,
    /// The list of menu entries to render.
    pub entries: Vec<MenuEntry>,
    /// Footer text.
    pub footer: &'static str,
}

/// A single tappable entry in the main menu.
#[derive(Clone, Debug)]
pub struct MenuEntry {
    /// Primary title.
    pub title: &'static str,
    /// Sub-title / description.
    pub subtitle: &'static str,
    /// Left-border accent colour (`0xRRGGBB`).
    pub accent_rgb: u32,
    /// The view this entry navigates to.
    pub target: ActiveView,
}

impl MenuFrame {
    fn build() -> Self {
        MenuFrame {
            background_rgb: BACKGROUND,
            entries: vec![
                MenuEntry {
                    title: "Animation Playground",
                    subtitle: "Bouncing balls & particle effects",
                    accent_rgb: BLUE,
                    target: ActiveView::AnimationPlayground,
                },
                MenuEntry {
                    title: "Shader Showcase",
                    subtitle: "Dynamic gradients & visual effects",
                    accent_rgb: MAUVE,
                    target: ActiveView::ShaderShowcase,
                },
            ],
            footer: "Powered by GPUI",
        }
    }
}

// ── BackButtonFrame ───────────────────────────────────────────────────────────

/// Visual description of the back button overlay.
///
/// Renderers place this in the top-left corner above the demo content.
#[derive(Clone, Debug)]
pub struct BackButtonFrame {
    /// Button label.
    pub label: &'static str,
    /// Top-left X position in logical pixels.
    pub x: f32,
    /// Top-left Y position in logical pixels.
    pub y: f32,
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
    /// Background fill colour (semi-transparent dark).
    pub background_rgb: u32,
    /// Background alpha.
    pub background_alpha: f32,
    /// Label colour.
    pub text_rgb: u32,
}

impl Default for BackButtonFrame {
    fn default() -> Self {
        BackButtonFrame {
            label: "< Back",
            x: 20.0,
            y: 50.0,
            width: 80.0,
            height: 36.0,
            background_rgb: 0x1a1a1a,
            background_alpha: 0.80,
            text_rgb: TEXT,
        }
    }
}

/// Returns a [`BackButtonFrame`] positioned at the standard top-left location.
///
/// The renderer must call the app's `on_back_button_tap` when a touch falls
/// within the returned rect.
pub fn back_button() -> BackButtonFrame {
    BackButtonFrame::default()
}

// ── DemoView ──────────────────────────────────────────────────────────────────

/// The content of the currently active view, as a render-ready snapshot.
#[derive(Clone, Debug)]
pub enum DemoViewFrame {
    /// The main menu.
    Menu(MenuFrame),
    /// An animation playground frame.
    Animation(AnimFrame, BackButtonFrame),
    /// A shader showcase frame.
    Shader(ShaderFrame, BackButtonFrame),
}

// ── DemoApp ───────────────────────────────────────────────────────────────────

/// Root application view.
///
/// Owns and drives both demos and routes touch events to the active view.
/// Navigation is controlled by calling the `navigate_to_*` / `navigate_to_menu`
/// methods.
pub struct DemoApp {
    active: ActiveView,
    animation: Option<AnimationPlayground>,
    shader: Option<ShaderShowcase>,
    /// Logical screen size `(width, height)`.  Updated via [`set_screen_size`].
    screen_size: (f32, f32),
}

impl DemoApp {
    // ── Construction ──────────────────────────────────────────────────────────

    /// Create a new demo app showing the main menu.
    pub fn new() -> Self {
        DemoApp {
            active: ActiveView::Menu,
            animation: None,
            shader: None,
            screen_size: (390.0, 844.0), // iPhone 14 logical size
        }
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    /// Update the logical screen size.
    ///
    /// Call whenever the Metal layer's drawable size changes.
    pub fn set_screen_size(&mut self, width: f32, height: f32) {
        self.screen_size = (width, height);
        if let Some(ref mut pg) = self.animation {
            pg.set_bounds(width, height);
        }
        if let Some(ref mut ss) = self.shader {
            ss.set_screen_size(width, height);
        }
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    /// Navigate to the Animation Playground demo.
    pub fn navigate_to_animation(&mut self) {
        let mut pg = AnimationPlayground::new();
        let (w, h) = self.screen_size;
        pg.set_bounds(w, h);
        self.animation = Some(pg);
        self.active = ActiveView::AnimationPlayground;
    }

    /// Navigate to the Shader Showcase demo.
    pub fn navigate_to_shader(&mut self) {
        let mut ss = ShaderShowcase::new();
        let (w, h) = self.screen_size;
        ss.set_screen_size(w, h);
        self.shader = Some(ss);
        self.active = ActiveView::ShaderShowcase;
    }

    /// Navigate back to the main menu and tear down the active demo.
    pub fn navigate_to_menu(&mut self) {
        self.active = ActiveView::Menu;
        self.animation = None;
        self.shader = None;
    }

    /// Returns the currently active view variant.
    pub fn active_view(&self) -> ActiveView {
        self.active
    }

    // ── Touch dispatch ────────────────────────────────────────────────────────

    /// Dispatch a touch-down event.
    ///
    /// Returns `true` when the tap hit the back button so the caller can decide
    /// whether to treat the event as consumed.
    pub fn on_touch_down(&mut self, x: f32, y: f32) -> bool {
        if self.active != ActiveView::Menu && self.hit_test_back_button(x, y) {
            self.navigate_to_menu();
            return true;
        }

        match self.active {
            ActiveView::Menu => {
                // Detect which menu entry was tapped.
                if let Some(target) = self.hit_test_menu_entry(x, y) {
                    match target {
                        ActiveView::AnimationPlayground => self.navigate_to_animation(),
                        ActiveView::ShaderShowcase => self.navigate_to_shader(),
                        ActiveView::Menu => {}
                    }
                }
            }
            ActiveView::AnimationPlayground => {
                if let Some(ref mut pg) = self.animation {
                    pg.on_touch_down(AnimPt::new(x, y));
                }
            }
            ActiveView::ShaderShowcase => {
                if let Some(ref mut ss) = self.shader {
                    ss.on_touch_down(ShaderPt::new(x, y));
                }
            }
        }
        false
    }

    /// Dispatch a touch-move event.
    pub fn on_touch_move(&mut self, x: f32, y: f32) {
        match self.active {
            ActiveView::AnimationPlayground => {
                if let Some(ref mut pg) = self.animation {
                    pg.on_touch_move(AnimPt::new(x, y));
                }
            }
            ActiveView::ShaderShowcase => {
                if let Some(ref mut ss) = self.shader {
                    ss.on_touch_move(ShaderPt::new(x, y));
                }
            }
            ActiveView::Menu => {}
        }
    }

    /// Dispatch a touch-up event.
    pub fn on_touch_up(&mut self, x: f32, y: f32) {
        match self.active {
            ActiveView::AnimationPlayground => {
                if let Some(ref mut pg) = self.animation {
                    pg.on_touch_up(AnimPt::new(x, y));
                }
            }
            ActiveView::ShaderShowcase => {
                if let Some(ref mut ss) = self.shader {
                    ss.on_touch_up();
                }
            }
            ActiveView::Menu => {}
        }
    }

    // ── Physics tick (animation playground) ──────────────────────────────────

    /// Advance the animation playground physics by `dt` seconds.
    ///
    /// Pass `None` to have the playground compute its own `dt` from wall time.
    /// No-op when the animation playground is not active.
    pub fn tick(&mut self, dt: Option<f32>) {
        if let Some(ref mut pg) = self.animation {
            pg.tick(dt);
        }
    }

    // ── Frame rendering ───────────────────────────────────────────────────────

    /// Produce a [`DemoViewFrame`] snapshot for the current moment.
    ///
    /// For the animation playground, call [`tick`](Self::tick) before this.
    pub fn render_frame(&mut self) -> DemoViewFrame {
        match self.active {
            ActiveView::Menu => DemoViewFrame::Menu(MenuFrame::build()),

            ActiveView::AnimationPlayground => {
                let snap = self
                    .animation
                    .as_ref()
                    .map(|pg| pg.render_frame())
                    .unwrap_or_else(|| {
                        // Fallback: empty frame (shouldn't happen in normal use).
                        AnimationPlayground::new().render_frame()
                    });
                DemoViewFrame::Animation(snap, back_button())
            }

            ActiveView::ShaderShowcase => {
                let frame = self
                    .shader
                    .as_mut()
                    .map(|ss| ss.render_frame())
                    .unwrap_or_else(|| ShaderShowcase::new().render_frame());
                DemoViewFrame::Shader(frame, back_button())
            }
        }
    }

    // ── Hit testing ───────────────────────────────────────────────────────────

    /// Returns `true` if `(x, y)` falls within the back-button rect.
    fn hit_test_back_button(&self, x: f32, y: f32) -> bool {
        let btn = BackButtonFrame::default();
        x >= btn.x && x <= btn.x + btn.width && y >= btn.y && y <= btn.y + btn.height
    }

    /// Returns the [`ActiveView`] target if `(x, y)` hits a menu entry button,
    /// or `None` if no entry was hit.
    ///
    /// Menu entries are stacked vertically starting at `y = 350.0` with
    /// `120px` height each and `16px` gap, centred in a `300px`-wide column.
    fn hit_test_menu_entry(&self, x: f32, y: f32) -> Option<ActiveView> {
        let (screen_w, _) = self.screen_size;
        let col_w: f32 = 300.0;
        let col_x = (screen_w - col_w) * 0.5;
        let entry_h: f32 = 90.0;
        let gap: f32 = 16.0;
        let start_y: f32 = 350.0;

        let entries = [ActiveView::AnimationPlayground, ActiveView::ShaderShowcase];

        for (i, &target) in entries.iter().enumerate() {
            let ey = start_y + i as f32 * (entry_h + gap);
            if x >= col_x && x <= col_x + col_w && y >= ey && y <= ey + entry_h {
                return Some(target);
            }
        }

        None
    }
}

impl Default for DemoApp {
    fn default() -> Self {
        Self::new()
    }
}

// ── MenuLayout (helper for renderers) ────────────────────────────────────────

/// Pre-computed layout metrics for the main menu.
///
/// Renderers can use this to draw the menu without reimplementing the layout
/// logic.
#[derive(Clone, Debug)]
pub struct MenuLayout {
    /// Screen width in logical pixels.
    pub screen_width: f32,
    /// Screen height in logical pixels.
    pub screen_height: f32,
    /// X position of the column left edge.
    pub col_x: f32,
    /// Width of the entry column.
    pub col_width: f32,
    /// Y position of the title.
    pub title_y: f32,
    /// Y positions and heights of each entry button.
    pub entry_rects: Vec<(f32, f32, f32, f32)>, // (x, y, w, h)
    /// Y position of the footer.
    pub footer_y: f32,
}

impl MenuLayout {
    /// Compute the layout for the given screen size.
    pub fn compute(screen_width: f32, screen_height: f32) -> Self {
        let col_w: f32 = 300.0;
        let col_x = (screen_width - col_w) * 0.5;
        let entry_h: f32 = 90.0;
        let gap: f32 = 16.0;
        let start_y: f32 = 350.0;

        let n = 2usize; // number of menu entries
        let entry_rects: Vec<(f32, f32, f32, f32)> = (0..n)
            .map(|i| {
                let ey = start_y + i as f32 * (entry_h + gap);
                (col_x, ey, col_w, entry_h)
            })
            .collect();

        let last_entry_bottom = start_y + n as f32 * (entry_h + gap);

        MenuLayout {
            screen_width,
            screen_height,
            col_x,
            col_width: col_w,
            title_y: screen_height * 0.35,
            entry_rects,
            footer_y: last_entry_bottom + 48.0,
        }
    }
}

// ── Colour palette re-exports for renderers ───────────────────────────────────

/// Colour palette constants re-exported so renderers don't need to import
/// the parent `demos` module directly.
pub mod palette {
    pub use super::super::{BACKGROUND, BLUE, MAUVE, OVERLAY, SUBTEXT, SURFACE, TEXT};
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── DemoApp construction ──────────────────────────────────────────────────

    #[test]
    fn new_app_shows_menu() {
        let app = DemoApp::new();
        assert_eq!(app.active_view(), ActiveView::Menu);
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    #[test]
    fn navigate_to_animation_changes_active_view() {
        let mut app = DemoApp::new();
        app.navigate_to_animation();
        assert_eq!(app.active_view(), ActiveView::AnimationPlayground);
    }

    #[test]
    fn navigate_to_shader_changes_active_view() {
        let mut app = DemoApp::new();
        app.navigate_to_shader();
        assert_eq!(app.active_view(), ActiveView::ShaderShowcase);
    }

    #[test]
    fn navigate_to_menu_returns_to_menu() {
        let mut app = DemoApp::new();
        app.navigate_to_animation();
        app.navigate_to_menu();
        assert_eq!(app.active_view(), ActiveView::Menu);
    }

    #[test]
    fn navigate_to_menu_drops_demos() {
        let mut app = DemoApp::new();
        app.navigate_to_animation();
        app.navigate_to_menu();
        assert!(app.animation.is_none());
        assert!(app.shader.is_none());
    }

    #[test]
    fn navigate_to_animation_creates_playground() {
        let mut app = DemoApp::new();
        app.navigate_to_animation();
        assert!(app.animation.is_some());
    }

    #[test]
    fn navigate_to_shader_creates_showcase() {
        let mut app = DemoApp::new();
        app.navigate_to_shader();
        assert!(app.shader.is_some());
    }

    // ── set_screen_size ───────────────────────────────────────────────────────

    #[test]
    fn set_screen_size_propagates_to_active_playground() {
        let mut app = DemoApp::new();
        app.navigate_to_animation();
        app.set_screen_size(1170.0, 2532.0);
        // If the playground received the update its bounds width should match.
        let pg = app.animation.as_ref().unwrap();
        assert_eq!(pg.bounds.width, 1170.0);
        assert_eq!(pg.bounds.height, 2532.0);
    }

    // ── Touch dispatch — menu navigation via tap ──────────────────────────────

    #[test]
    fn tap_first_menu_entry_navigates_to_animation() {
        let mut app = DemoApp::new();
        // The first entry starts at y=350; tap in the middle of it.
        let (screen_w, _) = app.screen_size;
        let x = screen_w / 2.0;
        let y = 395.0; // 350 + 45 (middle of 90px button)
        app.on_touch_down(x, y);
        assert_eq!(app.active_view(), ActiveView::AnimationPlayground);
    }

    #[test]
    fn tap_second_menu_entry_navigates_to_shader() {
        let mut app = DemoApp::new();
        let (screen_w, _) = app.screen_size;
        let x = screen_w / 2.0;
        // Second entry: 350 + (90 + 16) + 45 = 501
        let y = 501.0;
        app.on_touch_down(x, y);
        assert_eq!(app.active_view(), ActiveView::ShaderShowcase);
    }

    #[test]
    fn tap_outside_entries_does_not_navigate() {
        let mut app = DemoApp::new();
        // Tap far outside any entry.
        app.on_touch_down(5.0, 5.0);
        assert_eq!(app.active_view(), ActiveView::Menu);
    }

    // ── Touch dispatch — back button ──────────────────────────────────────────

    #[test]
    fn back_button_tap_returns_to_menu_from_animation() {
        let mut app = DemoApp::new();
        app.navigate_to_animation();
        // Back button is at x=20, y=50, 80×36.
        let consumed = app.on_touch_down(40.0, 60.0);
        assert!(consumed, "back button tap should be consumed");
        assert_eq!(app.active_view(), ActiveView::Menu);
    }

    #[test]
    fn back_button_tap_returns_to_menu_from_shader() {
        let mut app = DemoApp::new();
        app.navigate_to_shader();
        let consumed = app.on_touch_down(40.0, 60.0);
        assert!(consumed);
        assert_eq!(app.active_view(), ActiveView::Menu);
    }

    #[test]
    fn back_button_not_active_on_menu() {
        let mut app = DemoApp::new();
        // On the menu, tapping the back-button area should NOT navigate.
        let consumed = app.on_touch_down(40.0, 60.0);
        assert!(!consumed);
        assert_eq!(app.active_view(), ActiveView::Menu);
    }

    // ── render_frame ─────────────────────────────────────────────────────────

    #[test]
    fn render_frame_on_menu_returns_menu_variant() {
        let mut app = DemoApp::new();
        let frame = app.render_frame();
        assert!(
            matches!(frame, DemoViewFrame::Menu(_)),
            "expected Menu variant"
        );
    }

    #[test]
    fn render_frame_on_animation_returns_animation_variant() {
        let mut app = DemoApp::new();
        app.navigate_to_animation();
        let frame = app.render_frame();
        assert!(
            matches!(frame, DemoViewFrame::Animation(_, _)),
            "expected Animation variant"
        );
    }

    #[test]
    fn render_frame_on_shader_returns_shader_variant() {
        let mut app = DemoApp::new();
        app.navigate_to_shader();
        let frame = app.render_frame();
        assert!(
            matches!(frame, DemoViewFrame::Shader(_, _)),
            "expected Shader variant"
        );
    }

    #[test]
    fn render_frame_menu_has_two_entries() {
        let mut app = DemoApp::new();
        let frame = app.render_frame();
        if let DemoViewFrame::Menu(menu) = frame {
            assert_eq!(menu.entries.len(), 2);
        } else {
            panic!("expected Menu frame");
        }
    }

    // ── MenuFrame ─────────────────────────────────────────────────────────────

    #[test]
    fn menu_frame_entries_have_correct_targets() {
        let frame = MenuFrame::build();
        assert_eq!(frame.entries[0].target, ActiveView::AnimationPlayground);
        assert_eq!(frame.entries[1].target, ActiveView::ShaderShowcase);
    }

    #[test]
    fn menu_frame_background_matches_palette() {
        let frame = MenuFrame::build();
        assert_eq!(frame.background_rgb, BACKGROUND);
    }

    // ── back_button helper ────────────────────────────────────────────────────

    #[test]
    fn back_button_label_is_nonempty() {
        let btn = back_button();
        assert!(!btn.label.is_empty());
    }

    #[test]
    fn back_button_has_positive_dimensions() {
        let btn = back_button();
        assert!(btn.width > 0.0);
        assert!(btn.height > 0.0);
    }

    // ── MenuLayout ────────────────────────────────────────────────────────────

    #[test]
    fn menu_layout_column_is_centred() {
        let layout = MenuLayout::compute(390.0, 844.0);
        let expected_col_x = (390.0 - 300.0) / 2.0;
        assert!(
            (layout.col_x - expected_col_x).abs() < 1e-3,
            "col_x = {}",
            layout.col_x
        );
    }

    #[test]
    fn menu_layout_has_two_entry_rects() {
        let layout = MenuLayout::compute(390.0, 844.0);
        assert_eq!(layout.entry_rects.len(), 2);
    }

    #[test]
    fn menu_layout_entries_do_not_overlap() {
        let layout = MenuLayout::compute(390.0, 844.0);
        let (_, y0, _, h0) = layout.entry_rects[0];
        let (_, y1, _, _) = layout.entry_rects[1];
        assert!(y1 > y0 + h0, "entries should not overlap");
    }

    #[test]
    fn menu_layout_footer_below_entries() {
        let layout = MenuLayout::compute(390.0, 844.0);
        let (_, last_y, _, last_h) = *layout.entry_rects.last().unwrap();
        assert!(
            layout.footer_y > last_y + last_h,
            "footer should be below the last entry"
        );
    }

    // ── tick ──────────────────────────────────────────────────────────────────

    #[test]
    fn tick_does_not_panic_on_menu() {
        let mut app = DemoApp::new();
        app.tick(Some(0.016)); // Should be a no-op on the menu.
    }

    #[test]
    fn tick_advances_animation_playground() {
        let mut app = DemoApp::new();
        app.navigate_to_animation();
        // Spawn a ball so there's something to advance.
        if let Some(ref mut pg) = app.animation {
            pg.spawn_ball(AnimPt::new(200.0, 100.0), AnimPt::new(0.0, 0.0));
        }
        let y_before = app.animation.as_ref().unwrap().render_frame().balls[0]
            .pos
            .y;
        app.tick(Some(0.1));
        let y_after = app.animation.as_ref().unwrap().render_frame().balls[0]
            .pos
            .y;
        assert!(
            y_after > y_before,
            "ball should fall under gravity after tick"
        );
    }

    // ── ActiveView default ────────────────────────────────────────────────────

    #[test]
    fn active_view_default_is_menu() {
        assert_eq!(ActiveView::default(), ActiveView::Menu);
    }
}
