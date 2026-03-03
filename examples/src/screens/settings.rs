//! Settings screen — toggle dark mode and other preferences.
//!
//! This screen demonstrates interactive toggles and controls that mutate shared
//! state on the `Router` and trigger re-renders via `cx.notify()`.

use gpui::{div, prelude::*, px, rgb, App, MouseDownEvent, Window};

use super::{Router, BLUE, GREEN, MANTLE, MAUVE, PEACH, RED, SURFACE0, SURFACE1, TEXT, YELLOW};

/// Render the Settings screen content area.
///
/// Takes a mutable reference to the `Router` (for `cx.listener` closures that
/// mutate settings) and the GPUI context.
pub fn render(router: &Router, cx: &mut gpui::Context<Router>) -> impl IntoElement {
    let dark_mode = router.dark_mode;
    let text_color = if dark_mode { TEXT } else { 0x4c4f69 };
    let sub_text = if dark_mode { 0xa6adc8_u32 } else { 0x6c6f85 };
    let card_bg = if dark_mode { SURFACE0 } else { 0xe6e9ef };
    let divider_color = if dark_mode { SURFACE1 } else { 0xdce0e8 };

    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_4()
        .px_4()
        .py_6()
        // ── Section: Appearance ───────────────────────────────────────────
        .child(section_header("Appearance", sub_text))
        .child(
            settings_card(card_bg)
                // Dark mode toggle
                .child(toggle_row(
                    "[*]",
                    "Dark Mode",
                    "Use a dark colour scheme",
                    dark_mode,
                    text_color,
                    sub_text,
                    cx.listener(|this, _event, _window, cx| {
                        this.dark_mode = !this.dark_mode;
                        cx.notify();
                    }),
                ))
                .child(divider(divider_color)),
        )
        // ── Section: Counter ─────────────────────────────────────────────
        .child(section_header("Counter", sub_text))
        .child(
            settings_card(card_bg)
                // Reset counter
                .child(action_row(
                    "[R]",
                    "Reset Counter",
                    "Set the counter back to zero",
                    RED,
                    text_color,
                    sub_text,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count = 0;
                        cx.notify();
                    }),
                ))
                .child(divider(divider_color))
                // Set counter to 100
                .child(action_row(
                    "100",
                    "Set to 100",
                    "Jump the counter to 100",
                    PEACH,
                    text_color,
                    sub_text,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count = 100;
                        cx.notify();
                    }),
                ))
                .child(divider(divider_color))
                // Set counter to 500
                .child(action_row(
                    "[^]",
                    "Set to 500",
                    "Jump the counter to 500",
                    YELLOW,
                    text_color,
                    sub_text,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count = 500;
                        cx.notify();
                    }),
                )),
        )
        // ── Section: Profile ─────────────────────────────────────────────
        .child(section_header("Profile", sub_text))
        .child(settings_card(card_bg).child(action_row(
            "[U]",
            "Change Name",
            &format!("Currently: {}", router.user_name),
            BLUE,
            text_color,
            sub_text,
            cx.listener(|this, _event, _window, cx| {
                // Cycle through a few demo names.
                let names = ["Android", "GPUI User", "Rustacean", "Mobile Dev"];
                let current_idx = names
                    .iter()
                    .position(|n| *n == this.user_name.as_ref())
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % names.len();
                this.user_name = names[next_idx].into();
                cx.notify();
            }),
        )))
        // ── Section: Theme Preview ───────────────────────────────────────
        .child(section_header("Theme Preview", sub_text))
        .child(
            settings_card(card_bg).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_3()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(sub_text))
                            .child(if dark_mode {
                                "Catppuccin Mocha (Dark)"
                            } else {
                                "Catppuccin Latte (Light)"
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(colour_chip(BLUE, "Blue"))
                            .child(colour_chip(GREEN, "Green"))
                            .child(colour_chip(MAUVE, "Mauve"))
                            .child(colour_chip(YELLOW, "Yellow"))
                            .child(colour_chip(PEACH, "Peach"))
                            .child(colour_chip(RED, "Red")),
                    ),
            ),
        )
        // ── Footer ───────────────────────────────────────────────────────
        .child(
            div()
                .mt_4()
                .text_xs()
                .text_center()
                .text_color(rgb(sub_text))
                .child("Settings are stored in memory only"),
        )
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Section header label.
fn section_header(title: &str, color: u32) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(rgb(color))
        .px_1()
        .child(title.to_string().to_uppercase())
}

/// A rounded card container for settings rows.
fn settings_card(bg: u32) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .rounded_xl()
        .bg(rgb(bg))
        .overflow_hidden()
}

/// A horizontal divider line.
fn divider(color: u32) -> impl IntoElement {
    div().w_full().h(px(1.0)).bg(rgb(color)).mx_3()
}

/// A row with a toggle indicator (on/off).
fn toggle_row(
    icon: &str,
    title: &str,
    description: &str,
    is_on: bool,
    text_color: u32,
    sub_text: u32,
    handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let toggle_bg = if is_on { GREEN } else { SURFACE1 };
    let toggle_label = if is_on { "ON" } else { "OFF" };
    let toggle_text = if is_on { MANTLE } else { sub_text };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_4()
        .py_3()
        .child(div().text_xl().child(icon.to_string()))
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
        .child(
            div()
                .px_3()
                .py_1()
                .rounded_full()
                .bg(rgb(toggle_bg))
                .text_xs()
                .text_color(rgb(toggle_text))
                .child(toggle_label),
        )
        .on_mouse_down(gpui::MouseButton::Left, handler)
}

/// A row with a tappable action.
fn action_row(
    icon: &str,
    title: &str,
    description: &str,
    accent: u32,
    text_color: u32,
    sub_text: u32,
    handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_4()
        .py_3()
        .child(div().text_xl().child(icon.to_string()))
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

/// A small colour chip with a label underneath.
fn colour_chip(color: u32, label: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .child(div().size_8().rounded_lg().bg(rgb(color)))
        .child(
            div()
                .text_xs()
                .text_color(rgb(color))
                .child(label.to_string()),
        )
}
