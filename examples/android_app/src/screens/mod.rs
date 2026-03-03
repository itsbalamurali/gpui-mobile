//! Navigation router for the Android example app.
//!
//! This module defines the available screens, a shared navigation model,
//! and a top-level `Router` view that renders the currently active screen.

pub mod about;
pub mod counter;
pub mod home;
pub mod settings;

use gpui::{div, prelude::*, px, rgb, Context, SharedString, Window};

// ── Screen enum ──────────────────────────────────────────────────────────────

/// All navigable screens in the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Home,
    Counter,
    Settings,
    About,
}

impl Screen {
    /// Human-readable title for the screen (used in the nav bar).
    pub fn title(&self) -> &'static str {
        match self {
            Screen::Home => "Home",
            Screen::Counter => "Counter",
            Screen::Settings => "Settings",
            Screen::About => "About",
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

// ── Router ───────────────────────────────────────────────────────────────────

/// Top-level view that owns navigation state and delegates rendering to the
/// active screen.
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
}

impl Router {
    pub fn new() -> Self {
        // Query safe area insets from the Android platform if available.
        let safe_area = Self::query_safe_area();

        Self {
            current_screen: Screen::Home,
            tap_count: 0,
            user_name: "Android".into(),
            dark_mode: true,
            history: Vec::new(),
            safe_area,
        }
    }

    /// Query the safe area insets from the global AndroidPlatform.
    ///
    /// Returns logical-pixel insets if the platform and primary window are
    /// available, otherwise returns zeros (no padding).
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
        SafeArea::default()
    }

    /// Navigate to a new screen, pushing the current one onto the history stack.
    pub fn navigate_to(&mut self, screen: Screen) {
        if self.current_screen != screen {
            self.history.push(self.current_screen);
            self.current_screen = screen;
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

impl Render for Router {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
    fn render_current_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        match self.current_screen {
            Screen::Home => self.render_home_screen(cx).into_any_element(),
            Screen::Counter => self.render_counter_screen(cx).into_any_element(),
            Screen::Settings => self.render_settings_screen(cx).into_any_element(),
            Screen::About => self.render_about_screen(cx).into_any_element(),
        }
    }

    /// Render the bottom tab bar.
    fn render_tab_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current_screen;
        let bar_bg = if self.dark_mode { MANTLE } else { 0xdce0e8 };

        let tabs = [
            ("[H]", "Home", Screen::Home),
            ("[#]", "Counter", Screen::Counter),
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

        for (icon, label, screen) in tabs {
            let is_active = current == screen;
            let label_color = if is_active { BLUE } else { SUBTEXT };

            bar = bar.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .px_4()
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
    //
    // Each of these delegates to the corresponding screen module's `render`
    // function, passing any shared state the screen needs.

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
}
