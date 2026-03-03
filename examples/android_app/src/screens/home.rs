//! Home screen — welcome message, colour swatches, and quick-nav cards.

use gpui::{div, prelude::*, px, rgb};

use super::{Router, Screen, BLUE, GREEN, MAUVE, PEACH, SURFACE0, TEAL, TEXT, YELLOW};

/// Render the Home screen content area.
///
/// Takes a reference to the `Router` for reading shared state and a mutable
/// context for wiring up `cx.listener` navigation handlers on the nav cards.
pub fn render(router: &Router, cx: &mut gpui::Context<Router>) -> impl IntoElement {
    let user_name = router.user_name.clone();
    let tap_count = router.tap_count;
    let text_color = if router.dark_mode { TEXT } else { 0x4c4f69 };
    let card_bg = if router.dark_mode { SURFACE0 } else { 0xe6e9ef };
    let sub_text = if router.dark_mode {
        0xa6adc8_u32
    } else {
        0x6c6f85
    };

    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_4()
        .px_4()
        .py_6()
        // ── Welcome banner ───────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_xl()
                        .text_color(rgb(text_color))
                        .child(format!("Hello, {}!", &user_name)),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(sub_text))
                        .child("Welcome to the GPUI Android demo"),
                ),
        )
        // ── Colour swatches ──────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(sub_text))
                        .child("Catppuccin Mocha palette"),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(swatch(BLUE))
                        .child(swatch(GREEN))
                        .child(swatch(MAUVE))
                        .child(swatch(YELLOW))
                        .child(swatch(PEACH))
                        .child(swatch(TEAL)),
                ),
        )
        // ── Stats card ───────────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_row()
                .gap_3()
                .child(stat_card(
                    "Total taps",
                    &tap_count.to_string(),
                    BLUE,
                    card_bg,
                    text_color,
                ))
                .child(stat_card("Screens", "4", GREEN, card_bg, text_color)),
        )
        // ── Quick-nav cards ──────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(sub_text))
                        .child("Explore screens"),
                )
                .child(nav_card(
                    "[#]",
                    "Counter",
                    "Tap the button and watch it count",
                    BLUE,
                    card_bg,
                    text_color,
                    sub_text,
                    cx.listener(|this, _event, _window, cx| {
                        this.navigate_to(Screen::Counter);
                        cx.notify();
                    }),
                ))
                .child(nav_card(
                    "[S]",
                    "Settings",
                    "Toggle dark mode and configure options",
                    MAUVE,
                    card_bg,
                    text_color,
                    sub_text,
                    cx.listener(|this, _event, _window, cx| {
                        this.navigate_to(Screen::Settings);
                        cx.notify();
                    }),
                ))
                .child(nav_card(
                    "[i]",
                    "About",
                    "Learn about GPUI and this example app",
                    TEAL,
                    card_bg,
                    text_color,
                    sub_text,
                    cx.listener(|this, _event, _window, cx| {
                        this.navigate_to(Screen::About);
                        cx.notify();
                    }),
                )),
        )
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// A small coloured square swatch.
fn swatch(color: u32) -> impl IntoElement {
    div().size_10().rounded_lg().bg(rgb(color))
}

/// A small card showing a numeric stat.
fn stat_card(label: &str, value: &str, accent: u32, bg: u32, text_color: u32) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_row()
        .gap_3()
        .p_4()
        .rounded_xl()
        .bg(rgb(bg))
        .items_center()
        .child(div().w(px(4.0)).h_full().rounded_sm().bg(rgb(accent)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_2xl()
                        .text_color(rgb(text_color))
                        .child(value.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(accent))
                        .child(label.to_string()),
                ),
        )
}

/// A navigation card that shows an icon, title, and description.
///
/// Tapping the card triggers the provided `handler`, which should navigate
/// to the target screen via `Router::navigate_to`.
fn nav_card(
    icon: &str,
    title: &str,
    description: &str,
    accent: u32,
    bg: u32,
    text_color: u32,
    sub_text: u32,
    handler: impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .gap_3()
        .p_4()
        .rounded_xl()
        .bg(rgb(bg))
        .items_center()
        .child(div().w(px(4.0)).h_full().rounded_sm().bg(rgb(accent)))
        .child(div().text_2xl().child(icon.to_string()))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap_1()
                .child(
                    div()
                        .text_base()
                        .text_color(rgb(text_color))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child(description.to_string()),
                ),
        )
        .child(div().text_sm().text_color(rgb(accent)).child("→"))
        .on_mouse_down(gpui::MouseButton::Left, handler)
}
