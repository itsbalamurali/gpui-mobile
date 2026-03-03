//! Navigation router for the cross-platform GPUI example app.
//!
//! This module defines the available screens, a shared navigation model,
//! and a top-level `Router` view that renders the currently active screen.
//!
//! ## Screens
//!
//! - **Home** — welcome message, colour swatches, stats, and quick-nav cards.
//! - **Counter** — increment / decrement / reset a shared tap counter.
//! - **Settings** — toggle dark mode, reset counter, change user name.
//! - **About** — app info, technology stack, architecture, and credits.
//! - **Animations** — bouncing balls with physics, trails, and particle effects.
//! - **Shaders** — dynamic gradients, floating orbs, and ripple effects.

pub mod about;
pub mod components;
pub mod counter;
pub mod home;
pub mod settings;

use crate::demos::{AnimationPlayground, ShaderShowcase};
use gpui::{
    div, hsla, point, prelude::*, px, rgb, size, Bounds, Context, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, SharedString, Window,
};

// ── Screen enum ──────────────────────────────────────────────────────────────

/// All navigable screens in the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Home,
    Counter,
    Settings,
    About,
    Components,
    Animations,
    Shaders,
}

impl Screen {
    /// Human-readable title for the screen (used in the nav bar).
    pub fn title(&self) -> &'static str {
        match self {
            Screen::Home => "Home",
            Screen::Counter => "Counter",
            Screen::Settings => "Settings",
            Screen::About => "About",
            Screen::Components => "Components",
            Screen::Animations => "Animations",
            Screen::Shaders => "Shaders",
        }
    }
}

// ── Colour palette (Catppuccin Mocha) ────────────────────────────────────────

pub const BASE: u32 = 0x1e1e2e;
pub const SURFACE0: u32 = 0x313244;
pub const SURFACE1: u32 = 0x45475a;
pub const TEXT: u32 = 0xcdd6f4;
pub const SUBTEXT: u32 = 0xa6adc8;
pub const BLUE: u32 = 0x89b4fa;
pub const GREEN: u32 = 0xa6e3a1;
pub const RED: u32 = 0xf38ba8;
pub const MAUVE: u32 = 0xcba6f7;
pub const YELLOW: u32 = 0xf9e2af;
pub const PEACH: u32 = 0xfab387;
pub const TEAL: u32 = 0x94e2d5;
pub const MANTLE: u32 = 0x181825;
pub const SKY: u32 = 0x89dceb;
pub const LAVENDER: u32 = 0xb4befe;

// ── Safe area ────────────────────────────────────────────────────────────────

/// Safe area insets in logical pixels.
///
/// These represent the areas occupied by system UI (status bar, navigation
/// bar, camera notch) that the app content should pad around.
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct SafeArea {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

// ── Router ───────────────────────────────────────────────────────────────────

/// Top-level view that owns navigation state and delegates rendering to the
/// active screen.
pub struct Router {
    pub current_screen: Screen,
    /// Shared state: a global tap counter (carried across screens for demo).
    pub tap_count: u32,
    /// User name shown on the home screen.
    pub user_name: SharedString,
    /// A flag toggled in Settings.
    pub dark_mode: bool,
    /// Navigation history stack for back navigation.
    history: Vec<Screen>,
    /// Safe area insets (logical pixels) to pad around system chrome.
    pub safe_area: SafeArea,

    // ── Demo view state ──────────────────────────────────────────────────
    /// The animation playground demo (lazily created when the screen is visited).
    animation_playground: Option<AnimationPlayground>,
    /// The shader showcase demo (lazily created when the screen is visited).
    shader_showcase: Option<ShaderShowcase>,
}

impl Router {
    pub fn new() -> Self {
        let safe_area = Self::query_safe_area();

        let user_name = if cfg!(target_os = "ios") {
            "iOS"
        } else if cfg!(target_os = "android") {
            "Android"
        } else {
            "Mobile"
        };

        Self {
            current_screen: Screen::Home,
            tap_count: 0,
            user_name: user_name.into(),
            dark_mode: true,
            history: Vec::new(),
            safe_area,
            animation_playground: None,
            shader_showcase: None,
        }
    }

    /// Query the safe area insets from the platform.
    ///
    /// On Android, reads insets from the global `AndroidPlatform` via
    /// `jni_entry`.  On iOS, safe area insets are managed by UIKit and
    /// will be queried once the iOS platform integration exposes them.
    ///
    /// Returns logical-pixel insets if available, otherwise zeros (no padding).
    fn query_safe_area() -> SafeArea {
        #[cfg(target_os = "android")]
        {
            use gpui_mobile::android::jni_entry;
            if let Some(platform) = jni_entry::platform() {
                if let Some(win) = platform.primary_window() {
                    let insets = win.safe_area_insets_logical();
                    log::info!(
                        "Router: safe area insets (logical px): top={:.1} bottom={:.1} left={:.1} right={:.1}",
                        insets.top, insets.bottom, insets.left, insets.right,
                    );
                    return SafeArea {
                        top: insets.top,
                        bottom: insets.bottom,
                        left: insets.left,
                        right: insets.right,
                    };
                }
            }
        }

        #[cfg(target_os = "ios")]
        {
            // TODO: Query safe area insets from the iOS platform once
            // IosWindow exposes them (status bar, home indicator, notch).
            // For now, provide sensible defaults for modern iPhones.
            return SafeArea {
                top: 59.0,    // status bar + notch
                bottom: 34.0, // home indicator
                left: 0.0,
                right: 0.0,
            };
        }

        #[allow(unreachable_code)]
        SafeArea::default()
    }

    /// Navigate to a new screen, pushing the current one onto the history stack.
    pub fn navigate_to(&mut self, screen: Screen) {
        if self.current_screen != screen {
            self.history.push(self.current_screen);
            self.current_screen = screen;

            // Lazily initialise demo state when first visited.
            match screen {
                Screen::Animations if self.animation_playground.is_none() => {
                    self.animation_playground = Some(AnimationPlayground::new());
                }
                Screen::Shaders if self.shader_showcase.is_none() => {
                    self.shader_showcase = Some(ShaderShowcase::new());
                }
                _ => {}
            }
        }
    }

    /// Go back to the previous screen. Returns `true` if navigation occurred.
    pub fn go_back(&mut self) -> bool {
        if let Some(prev) = self.history.pop() {
            self.current_screen = prev;
            true
        } else {
            false
        }
    }

    /// Whether the back button should be shown.
    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }
}

// ── Render ───────────────────────────────────────────────────────────────────

impl Render for Router {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // For the fullscreen demo screens (Animations, Shaders) we skip the
        // chrome (nav bar, tab bar, safe area spacers) and render edge-to-edge.
        match self.current_screen {
            Screen::Animations => {
                return self.render_animations_screen(window, cx).into_any_element();
            }
            Screen::Shaders => {
                return self.render_shaders_screen(window, cx).into_any_element();
            }
            _ => {}
        }

        let bg_color = if self.dark_mode { BASE } else { 0xeff1f5 };
        let text_color = if self.dark_mode { TEXT } else { 0x4c4f69 };
        let safe_top = self.safe_area.top;
        let safe_bottom = self.safe_area.bottom;

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(bg_color))
            .text_color(rgb(text_color))
            // ── Top safe-area spacer (status bar / notch) ────────────────
            .when(safe_top > 0.0, |d| {
                d.child(div().w_full().h(px(safe_top)).bg(rgb(if self.dark_mode {
                    MANTLE
                } else {
                    0xdce0e8
                })))
            })
            // ── Top navigation bar ───────────────────────────────────────
            .child(self.render_nav_bar(cx))
            // ── Screen content ───────────────────────────────────────────
            .child(self.render_current_screen(cx))
            // ── Bottom tab bar ───────────────────────────────────────────
            .child(self.render_tab_bar(cx))
            // ── Bottom safe-area spacer (nav bar / gesture indicator) ────
            .when(safe_bottom > 0.0, |d| {
                d.child(div().w_full().h(px(safe_bottom)).bg(rgb(if self.dark_mode {
                    MANTLE
                } else {
                    0xdce0e8
                })))
            })
            .into_any_element()
    }
}

impl Router {
    /// Render the top navigation bar with title and optional back button.
    fn render_nav_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let can_go_back = self.can_go_back();
        let title = self.current_screen.title();
        let bar_bg = if self.dark_mode { MANTLE } else { 0xdce0e8 };
        let text_col = if self.dark_mode { TEXT } else { 0x4c4f69 };

        div()
            .flex()
            .flex_row()
            .w_full()
            .px_4()
            .py_3()
            .bg(rgb(bar_bg))
            .items_center()
            .child(if can_go_back {
                div()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(BLUE))
                    .text_color(rgb(MANTLE))
                    .text_sm()
                    .child("← Back")
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.go_back();
                            cx.notify();
                        }),
                    )
            } else {
                // Invisible spacer to keep the title centred.
                div().px_3().py_1().child("      ")
            })
            .child(
                div()
                    .flex_1()
                    .text_center()
                    .text_lg()
                    .text_color(rgb(text_col))
                    .child(title),
            )
            // Right spacer to balance the back button.
            .child(div().px_3().py_1().child("      "))
    }

    /// Render the content area for the currently active screen.
    ///
    /// The content is wrapped in a scrollable container so that screens
    /// with more content than fits on screen (e.g. About) can be scrolled
    /// vertically via touch drag gestures.
    fn render_current_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let screen_content = match self.current_screen {
            Screen::Home => self.render_home_screen(cx).into_any_element(),
            Screen::Counter => self.render_counter_screen(cx).into_any_element(),
            Screen::Settings => self.render_settings_screen(cx).into_any_element(),
            Screen::About => self.render_about_screen(cx).into_any_element(),
            Screen::Components => self.render_components_screen(cx).into_any_element(),
            // Animations and Shaders are rendered fullscreen (handled above
            // in Render::render) and should never reach here.
            Screen::Animations | Screen::Shaders => div().into_any_element(),
        };

        div()
            .id("screen-scroll-container")
            .flex_1()
            .overflow_y_scroll()
            .child(screen_content)
    }

    /// Render the bottom tab bar.
    fn render_tab_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current_screen;
        let bar_bg = if self.dark_mode { MANTLE } else { 0xdce0e8 };

        let tabs: &[(&str, &str, Screen)] = &[
            ("[H]", "Home", Screen::Home),
            ("[#]", "Counter", Screen::Counter),
            ("[▶]", "Anims", Screen::Animations),
            ("[◆]", "UI Kit", Screen::Components),
            ("[S]", "Settings", Screen::Settings),
            ("[i]", "About", Screen::About),
        ];

        let mut bar = div()
            .flex()
            .flex_row()
            .w_full()
            .py_2()
            .bg(rgb(bar_bg))
            .justify_around()
            .items_center();

        for &(icon, label, screen) in tabs {
            let is_active = current == screen;
            let label_color = if is_active { BLUE } else { SUBTEXT };

            bar = bar.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_lg()
                    .when(is_active, |d| d.bg(rgb(SURFACE0)))
                    .child(div().text_lg().text_color(rgb(label_color)).child(icon))
                    .child(div().text_xs().text_color(rgb(label_color)).child(label))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.navigate_to(screen);
                            cx.notify();
                        }),
                    ),
            );
        }

        bar
    }

    // ── Per-screen render helpers ────────────────────────────────────────────

    fn render_home_screen(&self, cx: &mut Context<Self>) -> impl IntoElement {
        home::render(self, cx)
    }

    fn render_counter_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        counter::render(self, cx)
    }

    fn render_settings_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        settings::render(self, cx)
    }

    fn render_about_screen(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        about::render(self)
    }

    fn render_components_screen(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        components::render(self)
    }

    // ── Fullscreen demo screens ──────────────────────────────────────────────

    /// Render the Animations screen — fullscreen, edge-to-edge.
    fn render_animations_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Request continuous animation frames so physics keeps ticking.
        window.request_animation_frame();

        // Ensure the playground exists.
        if self.animation_playground.is_none() {
            self.animation_playground = Some(AnimationPlayground::new());
        }

        // Update bounds from the current viewport.
        let viewport = window.viewport_size();
        if let Some(playground) = &mut self.animation_playground {
            playground.set_bounds(Bounds {
                origin: point(0.0, 0.0),
                size: size(viewport.width.as_f32(), viewport.height.as_f32()),
            });
        }

        div()
            .size_full()
            .bg(rgb(BASE))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    if let Some(playground) = &mut this.animation_playground {
                        let pos = point(event.position.x.as_f32(), event.position.y.as_f32());
                        playground.touch_start = Some((pos, std::time::Instant::now()));
                        playground.current_touch = Some(pos);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                    if let Some(playground) = &mut this.animation_playground {
                        let position = point(event.position.x.as_f32(), event.position.y.as_f32());

                        if let Some((start_pos, start_time)) = playground.touch_start.take() {
                            let elapsed = start_time.elapsed();
                            let dx = position.x - start_pos.x;
                            let dy = position.y - start_pos.y;
                            let distance = (dx * dx + dy * dy).sqrt();

                            if elapsed < std::time::Duration::from_millis(200) && distance < 20.0 {
                                // Short tap → spawn particles
                                let color_rgb = crate::demos::random_color(playground.next_ball_id);
                                playground.spawn_particles(position, rgb(color_rgb).into());
                                playground.next_ball_id += 1;
                            } else {
                                // Swipe → fling a ball
                                let dt = elapsed.as_secs_f32().max(0.01);
                                let velocity = point(dx / dt * 0.5, dy / dt * 0.5);
                                playground.spawn_ball(start_pos, velocity);
                            }
                        }
                        playground.current_touch = None;
                        cx.notify();
                    }
                }),
            )
            // Render the playground content with a single back button
            .child(if let Some(playground) = &mut self.animation_playground {
                playground
                    .render_with_back_button(
                        window,
                        cx.listener(|this, _, _window, cx| {
                            this.go_back();
                            cx.notify();
                        }),
                    )
                    .into_any_element()
            } else {
                div().into_any_element()
            })
    }

    /// Render the Shaders screen — fullscreen, edge-to-edge.
    fn render_shaders_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Request continuous animation frames for the shader loop.
        window.request_animation_frame();

        // Ensure the showcase exists.
        if self.shader_showcase.is_none() {
            self.shader_showcase = Some(ShaderShowcase::new());
        }

        // Update screen center for parallax calculations.
        if let Some(showcase) = &mut self.shader_showcase {
            let viewport = window.viewport_size();
            showcase.set_screen_center(point(
                viewport.width.as_f32() / 2.0,
                viewport.height.as_f32() / 2.0,
            ));
        }

        div()
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    if let Some(showcase) = &mut this.shader_showcase {
                        let pos = point(event.position.x.as_f32(), event.position.y.as_f32());
                        showcase.touch_position = Some(pos);
                        showcase.spawn_ripple(pos);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if let Some(showcase) = &mut this.shader_showcase {
                    let pos = point(event.position.x.as_f32(), event.position.y.as_f32());
                    showcase.touch_position = Some(pos);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if let Some(showcase) = &mut this.shader_showcase {
                        showcase.touch_position = None;
                        cx.notify();
                    }
                }),
            )
            // Render the showcase content with a single back button
            .child(if let Some(showcase) = &mut self.shader_showcase {
                showcase
                    .render_with_back_button(
                        window,
                        cx.listener(|this, _, _window, cx| {
                            this.go_back();
                            cx.notify();
                        }),
                    )
                    .into_any_element()
            } else {
                div().into_any_element()
            })
    }
}

// ── Back button (shared with demo screens) ───────────────────────────────────

/// Floating back button overlaid on fullscreen demo screens.
fn back_button<F>(on_click: F) -> impl IntoElement
where
    F: Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
{
    div()
        .absolute()
        .top(px(54.0))
        .left(px(16.0))
        .px_4()
        .py_2()
        .bg(hsla(0.0, 0.0, 0.2, 0.8))
        .rounded_lg()
        .text_color(rgb(TEXT))
        .text_sm()
        .child("← Back")
        .on_mouse_down(MouseButton::Left, on_click)
}
