//! About screen — app info, technology stack, and credits.
//!
//! This screen is purely informational — it reads shared state from the
//! `Router` but does not mutate it.

use gpui::{div, prelude::*, px, rgb};

use super::{Router, BLUE, GREEN, MANTLE, MAUVE, PEACH, SURFACE0, SURFACE1, TEAL, TEXT, YELLOW};

/// Render the About screen content area.
///
/// This is a pure layout function — it reads shared state from the `Router` but
/// does not mutate it.
pub fn render(router: &Router) -> impl IntoElement {
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
        // ── App icon / banner ────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .py_4()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_20()
                        .rounded_xl()
                        .bg(rgb(BLUE))
                        .text_3xl()
                        .text_color(rgb(MANTLE))
                        .child("G"),
                )
                .child(
                    div()
                        .text_xl()
                        .text_color(rgb(text_color))
                        .child("GPUI Android Example"),
                )
                .child(div().text_sm().text_color(rgb(sub_text)).child("v0.1.0")),
        )
        // ── Description card ─────────────────────────────────────────────
        .child(
            info_card(card_bg).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .child(div().text_sm().text_color(rgb(text_color)).child(
                        "A multi-screen demo app built with GPUI, \
                                 Zed's GPU-accelerated UI framework, running \
                                 natively on Android via Vulkan.",
                    ))
                    .child(div().text_xs().text_color(rgb(sub_text)).child(
                        "This example showcases navigation, interactive \
                                 controls, shared state, and theming — all rendered \
                                 at 60 fps with wgpu.",
                    )),
            ),
        )
        // ── Technology stack ─────────────────────────────────────────────
        .child(section_header("Technology Stack", sub_text))
        .child(
            info_card(card_bg)
                .child(tech_row(
                    "🦀",
                    "Rust",
                    "Systems programming language",
                    PEACH,
                    text_color,
                    sub_text,
                ))
                .child(divider_line(divider_color))
                .child(tech_row(
                    "🖼",
                    "GPUI",
                    "Zed's GPU-accelerated UI framework",
                    BLUE,
                    text_color,
                    sub_text,
                ))
                .child(divider_line(divider_color))
                .child(tech_row(
                    "🔺",
                    "wgpu",
                    "Cross-platform graphics API (Vulkan)",
                    GREEN,
                    text_color,
                    sub_text,
                ))
                .child(divider_line(divider_color))
                .child(tech_row(
                    "📱",
                    "android-activity",
                    "Native Android lifecycle",
                    MAUVE,
                    text_color,
                    sub_text,
                ))
                .child(divider_line(divider_color))
                .child(tech_row(
                    "🎨",
                    "Catppuccin",
                    "Soothing pastel colour theme",
                    TEAL,
                    text_color,
                    sub_text,
                )),
        )
        // ── Architecture ─────────────────────────────────────────────────
        .child(section_header("Architecture", sub_text))
        .child(
            info_card(card_bg).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .child(architecture_row(
                        "Entry Point",
                        "android_main()",
                        BLUE,
                        text_color,
                        sub_text,
                    ))
                    .child(architecture_row(
                        "Platform",
                        "AndroidPlatform (Arc)",
                        MAUVE,
                        text_color,
                        sub_text,
                    ))
                    .child(architecture_row(
                        "Renderer",
                        "WgpuRenderer (Vulkan)",
                        GREEN,
                        text_color,
                        sub_text,
                    ))
                    .child(architecture_row(
                        "UI Layer",
                        "GPUI Views + Flexbox",
                        PEACH,
                        text_color,
                        sub_text,
                    ))
                    .child(architecture_row(
                        "Input",
                        "Touch → MouseDown events",
                        YELLOW,
                        text_color,
                        sub_text,
                    ))
                    .child(architecture_row(
                        "Navigation",
                        "Router + Screen enum",
                        TEAL,
                        text_color,
                        sub_text,
                    )),
            ),
        )
        // ── Features in this demo ────────────────────────────────────────
        .child(section_header("Features", sub_text))
        .child(
            info_card(card_bg)
                .child(feature_row(
                    "✅",
                    "Multi-screen navigation with history",
                    GREEN,
                    text_color,
                ))
                .child(divider_line(divider_color))
                .child(feature_row(
                    "✅",
                    "Bottom tab bar with active indicator",
                    GREEN,
                    text_color,
                ))
                .child(divider_line(divider_color))
                .child(feature_row(
                    "✅",
                    "Back button with navigation stack",
                    GREEN,
                    text_color,
                ))
                .child(divider_line(divider_color))
                .child(feature_row(
                    "✅",
                    "Shared state across screens",
                    GREEN,
                    text_color,
                ))
                .child(divider_line(divider_color))
                .child(feature_row(
                    "✅",
                    "Dark / light mode toggle",
                    GREEN,
                    text_color,
                ))
                .child(divider_line(divider_color))
                .child(feature_row(
                    "✅",
                    "Interactive counter with milestones",
                    GREEN,
                    text_color,
                ))
                .child(divider_line(divider_color))
                .child(feature_row(
                    "✅",
                    "Touch input via GPUI event handlers",
                    GREEN,
                    text_color,
                )),
        )
        // ── Credits ──────────────────────────────────────────────────────
        .child(section_header("Credits", sub_text))
        .child(
            info_card(card_bg).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .child(credit_row(
                        "GPUI Framework",
                        "Zed Industries",
                        BLUE,
                        text_color,
                        sub_text,
                    ))
                    .child(credit_row(
                        "Colour Theme",
                        "Catppuccin",
                        MAUVE,
                        text_color,
                        sub_text,
                    ))
                    .child(credit_row(
                        "Graphics Backend",
                        "wgpu contributors",
                        GREEN,
                        text_color,
                        sub_text,
                    ))
                    .child(credit_row(
                        "Android Glue",
                        "rust-mobile",
                        PEACH,
                        text_color,
                        sub_text,
                    )),
            ),
        )
        // ── Footer ───────────────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .py_4()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child("Built with ❤️ in Rust"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child("MIT / Apache-2.0"),
                ),
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

/// A rounded card container.
fn info_card(bg: u32) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .rounded_xl()
        .bg(rgb(bg))
        .overflow_hidden()
}

/// A horizontal divider line.
fn divider_line(color: u32) -> impl IntoElement {
    div().w_full().h(px(1.0)).bg(rgb(color)).mx_3()
}

/// A row showing a technology with icon, name, and description.
fn tech_row(
    icon: &str,
    name: &str,
    description: &str,
    accent: u32,
    text_color: u32,
    sub_text: u32,
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
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child(description.to_string()),
                ),
        )
        .child(div().size_2().rounded_full().bg(rgb(accent)))
}

/// A row showing an architecture layer.
fn architecture_row(
    layer: &str,
    detail: &str,
    accent: u32,
    text_color: u32,
    sub_text: u32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().size_2().rounded_full().bg(rgb(accent)))
        .child(
            div()
                .text_sm()
                .text_color(rgb(text_color))
                .child(layer.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(sub_text))
                .child(format!("— {}", detail)),
        )
}

/// A row showing a feature with a check mark.
fn feature_row(icon: &str, description: &str, accent: u32, text_color: u32) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_4()
        .py_2()
        .child(
            div()
                .text_sm()
                .text_color(rgb(accent))
                .child(icon.to_string()),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(text_color))
                .child(description.to_string()),
        )
}

/// A row showing a credit entry.
fn credit_row(
    project: &str,
    author: &str,
    accent: u32,
    text_color: u32,
    sub_text: u32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().size_2().rounded_full().bg(rgb(accent)))
        .child(
            div()
                .text_sm()
                .text_color(rgb(text_color))
                .child(project.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(sub_text))
                .child(format!("by {}", author)),
        )
}
