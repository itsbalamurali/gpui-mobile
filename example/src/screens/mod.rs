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
pub mod chat;
pub mod components;
pub mod counter;
pub mod feed;
pub mod form;
pub mod home;
pub mod packages_demo;
pub mod settings;
pub mod swiper;
pub mod webview_browser;

use crate::demos::{AnimationPlayground, ShaderShowcase};
use gpui::{
    div, point, prelude::*, px, rgb, size, Bounds, Context, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, SharedString, Window,
};
use gpui_mobile::components::material::{MaterialTheme, NavigationBarBuilder, TextField, TopAppBar};
use gpui_mobile::{set_system_chrome, StatusBarContentStyle, SystemChromeStyle};

// ── Screen enum ──────────────────────────────────────────────────────────────

/// All navigable screens in the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Home,
    Counter,
    Settings,
    About,
    AppleGlass,
    Material,
    Form,
    Animations,
    Shaders,
    PackagesDemo,
    WebViewBrowser,
    Swiper,
    Feed,
    Chat,
}

impl Screen {
    /// Human-readable title for the screen (used in the nav bar).
    pub fn title(&self) -> &'static str {
        match self {
            Screen::Home => "Home",
            Screen::Counter => "Counter",
            Screen::Settings => "Settings",
            Screen::About => "About",
            Screen::AppleGlass => "Apple Liquid Glass",
            Screen::Material => "Material Design 3",
            Screen::Form => "Material Form",
            Screen::Animations => "Animations",
            Screen::Shaders => "Shaders",
            Screen::PackagesDemo => "Packages",
            Screen::WebViewBrowser => "Browser",
            Screen::Swiper => "Discover",
            Screen::Feed => "Feed",
            Screen::Chat => "Sarah Johnson",
        }
    }

    /// Whether this screen is a primary tab-bar destination.
    ///
    /// Tab roots are the screens directly reachable from the bottom
    /// navigation bar. Navigating between them clears the history
    /// stack so the back button is never shown on these screens.
    pub fn is_tab_root(&self) -> bool {
        matches!(
            self,
            Screen::Home | Screen::Counter | Screen::Settings | Screen::About
        )
    }
}

// ── Colour palette (Google Material) ─────────────────────────────────────────

pub const BASE: u32 = 0x121318;        // Dark surface
pub const SURFACE0: u32 = 0x1E1F25;   // Dark surface container
pub const SURFACE1: u32 = 0x282A2F;   // Dark surface container high
pub const TEXT: u32 = 0xE2E2E9;       // Dark on-surface
pub const SUBTEXT: u32 = 0xC4C6D0;    // Dark on-surface-variant
pub const BLUE: u32 = 0x4285F4;       // Google Blue
pub const GREEN: u32 = 0x34A853;      // Google Green
pub const RED: u32 = 0xEA4335;        // Google Red
pub const MAUVE: u32 = 0xA142F4;      // Google Purple
pub const YELLOW: u32 = 0xFBBC04;     // Google Yellow
pub const PEACH: u32 = 0xFA7B17;      // Google Orange
pub const TEAL: u32 = 0x24C1E0;       // Google Teal
pub const MANTLE: u32 = 0x0D0E13;     // Dark surface container lowest
pub const SKY: u32 = 0x4FC3F7;        // Light Blue
pub const LAVENDER: u32 = 0x7B8CF8;   // Indigo light

// Light mode equivalents (used inline in screen render functions).
pub const LIGHT_TEXT: u32 = 0x1A1B20;
pub const LIGHT_SUBTEXT: u32 = 0x44474F;
pub const LIGHT_CARD_BG: u32 = 0xEDEDF4;
pub const LIGHT_DIVIDER: u32 = 0xC4C6D0;

// ── Safe area ────────────────────────────────────────────────────────────────

/// Safe area insets in logical pixels.
///
/// These represent the areas occupied by system UI (status bar, navigation
/// bar, camera notch) that the app content should pad around.
#[derive(Debug, Clone, Copy, Default)]
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

    // ── Form demo state ─────────────────────────────────────────────────
    pub form: FormState,

    // ── Pull-to-refresh state ────────────────────────────────────────
    /// Y coordinate where the pull gesture started (None if not pulling).
    pub pull_start_y: Option<f32>,
    /// Current pull distance in pixels.
    pub pull_distance: f32,
    /// Whether the refresh is currently active (showing spinner).
    pub refreshing: bool,

    // ── WebView browser state ────────────────────────────────────────
    /// The URL to load in the in-app browser.
    pub webview_url: String,
    /// Active WebView handle (if open).
    pub webview_handle: Option<usize>,

    // ── Swiper state ─────────────────────────────────────────────────
    /// Index of the current top card in the swiper.
    pub swiper_index: usize,
    /// Horizontal drag offset for the current card (pixels).
    pub swiper_drag_x: f32,
    /// X position where the drag gesture started (for relative tracking).
    pub swiper_drag_start_x: Option<f32>,
    /// Whether a drag gesture is active.
    pub swiper_dragging: bool,
    /// Swipe-out animation: +1.0 = liked (fly right), -1.0 = noped (fly left), 0.0 = none.
    pub swiper_fly_direction: f32,
    /// Monotonic counter used to generate unique animation IDs per swipe.
    pub swiper_anim_id: u32,

    // ── File/Image picker state ─────────────────────────────────────
    /// Last picked file name (for demo display).
    pub last_picked_file: Option<String>,
    /// Last picked image name (for demo display).
    pub last_picked_image: Option<String>,

    // ── Camera state ─────────────────────────────────────────────────
    /// Active camera session handle.
    pub camera_handle: Option<usize>,
    /// Last camera status message (for demo display).
    pub camera_status: Option<String>,
    /// Whether the camera preview is active.
    pub camera_previewing: bool,
    /// Whether video recording is active.
    pub camera_recording: bool,

    // ── Permission handler state ──────────────────────────────────
    /// Last permission status message (for demo display).
    pub perm_status: Option<String>,

    // ── Location state ────────────────────────────────────────────
    /// Last location result (for demo display).
    pub location_status: Option<String>,

    // ── Notifications state ───────────────────────────────────────
    /// Last notification status (for demo display).
    pub notif_status: Option<String>,
    /// Notification counter for unique IDs.
    pub notif_counter: i32,

    // ── Audio state ───────────────────────────────────────────────
    /// Last audio status (for demo display).
    pub audio_status: Option<String>,

    // ── Video player state ────────────────────────────────────────
    /// Last video player status (for demo display).
    pub video_status: Option<String>,

    // ── Feed state ───────────────────────────────────────────────────
    /// Which feed posts are "liked" (by index).
    pub feed_likes: [bool; 6],
    /// Feed pull-to-refresh: Y coordinate where drag started.
    pub feed_pull_start_y: Option<f32>,
    /// Feed pull-to-refresh: current pull distance in px.
    pub feed_pull_distance: f32,
    /// Whether the feed is currently "refreshing".
    pub feed_refreshing: bool,

    // ── Chat state ────────────────────────────────────────────────────
    /// Text currently being composed in the chat input.
    pub chat_compose_text: String,
    /// Messages sent by the user during this session.
    pub chat_sent_messages: Vec<String>,
    /// Whether the chat composer field is focused.
    pub chat_focused: bool,
    /// Which message index has the reaction picker open (None = no picker).
    pub chat_reaction_picker: Option<usize>,
    /// User-added reactions per message index.
    pub chat_user_reactions: Vec<Vec<String>>,
    /// Whether the mic is currently recording.
    pub chat_mic_recording: bool,
    /// Horizontal swipe offset per message (for revealing timestamps).
    /// Positive = swiped left (sent messages), negative = swiped right (received).
    pub chat_swipe_offset: f32,
    /// Message index currently being swiped.
    pub chat_swipe_msg: Option<usize>,
    /// X position where the swipe started.
    pub chat_swipe_start_x: Option<f32>,
}

/// Mutable state backing the Material Form demo screen.
#[derive(Debug, Clone)]
pub struct FormState {
    pub notifications: bool,
    pub auto_update: bool,
    pub account_type: u8, // 0=personal, 1=business, 2=education
    pub interests: [bool; 4], // tech, design, science, music
    pub skill_level: f32,
    pub experience: f32,
    pub terms_accepted: bool,
    pub newsletter: bool,
    // Text input fields (with cursor/selection state)
    pub full_name: TextField,
    pub email: TextField,
    pub phone: TextField,
    /// Which field is currently focused (None = no field focused).
    pub focused_field: Option<u8>, // 0=name, 1=email, 2=phone
}

impl Default for FormState {
    fn default() -> Self {
        Self {
            notifications: true,
            auto_update: true,
            account_type: 0,
            interests: [true, false, true, false],
            skill_level: 0.6,
            experience: 0.3,
            terms_accepted: false,
            newsletter: false,
            full_name: TextField::new("Jane Doe"),
            email: TextField::new("jane@example.com"),
            phone: TextField::new("+1 (555) 123-4567"),
            focused_field: None,
        }
    }
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
            form: FormState::default(),
            pull_start_y: None,
            pull_distance: 0.0,
            refreshing: false,
            webview_url: "https://google.com".into(),
            webview_handle: None,
            swiper_index: 0,
            swiper_drag_x: 0.0,
            swiper_drag_start_x: None,
            swiper_dragging: false,
            swiper_fly_direction: 0.0,
            swiper_anim_id: 0,
            last_picked_file: None,
            last_picked_image: None,
            camera_handle: None,
            camera_status: None,
            camera_previewing: false,
            camera_recording: false,
            perm_status: None,
            location_status: None,
            notif_status: None,
            notif_counter: 0,
            audio_status: None,
            video_status: None,
            feed_likes: [false; 6],
            feed_pull_start_y: None,
            feed_pull_distance: 0.0,
            feed_refreshing: false,
            chat_compose_text: String::new(),
            chat_sent_messages: Vec::new(),
            chat_focused: false,
            chat_reaction_picker: None,
            chat_user_reactions: Vec::new(),
            chat_mic_recording: false,
            chat_swipe_offset: 0.0,
            chat_swipe_msg: None,
            chat_swipe_start_x: None,
        }
    }

    /// Query the safe area insets from the platform.
    ///
    /// On Android, reads insets from the global `AndroidPlatform` via
    /// `jni`.  On iOS, safe area insets are managed by UIKit and
    /// will be queried once the iOS platform integration exposes them.
    ///
    /// Returns logical-pixel insets if available, otherwise zeros (no padding).
    fn query_safe_area() -> SafeArea {
        #[cfg(target_os = "android")]
        {
            use gpui_mobile::android::jni;
            if let Some(platform) = jni::platform() {
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
            let (top, bottom, left, right) = gpui_mobile::safe_area_insets();
            if top > 0.0 || bottom > 0.0 {
                return SafeArea { top, bottom, left, right };
            }
            // Fallback for before the window is ready
            return SafeArea {
                top: 59.0,
                bottom: 34.0,
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
            // Dismiss webview when leaving the browser screen
            if self.current_screen == Screen::WebViewBrowser {
                if let Some(ptr) = self.webview_handle.take() {
                    let handle = gpui_mobile::packages::webview::WebViewHandle { ptr };
                    let _ = gpui_mobile::packages::webview::dismiss(handle);
                }
            }
            // Dismiss keyboard when leaving form or chat screens
            if self.form.focused_field.is_some() {
                self.form.focused_field = None;
                self.form.full_name.selection = None;
                self.form.email.selection = None;
                self.form.phone.selection = None;
                gpui_mobile::hide_keyboard();
                gpui_mobile::set_text_input_callback(None);
            }
            if self.chat_focused {
                self.chat_focused = false;
                gpui_mobile::hide_keyboard();
                gpui_mobile::set_text_input_callback(None);
            }
            if screen.is_tab_root() {
                // Switching to a tab-bar root screen — clear history so
                // the back button is not shown on primary destinations.
                self.history.clear();
            } else {
                self.history.push(self.current_screen);
            }
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
            // Dismiss webview when leaving browser
            if self.current_screen == Screen::WebViewBrowser {
                if let Some(ptr) = self.webview_handle.take() {
                    let handle = gpui_mobile::packages::webview::WebViewHandle { ptr };
                    let _ = gpui_mobile::packages::webview::dismiss(handle);
                }
            }
            // Dismiss keyboard when navigating back
            if self.form.focused_field.is_some() {
                self.form.focused_field = None;
                self.form.full_name.selection = None;
                self.form.email.selection = None;
                self.form.phone.selection = None;
                gpui_mobile::hide_keyboard();
                gpui_mobile::set_text_input_callback(None);
            }
            self.current_screen = prev;
            true
        } else {
            false
        }
    }

    /// Whether the back button should be shown.
    ///
    /// Tab-bar root screens never show a back button, even if there
    /// is history (e.g. the user navigated Home → Counter → Home —
    /// history is cleared on tab switches so this is defensive).
    pub fn can_go_back(&self) -> bool {
        !self.current_screen.is_tab_root() && !self.history.is_empty()
    }
}

// ── Render ───────────────────────────────────────────────────────────────────

impl Render for Router {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        log::info!("Router: render() screen={:?}", self.current_screen);
        let show_tab_bar = self.current_screen.is_tab_root();
        let theme = gpui_mobile::components::material::MaterialTheme::from_appearance(self.dark_mode);
        let bg_color = theme.surface;
        let text_color = theme.on_surface;
        let safe_top = self.safe_area.top;
        let safe_bottom = self.safe_area.bottom;

        // ── Compute system chrome style ──────────────────────────────────
        let chrome = self.system_chrome_style();
        let top_color = chrome.status_bar_color.unwrap_or(bg_color);
        let bottom_color = chrome.navigation_bar_color.unwrap_or(bg_color);

        // Apply to the OS-level status bar / navigation bar.
        set_system_chrome(&chrome);

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(bg_color))
            .text_color(rgb(text_color))
            // ── Top safe-area spacer (status bar / notch) ────────────────
            .when(safe_top > 0.0, |d| {
                d.child(div().w_full().h(px(safe_top)).bg(rgb(top_color)))
            })
            // ── Top navigation bar ───────────────────────────────────────
            .child(self.render_nav_bar(cx))
            // ── Screen content ───────────────────────────────────────────
            .child(self.render_current_screen(window, cx))
            // ── Bottom tab bar (only for tab-root screens) ───────────────
            .when(show_tab_bar, |d| {
                d.child(self.render_tab_bar(cx))
            })
            // ── Bottom safe-area spacer (nav bar / gesture indicator) ────
            .when(safe_bottom > 0.0 && show_tab_bar, |d| {
                d.child(div().w_full().h(px(safe_bottom)).bg(rgb(bottom_color)))
            })
            .into_any_element()
    }
}

impl Router {
    /// Compute the system chrome style for the current screen and theme.
    ///
    /// Default: dark mode → dark status bar with light text; light mode → light
    /// status bar with dark text. Fullscreen demo screens override to dark chrome.
    fn system_chrome_style(&self) -> SystemChromeStyle {
        let is_fullscreen_demo = matches!(self.current_screen, Screen::Animations | Screen::Shaders);
        let theme = gpui_mobile::components::material::MaterialTheme::from_appearance(self.dark_mode);

        if is_fullscreen_demo {
            SystemChromeStyle {
                status_bar_color: Some(BASE),
                status_bar_style: StatusBarContentStyle::Light,
                navigation_bar_color: Some(BASE),
            }
        } else {
            SystemChromeStyle {
                status_bar_color: Some(theme.surface),
                status_bar_style: if self.dark_mode {
                    StatusBarContentStyle::Light
                } else {
                    StatusBarContentStyle::Dark
                },
                navigation_bar_color: Some(if self.current_screen.is_tab_root() {
                    theme.surface_container // matches NavigationBar
                } else {
                    theme.surface // no tab bar, match content bg
                }),
            }
        }
    }

    /// Render the top navigation bar using the Material Design TopAppBar.
    fn render_nav_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let can_go_back = self.can_go_back();
        let title = self.current_screen.title();
        let theme = MaterialTheme::from_appearance(self.dark_mode);

        let mut bar = if can_go_back {
            TopAppBar::small(title, theme)
        } else {
            TopAppBar::center_aligned(title, theme)
        };

        if can_go_back {
            bar = bar.leading_icon(
                "←",
                cx.listener(|this, _event, _window, cx| {
                    this.go_back();
                    cx.notify();
                }),
            );
        }

        bar
    }

    /// Render the content area for the currently active screen.
    ///
    /// Regular screens are wrapped in a scrollable container. Demo screens
    /// (Animations, Shaders) fill the remaining space with their own content
    /// and touch handlers.
    fn render_current_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        match self.current_screen {
            Screen::Animations => {
                return self.render_animations_content(window, cx).into_any_element();
            }
            Screen::Shaders => {
                return self.render_shaders_content(window, cx).into_any_element();
            }
            _ => {}
        }

        let screen_content = match self.current_screen {
            Screen::Home => self.render_home_screen(cx).into_any_element(),
            Screen::Counter => self.render_counter_screen(cx).into_any_element(),
            Screen::Settings => self.render_settings_screen(cx).into_any_element(),
            Screen::About => self.render_about_screen(cx).into_any_element(),
            Screen::AppleGlass => self.render_apple_glass_screen(cx).into_any_element(),
            Screen::Material => self.render_material_screen(cx).into_any_element(),
            Screen::Form => self.render_form_screen(cx).into_any_element(),
            Screen::PackagesDemo => self.render_packages_demo_screen(cx).into_any_element(),
            Screen::WebViewBrowser => self.render_webview_browser_screen(cx).into_any_element(),
            Screen::Swiper => self.render_swiper_screen(cx).into_any_element(),
            Screen::Feed => self.render_feed_screen(cx).into_any_element(),
            Screen::Chat => self.render_chat_screen(cx).into_any_element(),
            Screen::Animations | Screen::Shaders => unreachable!(),
        };

        div()
            .id("screen-scroll-container")
            .flex_1()
            .overflow_y_scroll()
            // Dismiss keyboard when tapping outside text input fields.
            // Safe to use cx.listener here because text inputs use on_tap_notify
            // (no entity lease) so there's no double-lease conflict.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                    let mut changed = false;
                    if this.form.focused_field.is_some() {
                        this.form.focused_field = None;
                        this.form.full_name.selection = None;
                        this.form.email.selection = None;
                        this.form.phone.selection = None;
                        changed = true;
                    }
                    if this.chat_focused {
                        this.chat_focused = false;
                        changed = true;
                    }
                    if changed {
                        gpui_mobile::hide_keyboard();
                        gpui_mobile::set_text_input_callback(None);
                        cx.notify();
                    }
                }),
            )
            .child(screen_content)
            .into_any_element()
    }

    /// Render the bottom tab bar using the Material Design navigation bar.
    ///
    /// Animations and Shaders are accessible from the Home screen nav cards
    /// instead of occupying bottom bar slots.
    fn render_tab_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current_screen;
        let dark = self.dark_mode;

        NavigationBarBuilder::new(dark)
            .item(
                "🏠",
                "Home",
                current == Screen::Home,
                cx.listener(move |this, _, _, cx| {
                    this.navigate_to(Screen::Home);
                    cx.notify();
                }),
            )
            .item(
                "🔢",
                "Counter",
                current == Screen::Counter,
                cx.listener(move |this, _, _, cx| {
                    this.navigate_to(Screen::Counter);
                    cx.notify();
                }),
            )
            .item(
                "⚙️",
                "Settings",
                current == Screen::Settings,
                cx.listener(move |this, _, _, cx| {
                    this.navigate_to(Screen::Settings);
                    cx.notify();
                }),
            )
            .item(
                "ℹ️",
                "About",
                current == Screen::About,
                cx.listener(move |this, _, _, cx| {
                    this.navigate_to(Screen::About);
                    cx.notify();
                }),
            )
            .build()
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

    fn render_apple_glass_screen(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        components::render_apple_glass(self)
    }

    fn render_material_screen(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        components::render_material(self)
    }

    fn render_form_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        form::render(self, cx)
    }

    fn render_packages_demo_screen(&self, cx: &mut Context<Self>) -> impl IntoElement {
        packages_demo::render(self, cx)
    }

    fn render_webview_browser_screen(&self, cx: &mut Context<Self>) -> impl IntoElement {
        webview_browser::render(self, cx)
    }

    fn render_swiper_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        swiper::render(self, cx)
    }

    fn render_feed_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        feed::render(self, cx)
    }

    fn render_chat_screen(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        chat::render(self, cx)
    }

    // ── Demo screen content (rendered below the TopAppBar) ────────────────────

    /// Render the Animations content area with touch handlers.
    fn render_animations_content(
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
            .flex_1()
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
            .on_mouse_move(
                cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                    if let Some(playground) = &mut this.animation_playground {
                        let pos = point(event.position.x.as_f32(), event.position.y.as_f32());
                        if playground.touch_start.is_none() {
                            playground.touch_start = Some((pos, std::time::Instant::now()));
                        }
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
                                let color_rgb = crate::demos::random_color(playground.next_ball_id);
                                playground.spawn_particles(position, rgb(color_rgb).into());
                                playground.next_ball_id += 1;
                            } else {
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
            .child(if let Some(playground) = &mut self.animation_playground {
                playground.render_content(window).into_any_element()
            } else {
                div().into_any_element()
            })
    }

    /// Render the Shaders content area with touch handlers.
    fn render_shaders_content(
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
            .flex_1()
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
            .child(if let Some(showcase) = &mut self.shader_showcase {
                showcase.render_content(window).into_any_element()
            } else {
                div().into_any_element()
            })
    }
}

