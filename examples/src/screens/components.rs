//! Components showcase screen — Apple Glass & Material Design.
//!
//! This screen demonstrates two distinct design languages built with
//! raw GPUI primitives, running natively on iOS (Metal) and Android (Vulkan):
//!
//! - **Apple Glass** — frosted translucent panels, vibrancy, SF-style
//!   controls, thin separators, and subtle depth.
//! - **Material Design** — elevated cards, FABs, filled/outlined buttons,
//!   chips, outlined text fields, snackbars, and bottom sheets.

use gpui::{
    div, hsla, linear_color_stop, linear_gradient, prelude::*, px, relative, rgb, SharedString,
};

use super::{
    Router, BLUE, GREEN, LAVENDER, MANTLE, MAUVE, PEACH, RED, SKY, SURFACE0, SURFACE1, TEAL, TEXT,
    YELLOW,
};

// ── Public render entry point ────────────────────────────────────────────────

/// Render the Components showcase screen.
pub fn render(router: &Router) -> impl IntoElement {
    let dark = router.dark_mode;
    let text_color = if dark { TEXT } else { 0x4c4f69 };
    let sub_text: u32 = if dark { 0xa6adc8 } else { 0x6c6f85 };

    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_6()
        .px_4()
        .py_6()
        // ════════════════════════════════════════════════════════════════
        //  APPLE GLASS SECTION
        // ════════════════════════════════════════════════════════════════
        .child(design_language_header(
            "Apple Glass",
            "Frosted panels · Vibrancy · SF-style controls",
            BLUE,
            sub_text,
        ))
        // ── Glass hero card ──────────────────────────────────────────────
        .child(glass_hero_card(dark))
        // ── Glass action buttons ─────────────────────────────────────────
        .child(section_label("Buttons", sub_text))
        .child(glass_buttons_row(dark))
        // ── Glass segmented control ──────────────────────────────────────
        .child(section_label("Segmented Control", sub_text))
        .child(glass_segmented_control(dark))
        // ── Glass list / settings ────────────────────────────────────────
        .child(section_label("Settings List", sub_text))
        .child(glass_settings_list(dark))
        // ── Glass notification banners ───────────────────────────────────
        .child(section_label("Notification Banners", sub_text))
        .child(glass_notification_banners(dark))
        // ── Glass search bar ─────────────────────────────────────────────
        .child(section_label("Search Bar", sub_text))
        .child(glass_search_bar(dark))
        // ── Glass slider ─────────────────────────────────────────────────
        .child(section_label("Sliders", sub_text))
        .child(glass_sliders(dark))
        // ── Glass tab bar ────────────────────────────────────────────────
        .child(section_label("Tab Bar", sub_text))
        .child(glass_tab_bar(dark))
        // ════════════════════════════════════════════════════════════════
        //  MATERIAL DESIGN SECTION
        // ════════════════════════════════════════════════════════════════
        .child(div().mt_4().child(design_language_header(
            "Material Design",
            "Elevation · FABs · Chips · Outlined fields",
            GREEN,
            sub_text,
        )))
        // ── Material hero card ───────────────────────────────────────────
        .child(material_hero_card(dark))
        // ── Material buttons ─────────────────────────────────────────────
        .child(section_label("Buttons", sub_text))
        .child(material_buttons(dark))
        // ── Material FABs ────────────────────────────────────────────────
        .child(section_label("Floating Action Buttons", sub_text))
        .child(material_fabs(dark))
        // ── Material chips ───────────────────────────────────────────────
        .child(section_label("Chips", sub_text))
        .child(material_chips(dark))
        // ── Material text fields ─────────────────────────────────────────
        .child(section_label("Text Fields", sub_text))
        .child(material_text_fields(dark))
        // ── Material cards ───────────────────────────────────────────────
        .child(section_label("Cards", sub_text))
        .child(material_cards(dark))
        // ── Material snackbar ────────────────────────────────────────────
        .child(section_label("Snackbar", sub_text))
        .child(material_snackbar(dark))
        // ── Material bottom sheet ────────────────────────────────────────
        .child(section_label("Bottom Sheet", sub_text))
        .child(material_bottom_sheet(dark))
        // ── Material navigation bar ──────────────────────────────────────
        .child(section_label("Navigation Bar", sub_text))
        .child(material_navigation_bar(dark))
        // ── Shared components ────────────────────────────────────────────
        .child(div().mt_4().child(design_language_header(
            "Shared Patterns",
            "Progress · Avatars · Badges · Stats",
            MAUVE,
            sub_text,
        )))
        .child(section_label("Progress Indicators", sub_text))
        .child(shared_progress_bars(dark))
        .child(section_label("Avatars", sub_text))
        .child(shared_avatars(dark))
        .child(section_label("Badges", sub_text))
        .child(shared_badges(dark))
        .child(section_label("Stat Cards", sub_text))
        .child(shared_stat_cards(dark))
        .child(section_label("Skeleton Loaders", sub_text))
        .child(shared_skeleton_loaders(dark))
        // ── Footer ───────────────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .py_6()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child("Components built with raw GPUI primitives"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child("Apple Glass · Material Design · Cross-platform"),
                ),
        )
}

// ═════════════════════════════════════════════════════════════════════════════
// Layout helpers
// ═════════════════════════════════════════════════════════════════════════════

fn design_language_header(
    title: &str,
    subtitle: &str,
    accent: u32,
    sub_text: u32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .child(
            div()
                .w(px(4.0))
                .h(px(32.0))
                .rounded(px(2.0))
                .bg(rgb(accent)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_lg()
                        .text_color(rgb(accent))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child(subtitle.to_string()),
                ),
        )
}

fn section_label(title: &str, color: u32) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(rgb(color))
        .px_1()
        .child(title.to_string().to_uppercase())
}

/// Thin 0.5px separator used in Apple-style lists.
fn glass_separator(dark: bool) -> impl IntoElement {
    let color = if dark {
        hsla(0.0, 0.0, 1.0, 0.08)
    } else {
        hsla(0.0, 0.0, 0.0, 0.08)
    };
    div().w_full().h(px(0.5)).bg(color).ml(px(52.0))
}

/// Full-width thin separator.
fn glass_separator_full(dark: bool) -> impl IntoElement {
    let color = if dark {
        hsla(0.0, 0.0, 1.0, 0.08)
    } else {
        hsla(0.0, 0.0, 0.0, 0.08)
    };
    div().w_full().h(px(0.5)).bg(color)
}

// ═════════════════════════════════════════════════════════════════════════════
// APPLE GLASS COMPONENTS
// ═════════════════════════════════════════════════════════════════════════════

/// Glass panel base — simulates frosted glass with translucent bg + border.
fn glass_panel(dark: bool) -> gpui::Div {
    let bg = if dark {
        hsla(0.0, 0.0, 1.0, 0.06)
    } else {
        hsla(0.0, 0.0, 1.0, 0.72)
    };
    let border = if dark {
        hsla(0.0, 0.0, 1.0, 0.1)
    } else {
        hsla(0.0, 0.0, 0.0, 0.06)
    };

    div()
        .flex()
        .flex_col()
        .rounded(px(13.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .overflow_hidden()
}

/// Hero card with gradient mesh background + glass overlay.
fn glass_hero_card(dark: bool) -> impl IntoElement {
    let text_primary = if dark {
        hsla(0.0, 0.0, 1.0, 0.92)
    } else {
        hsla(0.0, 0.0, 0.0, 0.85)
    };
    let text_secondary = if dark {
        hsla(0.0, 0.0, 1.0, 0.55)
    } else {
        hsla(0.0, 0.0, 0.0, 0.45)
    };
    let panel_bg = if dark {
        hsla(0.72, 0.5, 0.3, 0.25)
    } else {
        hsla(0.72, 0.5, 0.85, 0.6)
    };
    let panel_border = if dark {
        hsla(0.72, 0.3, 0.5, 0.2)
    } else {
        hsla(0.72, 0.3, 0.8, 0.3)
    };

    div()
        .flex()
        .flex_col()
        .rounded(px(16.0))
        .overflow_hidden()
        .bg(linear_gradient(
            135.0,
            linear_color_stop(
                if dark {
                    hsla(0.6, 0.6, 0.2, 1.0)
                } else {
                    hsla(0.6, 0.4, 0.85, 1.0)
                },
                0.0,
            ),
            linear_color_stop(
                if dark {
                    hsla(0.8, 0.5, 0.25, 1.0)
                } else {
                    hsla(0.8, 0.4, 0.9, 1.0)
                },
                1.0,
            ),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .p_5()
                .bg(panel_bg)
                .border_1()
                .border_color(panel_border)
                .rounded(px(16.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(44.0))
                                .rounded(px(10.0))
                                .bg(hsla(0.6, 0.7, 0.6, 0.3))
                                .text_xl()
                                .child("🍎"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_lg()
                                        .text_color(text_primary)
                                        .child("Frosted Glass UI"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_secondary)
                                        .child("Translucent panels with vibrancy"),
                                ),
                        ),
                )
                .child(div().text_sm().text_color(text_secondary).child(
                    "Layered translucent materials create depth and \
                             visual hierarchy, inspired by visionOS and iOS design.",
                )),
        )
}

fn glass_buttons_row(dark: bool) -> impl IntoElement {
    let text_white = hsla(0.0, 0.0, 1.0, 0.92);

    div()
        .flex()
        .flex_col()
        .gap_3()
        // Row 1: Tinted glass buttons (iOS style)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(glass_button_tinted("Primary", 0.6, 0.6, 0.5))
                .child(glass_button_tinted("Success", 0.38, 0.7, 0.45))
                .child(glass_button_tinted("Danger", 0.0, 0.7, 0.5))
                .child(glass_button_tinted("Warning", 0.1, 0.8, 0.5)),
        )
        // Row 2: Plain glass (gray) buttons
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(glass_button_plain("Cancel", dark))
                .child(glass_button_plain("Skip", dark))
                .child(glass_button_plain("Later", dark)),
        )
        // Row 3: Pill / capsule buttons (SF style)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(
                    div()
                        .px_5()
                        .py(px(10.0))
                        .rounded(px(20.0))
                        .bg(rgb(BLUE))
                        .text_sm()
                        .text_color(text_white)
                        .child("Get Started"),
                )
                .child(
                    div()
                        .px_5()
                        .py(px(10.0))
                        .rounded(px(20.0))
                        .border_1()
                        .border_color(rgb(BLUE))
                        .text_sm()
                        .text_color(rgb(BLUE))
                        .child("Learn More"),
                ),
        )
}

fn glass_button_tinted(label: &str, hue: f32, sat: f32, light: f32) -> impl IntoElement {
    div()
        .px_4()
        .py(px(8.0))
        .rounded(px(10.0))
        .bg(hsla(hue, sat, light, 0.18))
        .text_sm()
        .text_color(hsla(hue, sat, light + 0.15, 1.0))
        .child(label.to_string())
}

fn glass_button_plain(label: &str, dark: bool) -> impl IntoElement {
    let bg = if dark {
        hsla(0.0, 0.0, 1.0, 0.08)
    } else {
        hsla(0.0, 0.0, 0.0, 0.05)
    };
    let fg = if dark {
        hsla(0.0, 0.0, 1.0, 0.7)
    } else {
        hsla(0.0, 0.0, 0.0, 0.55)
    };

    div()
        .px_4()
        .py(px(8.0))
        .rounded(px(10.0))
        .bg(bg)
        .text_sm()
        .text_color(fg)
        .child(label.to_string())
}

fn glass_segmented_control(dark: bool) -> impl IntoElement {
    let track_bg = if dark {
        hsla(0.0, 0.0, 1.0, 0.06)
    } else {
        hsla(0.0, 0.0, 0.0, 0.04)
    };
    let active_bg = if dark {
        hsla(0.0, 0.0, 1.0, 0.12)
    } else {
        hsla(0.0, 0.0, 1.0, 0.85)
    };
    let active_fg = if dark {
        hsla(0.0, 0.0, 1.0, 0.92)
    } else {
        hsla(0.0, 0.0, 0.0, 0.85)
    };
    let inactive_fg = if dark {
        hsla(0.0, 0.0, 1.0, 0.45)
    } else {
        hsla(0.0, 0.0, 0.0, 0.4)
    };
    let border = if dark {
        hsla(0.0, 0.0, 1.0, 0.1)
    } else {
        hsla(0.0, 0.0, 0.0, 0.08)
    };

    let segment = |label: &str, active: bool| -> gpui::Div {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .py(px(7.0))
            .rounded(px(8.0))
            .text_sm()
            .when(active, |d| {
                d.bg(active_bg)
                    .text_color(active_fg)
                    .border_1()
                    .border_color(border)
            })
            .when(!active, |d| d.text_color(inactive_fg))
            .child(label.to_string())
    };

    div()
        .flex()
        .flex_row()
        .p(px(2.0))
        .rounded(px(10.0))
        .bg(track_bg)
        .border_1()
        .border_color(border)
        .child(segment("Day", false))
        .child(segment("Week", true))
        .child(segment("Month", false))
        .child(segment("Year", false))
}

fn glass_settings_list(dark: bool) -> impl IntoElement {
    let text_primary = if dark {
        hsla(0.0, 0.0, 1.0, 0.92)
    } else {
        hsla(0.0, 0.0, 0.0, 0.85)
    };
    let text_secondary = if dark {
        hsla(0.0, 0.0, 1.0, 0.45)
    } else {
        hsla(0.0, 0.0, 0.0, 0.4)
    };

    glass_panel(dark)
        .child(glass_settings_row(
            "🔔",
            "Notifications",
            Some("On"),
            true,
            text_primary,
            text_secondary,
            dark,
        ))
        .child(glass_separator(dark))
        .child(glass_settings_row(
            "🌙",
            "Dark Mode",
            None,
            true,
            text_primary,
            text_secondary,
            dark,
        ))
        .child(glass_separator(dark))
        .child(glass_settings_row(
            "🔒",
            "Face ID",
            None,
            true,
            text_primary,
            text_secondary,
            dark,
        ))
        .child(glass_separator(dark))
        .child(glass_settings_row(
            "📶",
            "Wi-Fi",
            Some("Connected"),
            false,
            text_primary,
            text_secondary,
            dark,
        ))
        .child(glass_separator(dark))
        .child(glass_settings_row(
            "🔋",
            "Battery",
            Some("85%"),
            false,
            text_primary,
            text_secondary,
            dark,
        ))
}

fn glass_settings_row(
    icon: &str,
    title: &str,
    detail: Option<&str>,
    has_toggle: bool,
    text_primary: gpui::Hsla,
    text_secondary: gpui::Hsla,
    dark: bool,
) -> impl IntoElement {
    let row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_4()
        .py(px(11.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(30.0))
                .rounded(px(7.0))
                .bg(hsla(0.6, 0.5, 0.5, 0.2))
                .text_base()
                .child(icon.to_string()),
        )
        .child(
            div()
                .flex_1()
                .text_base()
                .text_color(text_primary)
                .child(title.to_string()),
        );

    if has_toggle {
        row.child(ios_toggle(true, dark))
    } else {
        let detail_text = detail.unwrap_or("");
        row.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_base()
                        .text_color(text_secondary)
                        .child(detail_text.to_string()),
                )
                .child(div().text_base().text_color(text_secondary).child("›")),
        )
    }
}

/// iOS-style toggle switch.
fn ios_toggle(is_on: bool, _dark: bool) -> impl IntoElement {
    let track_color = if is_on {
        hsla(0.38, 0.75, 0.50, 1.0) // green
    } else {
        hsla(0.0, 0.0, 0.5, 0.2)
    };
    let thumb_color = hsla(0.0, 0.0, 1.0, 0.95);

    div()
        .flex()
        .flex_row()
        .items_center()
        .w(px(51.0))
        .h(px(31.0))
        .rounded(px(16.0))
        .bg(track_color)
        .px(px(2.0))
        .when(is_on, |d| d.justify_end())
        .when(!is_on, |d| d.justify_start())
        .child(div().size(px(27.0)).rounded_full().bg(thumb_color))
}

fn glass_notification_banners(dark: bool) -> impl IntoElement {
    let text_primary = if dark {
        hsla(0.0, 0.0, 1.0, 0.92)
    } else {
        hsla(0.0, 0.0, 0.0, 0.85)
    };
    let text_secondary = if dark {
        hsla(0.0, 0.0, 1.0, 0.55)
    } else {
        hsla(0.0, 0.0, 0.0, 0.45)
    };

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(glass_notification(
            "📱",
            "Messages",
            "Hey! Are you coming to the meetup?",
            "now",
            text_primary,
            text_secondary,
            dark,
        ))
        .child(glass_notification(
            "📧",
            "Mail",
            "Your order has been shipped",
            "2m ago",
            text_primary,
            text_secondary,
            dark,
        ))
}

fn glass_notification(
    icon: &str,
    app: &str,
    message: &str,
    time: &str,
    text_primary: gpui::Hsla,
    text_secondary: gpui::Hsla,
    dark: bool,
) -> impl IntoElement {
    glass_panel(dark).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .p_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(38.0))
                    .rounded(px(9.0))
                    .bg(rgb(BLUE))
                    .text_lg()
                    .child(icon.to_string()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(text_primary)
                                    .child(app.to_string()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(text_secondary)
                                    .child(time.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_secondary)
                            .child(message.to_string()),
                    ),
            ),
    )
}

fn glass_search_bar(dark: bool) -> impl IntoElement {
    let bg = if dark {
        hsla(0.0, 0.0, 1.0, 0.06)
    } else {
        hsla(0.0, 0.0, 0.0, 0.04)
    };
    let border = if dark {
        hsla(0.0, 0.0, 1.0, 0.08)
    } else {
        hsla(0.0, 0.0, 0.0, 0.06)
    };
    let placeholder = if dark {
        hsla(0.0, 0.0, 1.0, 0.3)
    } else {
        hsla(0.0, 0.0, 0.0, 0.3)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px_3()
        .py(px(10.0))
        .rounded(px(12.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .child(div().text_base().text_color(placeholder).child("🔍"))
        .child(
            div()
                .flex_1()
                .text_base()
                .text_color(placeholder)
                .child("Search…"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(20.0))
                .rounded_full()
                .bg(if dark {
                    hsla(0.0, 0.0, 1.0, 0.1)
                } else {
                    hsla(0.0, 0.0, 0.0, 0.08)
                })
                .text_xs()
                .text_color(placeholder)
                .child("✕"),
        )
}

fn glass_sliders(dark: bool) -> impl IntoElement {
    let track_inactive = if dark {
        hsla(0.0, 0.0, 1.0, 0.1)
    } else {
        hsla(0.0, 0.0, 0.0, 0.08)
    };

    glass_panel(dark).child(
        div()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .child(glass_slider_row(
                "Brightness",
                "☀️",
                0.7,
                BLUE,
                track_inactive,
                dark,
            ))
            .child(glass_separator_full(dark))
            .child(glass_slider_row(
                "Volume",
                "🔊",
                0.45,
                GREEN,
                track_inactive,
                dark,
            ))
            .child(glass_separator_full(dark))
            .child(glass_slider_row(
                "Opacity",
                "💧",
                0.85,
                MAUVE,
                track_inactive,
                dark,
            )),
    )
}

fn glass_slider_row(
    label: &str,
    icon: &str,
    value: f32,
    accent: u32,
    track_inactive: gpui::Hsla,
    dark: bool,
) -> impl IntoElement {
    let text_secondary = if dark {
        hsla(0.0, 0.0, 1.0, 0.55)
    } else {
        hsla(0.0, 0.0, 0.0, 0.45)
    };

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .items_center()
                        .child(div().text_base().child(icon.to_string()))
                        .child(
                            div()
                                .text_sm()
                                .text_color(text_secondary)
                                .child(label.to_string()),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(text_secondary)
                        .child(format!("{}%", (value * 100.0) as u32)),
                ),
        )
        .child(
            // Track
            div()
                .w_full()
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(track_inactive)
                .relative()
                .child(
                    // Fill
                    div()
                        .h_full()
                        .rounded(px(3.0))
                        .bg(rgb(accent))
                        .w(relative(value)),
                )
                // Thumb (overlaid at the end of the fill)
                .child(
                    div()
                        .absolute()
                        .top(px(-9.0))
                        .left(relative(value))
                        .ml(px(-12.0))
                        .size(px(24.0))
                        .rounded_full()
                        .bg(hsla(0.0, 0.0, 1.0, 0.95))
                        .border_1()
                        .border_color(hsla(0.0, 0.0, 0.0, 0.08)),
                ),
        )
}

fn glass_tab_bar(dark: bool) -> impl IntoElement {
    let bg = if dark {
        hsla(0.0, 0.0, 1.0, 0.05)
    } else {
        hsla(0.0, 0.0, 1.0, 0.7)
    };
    let border = if dark {
        hsla(0.0, 0.0, 1.0, 0.08)
    } else {
        hsla(0.0, 0.0, 0.0, 0.06)
    };
    let active_color = hsla(0.6, 0.8, 0.6, 1.0);
    let inactive_color = if dark {
        hsla(0.0, 0.0, 1.0, 0.35)
    } else {
        hsla(0.0, 0.0, 0.0, 0.35)
    };

    let tab = |icon: &str, label: &str, active: bool| -> gpui::Div {
        let color = if active { active_color } else { inactive_color };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.0))
            .flex_1()
            .child(div().text_xl().text_color(color).child(icon.to_string()))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(color)
                    .child(label.to_string()),
            )
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .py(px(6.0))
        .px_2()
        .rounded(px(16.0))
        .bg(bg)
        .border_t_1()
        .border_color(border)
        .child(tab("🏠", "Home", true))
        .child(tab("🔍", "Search", false))
        .child(tab("❤️", "Favorites", false))
        .child(tab("👤", "Profile", false))
}

// ═════════════════════════════════════════════════════════════════════════════
// MATERIAL DESIGN COMPONENTS
// ═════════════════════════════════════════════════════════════════════════════

/// Material surface — elevated card with subtle shadow simulation.
fn material_surface(dark: bool, elevation: u8) -> gpui::Div {
    let bg = if dark {
        match elevation {
            0 => rgb(0x121212),
            1 => rgb(0x1e1e1e),
            2 => rgb(0x232323),
            _ => rgb(0x282828),
        }
    } else {
        rgb(0xffffff)
    };
    let border = if dark {
        hsla(0.0, 0.0, 1.0, 0.04 * elevation as f32)
    } else {
        hsla(0.0, 0.0, 0.0, 0.04 * elevation as f32)
    };

    div()
        .flex()
        .flex_col()
        .rounded(px(12.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .overflow_hidden()
}

fn material_hero_card(dark: bool) -> impl IntoElement {
    let text_on_primary = hsla(0.0, 0.0, 1.0, 0.95);

    div()
        .flex()
        .flex_col()
        .rounded(px(16.0))
        .overflow_hidden()
        .bg(linear_gradient(
            135.0,
            linear_color_stop(
                if dark {
                    hsla(0.38, 0.65, 0.35, 1.0)
                } else {
                    hsla(0.38, 0.65, 0.42, 1.0)
                },
                0.0,
            ),
            linear_color_stop(
                if dark {
                    hsla(0.45, 0.55, 0.3, 1.0)
                } else {
                    hsla(0.45, 0.55, 0.38, 1.0)
                },
                1.0,
            ),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .p_5()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(44.0))
                                .rounded_full()
                                .bg(hsla(0.0, 0.0, 1.0, 0.15))
                                .text_xl()
                                .child("🤖"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_lg()
                                        .text_color(text_on_primary)
                                        .child("Material Design"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(hsla(0.0, 0.0, 1.0, 0.7))
                                        .child("Elevation · Color · Motion"),
                                ),
                        ),
                )
                .child(div().text_sm().text_color(hsla(0.0, 0.0, 1.0, 0.75)).child(
                    "Material Design uses environmental cues like \
                             surfaces, depth, and shadow to express hierarchy.",
                )),
        )
}

fn material_buttons(dark: bool) -> impl IntoElement {
    let md_primary = if dark { 0xd0bcff_u32 } else { 0x6750a4 };
    let md_on_primary = if dark { 0x381e72_u32 } else { 0xffffff };
    let md_secondary = if dark { 0x332d41_u32 } else { 0xe8def8 };
    let md_on_secondary = if dark { 0xe8def8_u32 } else { 0x1d192b };
    let md_outline = if dark { 0x938f99_u32 } else { 0x79747e };
    let md_on_surface = if dark { 0xe6e1e5_u32 } else { 0x1c1b1f };
    let md_surface_variant = if dark { 0x49454f_u32 } else { 0xe7e0ec };

    div()
        .flex()
        .flex_col()
        .gap_3()
        // Row 1: Filled buttons
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(material_button_filled("Filled", md_primary, md_on_primary))
                .child(material_button_filled("Accept", 0xa6e3a1, 0x1e1e2e))
                .child(material_button_filled("Delete", 0xf38ba8, 0x1e1e2e)),
        )
        // Row 2: Tonal buttons
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(material_button_tonal(
                    "Tonal",
                    md_secondary,
                    md_on_secondary,
                ))
                .child(material_button_tonal(
                    "Secondary",
                    md_surface_variant,
                    md_on_surface,
                )),
        )
        // Row 3: Outlined buttons
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(material_button_outlined("Outlined", md_outline))
                .child(material_button_outlined("Cancel", md_outline)),
        )
        // Row 4: Text buttons
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(material_button_text("Text Button", md_primary))
                .child(material_button_text("Learn More", md_primary)),
        )
}

fn material_button_filled(label: &str, bg: u32, fg: u32) -> impl IntoElement {
    div()
        .px_6()
        .py(px(10.0))
        .rounded(px(20.0))
        .bg(rgb(bg))
        .text_sm()
        .text_color(rgb(fg))
        .child(label.to_string())
}

fn material_button_tonal(label: &str, bg: u32, fg: u32) -> impl IntoElement {
    div()
        .px_6()
        .py(px(10.0))
        .rounded(px(20.0))
        .bg(rgb(bg))
        .text_sm()
        .text_color(rgb(fg))
        .child(label.to_string())
}

fn material_button_outlined(label: &str, outline: u32) -> impl IntoElement {
    div()
        .px_6()
        .py(px(10.0))
        .rounded(px(20.0))
        .border_1()
        .border_color(rgb(outline))
        .text_sm()
        .text_color(rgb(outline))
        .child(label.to_string())
}

fn material_button_text(label: &str, color: u32) -> impl IntoElement {
    div()
        .px_3()
        .py(px(10.0))
        .text_sm()
        .text_color(rgb(color))
        .child(label.to_string())
}

fn material_fabs(dark: bool) -> impl IntoElement {
    let md_primary_container = if dark { 0x4f378b_u32 } else { 0xeaddff };
    let md_on_primary_container = if dark { 0xeaddff_u32 } else { 0x21005e };
    let md_secondary_container = if dark { 0x4a4458_u32 } else { 0xe8def8 };
    let md_on_secondary_container = if dark { 0xe8def8_u32 } else { 0x1d192b };
    let md_tertiary_container = if dark { 0x633b48_u32 } else { 0xffd8e4 };
    let md_on_tertiary_container = if dark { 0xffd8e4_u32 } else { 0x31111d };

    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_end()
        .gap_3()
        // Small FAB
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(40.0))
                .rounded(px(12.0))
                .bg(rgb(md_secondary_container))
                .text_lg()
                .text_color(rgb(md_on_secondary_container))
                .child("+"),
        )
        // Regular FAB
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(56.0))
                .rounded(px(16.0))
                .bg(rgb(md_primary_container))
                .text_2xl()
                .text_color(rgb(md_on_primary_container))
                .child("✏️"),
        )
        // Large FAB
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(96.0))
                .rounded(px(28.0))
                .bg(rgb(md_tertiary_container))
                .text_3xl()
                .text_color(rgb(md_on_tertiary_container))
                .child("📷"),
        )
        // Extended FAB
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_4()
                .h(px(56.0))
                .rounded(px(16.0))
                .bg(rgb(md_primary_container))
                .text_color(rgb(md_on_primary_container))
                .child(div().text_lg().child("✏️"))
                .child(div().text_sm().child("Compose")),
        )
}

fn material_chips(dark: bool) -> impl IntoElement {
    let outline = if dark { 0x938f99_u32 } else { 0x79747e };
    let on_surface = if dark { 0xe6e1e5_u32 } else { 0x1c1b1f };
    let selected_bg = if dark { 0x4a4458_u32 } else { 0xe8def8 };

    div()
        .flex()
        .flex_col()
        .gap_3()
        // Assist / suggestion chips
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(material_chip(
                    "🎵 Music",
                    false,
                    outline,
                    on_surface,
                    selected_bg,
                ))
                .child(material_chip(
                    "📸 Photos",
                    true,
                    outline,
                    on_surface,
                    selected_bg,
                ))
                .child(material_chip(
                    "🎬 Videos",
                    false,
                    outline,
                    on_surface,
                    selected_bg,
                ))
                .child(material_chip(
                    "📄 Docs",
                    false,
                    outline,
                    on_surface,
                    selected_bg,
                )),
        )
        // Filter chips
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(material_chip(
                    "✓ Nearby",
                    true,
                    outline,
                    on_surface,
                    selected_bg,
                ))
                .child(material_chip(
                    "Open Now",
                    false,
                    outline,
                    on_surface,
                    selected_bg,
                ))
                .child(material_chip(
                    "✓ 4+ Stars",
                    true,
                    outline,
                    on_surface,
                    selected_bg,
                ))
                .child(material_chip(
                    "Free WiFi",
                    false,
                    outline,
                    on_surface,
                    selected_bg,
                )),
        )
}

fn material_chip(
    label: &str,
    selected: bool,
    outline: u32,
    on_surface: u32,
    selected_bg: u32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(32.0))
        .px_3()
        .rounded(px(8.0))
        .when(selected, |d| d.bg(rgb(selected_bg)))
        .when(!selected, |d| d.border_1().border_color(rgb(outline)))
        .text_sm()
        .text_color(rgb(on_surface))
        .child(label.to_string())
}

fn material_text_fields(dark: bool) -> impl IntoElement {
    let outline = if dark { 0x938f99_u32 } else { 0x79747e };
    let on_surface = if dark { 0xe6e1e5_u32 } else { 0x1c1b1f };
    let placeholder = if dark { 0x938f99_u32 } else { 0x79747e };
    let filled_bg = if dark { 0x36343b_u32 } else { 0xe7e0ec };
    let primary = if dark { 0xd0bcff_u32 } else { 0x6750a4 };
    let error = if dark { 0xf2b8b5_u32 } else { 0xb3261e };

    div()
        .flex()
        .flex_col()
        .gap_3()
        // Outlined text field
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().text_color(rgb(primary)).child("Email"))
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(primary))
                        .text_base()
                        .text_color(rgb(on_surface))
                        .child("user@example.com"),
                ),
        )
        // Filled text field
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .px_4()
                        .pt_2()
                        .pb_3()
                        .rounded_tl(px(4.0))
                        .rounded_tr(px(4.0))
                        .bg(rgb(filled_bg))
                        .child(div().text_xs().text_color(rgb(primary)).child("Username"))
                        .child(
                            div()
                                .text_base()
                                .text_color(rgb(on_surface))
                                .child("john_doe"),
                        ),
                )
                .child(div().w_full().h(px(2.0)).bg(rgb(primary))),
        )
        // Error state
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().text_color(rgb(error)).child("Password"))
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(error))
                        .text_base()
                        .text_color(rgb(placeholder))
                        .child("••••••"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(error))
                        .child("⚠ Password must be at least 8 characters"),
                ),
        )
}

fn material_cards(dark: bool) -> impl IntoElement {
    let on_surface = if dark { 0xe6e1e5_u32 } else { 0x1c1b1f };
    let on_surface_variant = if dark { 0xcac4d0_u32 } else { 0x49454f };
    let primary = if dark { 0xd0bcff_u32 } else { 0x6750a4 };

    div()
        .flex()
        .flex_col()
        .gap_3()
        // Elevated card
        .child(
            material_surface(dark, 2).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(40.0))
                                    .rounded_full()
                                    .bg(rgb(primary))
                                    .text_color(rgb(if dark { 0x381e72 } else { 0xffffff }))
                                    .text_sm()
                                    .child("AB"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_base()
                                            .text_color(rgb(on_surface))
                                            .child("Elevated Card"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(on_surface_variant))
                                            .child("Material Design 3"),
                                    ),
                            ),
                    )
                    .child(div().text_sm().text_color(rgb(on_surface_variant)).child(
                        "Cards contain content and actions about a single subject. \
                                 Elevated cards have a drop shadow for visual hierarchy.",
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap_2()
                            .child(material_button_text("Cancel", primary))
                            .child(material_button_filled(
                                "Accept",
                                primary,
                                if dark { 0x381e72 } else { 0xffffff },
                            )),
                    ),
            ),
        )
        // Outlined card
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_4()
                .rounded(px(12.0))
                .border_1()
                .border_color(rgb(if dark { 0x49454f } else { 0xc4c0c9 }))
                .bg(rgb(if dark { 0x1c1b1f } else { 0xffffff }))
                .child(
                    div()
                        .text_base()
                        .text_color(rgb(on_surface))
                        .child("Outlined Card"),
                )
                .child(div().text_sm().text_color(rgb(on_surface_variant)).child(
                    "Outlined cards use a border instead of shadow. \
                             Best for grouping related content without heavy emphasis.",
                )),
        )
}

fn material_snackbar(dark: bool) -> impl IntoElement {
    let snack_bg = if dark { 0x332d41_u32 } else { 0x322f35 };
    let snack_text = if dark { 0xe6e1e5_u32 } else { 0xf4eff4 };
    let action_color = if dark { 0xd0bcff_u32 } else { 0xd0bcff };

    div()
        .flex()
        .flex_col()
        .gap_2()
        // Single-line snackbar
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_4()
                .py_3()
                .rounded(px(4.0))
                .bg(rgb(snack_bg))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(snack_text))
                        .child("File has been deleted"),
                )
                .child(div().text_sm().text_color(rgb(action_color)).child("UNDO")),
        )
        // Multi-line snackbar
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .px_4()
                .py_3()
                .rounded(px(4.0))
                .bg(rgb(snack_bg))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(snack_text))
                        .child("Connection lost. Changes will be synced when you're back online."),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap_2()
                        .child(div().text_sm().text_color(rgb(action_color)).child("RETRY"))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(action_color))
                                .child("DISMISS"),
                        ),
                ),
        )
}

fn material_bottom_sheet(dark: bool) -> impl IntoElement {
    let surface = if dark { 0x2b2930_u32 } else { 0xf3edf7 };
    let on_surface = if dark { 0xe6e1e5_u32 } else { 0x1c1b1f };
    let on_surface_variant = if dark { 0xcac4d0_u32 } else { 0x49454f };
    let drag_handle = if dark { 0x49454f_u32 } else { 0xc4c0c9 };

    div()
        .flex()
        .flex_col()
        .rounded_tl(px(28.0))
        .rounded_tr(px(28.0))
        .rounded_bl(px(12.0))
        .rounded_br(px(12.0))
        .bg(rgb(surface))
        .overflow_hidden()
        // Drag handle
        .child(
            div().flex().items_center().justify_center().py_3().child(
                div()
                    .w(px(32.0))
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .bg(rgb(drag_handle)),
            ),
        )
        // Title
        .child(
            div()
                .px_6()
                .pb_2()
                .text_base()
                .text_color(rgb(on_surface))
                .child("Share with…"),
        )
        // Sheet items
        .child(material_sheet_item(
            "📧",
            "Email",
            on_surface,
            on_surface_variant,
        ))
        .child(material_sheet_item(
            "💬",
            "Messages",
            on_surface,
            on_surface_variant,
        ))
        .child(material_sheet_item(
            "📋",
            "Copy Link",
            on_surface,
            on_surface_variant,
        ))
        .child(material_sheet_item(
            "📱",
            "AirDrop",
            on_surface,
            on_surface_variant,
        ))
        .child(div().h(px(8.0)))
}

fn material_sheet_item(
    icon: &str,
    label: &str,
    on_surface: u32,
    on_surface_variant: u32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_4()
        .px_6()
        .py_3()
        .child(
            div()
                .text_xl()
                .text_color(rgb(on_surface_variant))
                .child(icon.to_string()),
        )
        .child(
            div()
                .text_base()
                .text_color(rgb(on_surface))
                .child(label.to_string()),
        )
}

fn material_navigation_bar(dark: bool) -> impl IntoElement {
    let surface = if dark { 0x211f26_u32 } else { 0xf3edf7 };
    let active_indicator = if dark { 0x4a4458_u32 } else { 0xe8def8 };
    let active_color = if dark { 0xe6e1e5_u32 } else { 0x1c1b1f };
    let inactive_color = if dark { 0x938f99_u32 } else { 0x49454f };

    let nav_item = |icon: &str, label: &str, active: bool| -> gpui::Div {
        let fg = if active { active_color } else { inactive_color };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .flex_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_5()
                    .py(px(4.0))
                    .rounded(px(16.0))
                    .when(active, |d| d.bg(rgb(active_indicator)))
                    .text_xl()
                    .text_color(rgb(fg))
                    .child(icon.to_string()),
            )
            .child(div().text_xs().text_color(rgb(fg)).child(label.to_string()))
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .py_3()
        .rounded(px(12.0))
        .bg(rgb(surface))
        .child(nav_item("🏠", "Home", true))
        .child(nav_item("🔍", "Explore", false))
        .child(nav_item("📚", "Library", false))
        .child(nav_item("👤", "Profile", false))
}

// ═════════════════════════════════════════════════════════════════════════════
// SHARED PATTERNS
// ═════════════════════════════════════════════════════════════════════════════

fn shared_progress_bars(dark: bool) -> impl IntoElement {
    let card_bg = if dark { SURFACE0 } else { 0xe6e9ef };
    let text_color = if dark { TEXT } else { 0x4c4f69 };
    let sub_text: u32 = if dark { 0xa6adc8 } else { 0x6c6f85 };
    let track_color = if dark { SURFACE1 } else { 0xdce0e8 };

    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_4()
        .rounded(px(12.0))
        .bg(rgb(card_bg))
        .child(progress_row(
            "Storage",
            0.72,
            BLUE,
            track_color,
            text_color,
            sub_text,
        ))
        .child(progress_row(
            "Memory",
            0.45,
            GREEN,
            track_color,
            text_color,
            sub_text,
        ))
        .child(progress_row(
            "CPU",
            0.90,
            RED,
            track_color,
            text_color,
            sub_text,
        ))
        .child(progress_row(
            "Battery",
            0.58,
            YELLOW,
            track_color,
            text_color,
            sub_text,
        ))
}

fn progress_row(
    label: &str,
    progress: f32,
    color: u32,
    track: u32,
    text_color: u32,
    sub_text: u32,
) -> impl IntoElement {
    let pct = (progress * 100.0) as u32;
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(text_color))
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child(format!("{}%", pct)),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(rgb(track))
                .child(
                    div()
                        .h_full()
                        .rounded(px(3.0))
                        .bg(rgb(color))
                        .w(relative(progress)),
                ),
        )
}

fn shared_avatars(dark: bool) -> impl IntoElement {
    let card_bg = if dark { SURFACE0 } else { 0xe6e9ef };
    let sub_text: u32 = if dark { 0xa6adc8 } else { 0x6c6f85 };

    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded(px(12.0))
        .bg(rgb(card_bg))
        // Size variants
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(avatar("A", BLUE, MANTLE, px(48.0)))
                .child(avatar("BP", GREEN, MANTLE, px(40.0)))
                .child(avatar("ZD", MAUVE, MANTLE, px(36.0)))
                .child(avatar("M", PEACH, MANTLE, px(32.0)))
                .child(avatar("K", TEAL, MANTLE, px(28.0))),
        )
        // With status indicators
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .child(avatar_status("JD", BLUE, GREEN, "Online", sub_text))
                .child(avatar_status("AR", MAUVE, YELLOW, "Away", sub_text))
                .child(avatar_status("TX", PEACH, RED, "Busy", sub_text))
                .child(avatar_status("KL", TEAL, SURFACE1, "Offline", sub_text)),
        )
        // Stacked / group avatar
        .child(
            div()
                .flex()
                .flex_row()
                .child(avatar("A", BLUE, MANTLE, px(36.0)))
                .child(
                    div()
                        .ml(px(-10.0))
                        .child(avatar("B", GREEN, MANTLE, px(36.0))),
                )
                .child(
                    div()
                        .ml(px(-10.0))
                        .child(avatar("C", MAUVE, MANTLE, px(36.0))),
                )
                .child(
                    div()
                        .ml(px(-10.0))
                        .child(avatar("D", PEACH, MANTLE, px(36.0))),
                )
                .child(
                    div()
                        .ml(px(-10.0))
                        .child(avatar("+3", SURFACE1, TEXT, px(36.0))),
                ),
        )
}

fn avatar(initials: &str, bg: u32, fg: u32, size: gpui::Pixels) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(size)
        .rounded_full()
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .text_size(size * 0.4)
        .border_2()
        .border_color(rgb(MANTLE))
        .child(initials.to_string())
}

fn avatar_status(
    initials: &str,
    bg: u32,
    status_color: u32,
    status_label: &str,
    label_color: u32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .child(
            div()
                .relative()
                .child(avatar(initials, bg, MANTLE, px(40.0)))
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .right_0()
                        .size(px(12.0))
                        .rounded_full()
                        .bg(rgb(status_color))
                        .border_2()
                        .border_color(rgb(MANTLE)),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(label_color))
                .child(status_label.to_string()),
        )
}

fn shared_badges(dark: bool) -> impl IntoElement {
    let card_bg = if dark { SURFACE0 } else { 0xe6e9ef };

    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded(px(12.0))
        .bg(rgb(card_bg))
        // Solid badges
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(badge_solid("New", BLUE, MANTLE))
                .child(badge_solid("Live", GREEN, MANTLE))
                .child(badge_solid("Error", RED, MANTLE))
                .child(badge_solid("Beta", MAUVE, MANTLE))
                .child(badge_solid("3", PEACH, MANTLE)),
        )
        // Outline badges
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_2()
                .child(badge_outline("Draft", BLUE))
                .child(badge_outline("Active", GREEN))
                .child(badge_outline("Archived", YELLOW))
                .child(badge_outline("99+", RED)),
        )
        // Dot badges (notification indicator)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .child(icon_with_badge("🔔", RED, dark))
                .child(icon_with_badge("💬", BLUE, dark))
                .child(icon_with_badge("📧", GREEN, dark)),
        )
}

fn badge_solid(label: &str, bg: u32, fg: u32) -> impl IntoElement {
    div()
        .px_2()
        .py(px(2.0))
        .rounded(px(10.0))
        .bg(rgb(bg))
        .text_xs()
        .text_color(rgb(fg))
        .child(label.to_string())
}

fn badge_outline(label: &str, color: u32) -> impl IntoElement {
    div()
        .px_2()
        .py(px(2.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgb(color))
        .text_xs()
        .text_color(rgb(color))
        .child(label.to_string())
}

fn icon_with_badge(icon: &str, dot_color: u32, dark: bool) -> impl IntoElement {
    let icon_color: u32 = if dark { TEXT } else { 0x4c4f69 };

    div()
        .relative()
        .child(
            div()
                .text_xl()
                .text_color(rgb(icon_color))
                .child(icon.to_string()),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .mt(px(-2.0))
                .mr(px(-2.0))
                .size(px(10.0))
                .rounded_full()
                .bg(rgb(dot_color))
                .border_2()
                .border_color(rgb(MANTLE)),
        )
}

fn shared_stat_cards(dark: bool) -> impl IntoElement {
    let card_bg = if dark { SURFACE0 } else { 0xe6e9ef };
    let text_color = if dark { TEXT } else { 0x4c4f69 };
    let sub_text: u32 = if dark { 0xa6adc8 } else { 0x6c6f85 };

    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_row()
                .gap_3()
                .child(stat_card(
                    "Users", "1.2k", "↑ 12%", BLUE, GREEN, card_bg, text_color, sub_text,
                ))
                .child(stat_card(
                    "Revenue", "$4.8k", "↑ 8%", MAUVE, GREEN, card_bg, text_color, sub_text,
                )),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap_3()
                .child(stat_card(
                    "Orders", "328", "↓ 3%", PEACH, RED, card_bg, text_color, sub_text,
                ))
                .child(stat_card(
                    "Rating",
                    "4.9",
                    "★★★★★",
                    TEAL,
                    YELLOW,
                    card_bg,
                    text_color,
                    sub_text,
                )),
        )
}

fn stat_card(
    title: &str,
    value: &str,
    trend: &str,
    accent: u32,
    trend_color: u32,
    card_bg: u32,
    text_color: u32,
    sub_text: u32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_2()
        .p_4()
        .rounded_xl()
        .bg(rgb(card_bg))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().size(px(8.0)).rounded_full().bg(rgb(accent)))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .text_xl()
                .text_color(rgb(text_color))
                .child(value.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(trend_color))
                .child(trend.to_string()),
        )
}

fn shared_skeleton_loaders(dark: bool) -> impl IntoElement {
    let card_bg = if dark { SURFACE0 } else { 0xe6e9ef };
    let bone = if dark { SURFACE1 } else { 0xdce0e8 };

    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded(px(12.0))
        .bg(rgb(card_bg))
        // Card skeleton
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(div().size(px(40.0)).rounded_full().bg(rgb(bone)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .gap_2()
                        .child(
                            div()
                                .h(px(14.0))
                                .w(relative(0.6))
                                .rounded(px(4.0))
                                .bg(rgb(bone)),
                        )
                        .child(
                            div()
                                .h(px(10.0))
                                .w(relative(0.4))
                                .rounded(px(4.0))
                                .bg(rgb(bone)),
                        ),
                ),
        )
        // Text block skeleton
        .child(div().h(px(12.0)).w_full().rounded(px(4.0)).bg(rgb(bone)))
        .child(div().h(px(12.0)).w_full().rounded(px(4.0)).bg(rgb(bone)))
        .child(
            div()
                .h(px(12.0))
                .w(relative(0.75))
                .rounded(px(4.0))
                .bg(rgb(bone)),
        )
        // Image + text skeleton
        .child(
            div()
                .flex()
                .flex_row()
                .gap_3()
                .mt_2()
                .child(div().size(px(80.0)).rounded(px(8.0)).bg(rgb(bone)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .gap_2()
                        .justify_center()
                        .child(
                            div()
                                .h(px(14.0))
                                .w(relative(0.8))
                                .rounded(px(4.0))
                                .bg(rgb(bone)),
                        )
                        .child(
                            div()
                                .h(px(10.0))
                                .w(relative(0.5))
                                .rounded(px(4.0))
                                .bg(rgb(bone)),
                        )
                        .child(
                            div()
                                .h(px(10.0))
                                .w(relative(0.65))
                                .rounded(px(4.0))
                                .bg(rgb(bone)),
                        ),
                ),
        )
}
