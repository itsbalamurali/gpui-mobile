//! Components showcase screen — Apple Glass & Material Design.
//!
//! This screen delegates to the component library in
//! `gpui_mobile::components` — the glass, material, and shared modules
//! contain all the actual component implementations. This file just
//! composes them into a single scrollable showcase layout.

use gpui::{div, prelude::*, rgb};

use gpui_mobile::components::{
    common::{design_language_header, section_label},
    glass, material, shared,
};

use super::{Router, BLUE, GREEN, MAUVE};

// ── Public render entry point ────────────────────────────────────────────────

/// Render the Components showcase screen.
pub fn render(router: &Router) -> impl IntoElement {
    let dark = router.dark_mode;
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
        .child(glass::hero_card(dark))
        // ── Glass action buttons ─────────────────────────────────────────
        .child(section_label("Buttons", sub_text))
        .child(glass::buttons_row(dark))
        // ── Glass segmented control ──────────────────────────────────────
        .child(section_label("Segmented Control", sub_text))
        .child(glass::segmented_control(dark))
        // ── Glass list / settings ────────────────────────────────────────
        .child(section_label("Settings List", sub_text))
        .child(glass::settings_list(dark))
        // ── Glass notification banners ───────────────────────────────────
        .child(section_label("Notification Banners", sub_text))
        .child(glass::notification_banners(dark))
        // ── Glass search bar ─────────────────────────────────────────────
        .child(section_label("Search Bar", sub_text))
        .child(glass::search_bar(dark))
        // ── Glass slider ─────────────────────────────────────────────────
        .child(section_label("Sliders", sub_text))
        .child(glass::sliders(dark))
        // ── Glass tab bar ────────────────────────────────────────────────
        .child(section_label("Tab Bar", sub_text))
        .child(glass::tab_bar(dark))
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
        .child(material::hero_card(dark))
        // ── Material buttons ─────────────────────────────────────────────
        .child(section_label("Buttons", sub_text))
        .child(material::buttons(dark))
        // ── Material FABs ────────────────────────────────────────────────
        .child(section_label("Floating Action Buttons", sub_text))
        .child(material::fabs(dark))
        // ── Material chips ───────────────────────────────────────────────
        .child(section_label("Chips", sub_text))
        .child(material::chips(dark))
        // ── Material text fields ─────────────────────────────────────────
        .child(section_label("Text Fields", sub_text))
        .child(material::text_fields(dark))
        // ── Material cards ───────────────────────────────────────────────
        .child(section_label("Cards", sub_text))
        .child(material::cards(dark))
        // ── Material snackbar ────────────────────────────────────────────
        .child(section_label("Snackbar", sub_text))
        .child(material::snackbar(dark))
        // ── Material bottom sheet ────────────────────────────────────────
        .child(section_label("Bottom Sheet", sub_text))
        .child(material::bottom_sheet(dark))
        // ── Material navigation bar ──────────────────────────────────────
        .child(section_label("Navigation Bar", sub_text))
        .child(material::navigation_bar_demo(dark))
        // ── Material dialogs ─────────────────────────────────────────────
        .child(section_label("Dialogs", sub_text))
        .child(material::demos::dialog_demo(dark))
        // ── Material progress indicators ─────────────────────────────────
        .child(section_label("Progress Indicators (Material)", sub_text))
        .child(material::demos::progress_indicator_demo(dark))
        // ── Material search bar ──────────────────────────────────────────
        .child(section_label("Search Bar (Material)", sub_text))
        .child(material::demos::search_bar_demo(dark))
        // ── Material menus ───────────────────────────────────────────────
        .child(section_label("Menus", sub_text))
        .child(material::demos::menu_demo(dark))
        // ── Material app bars ────────────────────────────────────────────
        .child(section_label("App Bars", sub_text))
        .child(material::demos::app_bar_demo(dark))
        // ── Material controls (checkbox, radio, switch, slider) ──────────
        .child(section_label("Form Controls", sub_text))
        .child(material::demos::controls_demo(dark))
        // ── Material list tiles ──────────────────────────────────────────
        .child(section_label("List Tiles & Extras", sub_text))
        .child(material::demos::list_tile_demo(dark))
        // ── Material tab bar ─────────────────────────────────────────────
        .child(section_label("Tab Bar", sub_text))
        .child(material::demos::tab_bar_demo(dark))
        // ── Material navigation rail ─────────────────────────────────────
        .child(section_label("Navigation Rail", sub_text))
        .child(material::demos::navigation_rail_demo(dark))
        // ── Material navigation drawer ───────────────────────────────────
        .child(section_label("Navigation Drawer", sub_text))
        .child(material::demos::navigation_drawer_demo(dark))
        // ── Material scaffold ────────────────────────────────────────────
        .child(section_label("Scaffold", sub_text))
        .child(material::demos::scaffold_demo(dark))
        // ── Material builder buttons ─────────────────────────────────────
        .child(section_label("Buttons (Builder API)", sub_text))
        .child(material::demos::button_demo(dark))
        // ── Material builder FABs ────────────────────────────────────────
        .child(section_label("FABs (Builder API)", sub_text))
        .child(material::demos::fab_demo(dark))
        // ── Material builder cards ───────────────────────────────────────
        .child(section_label("Cards (Builder API)", sub_text))
        .child(material::demos::card_demo(dark))
        // ── Shared components ────────────────────────────────────────────
        .child(div().mt_4().child(design_language_header(
            "Shared Patterns",
            "Progress · Avatars · Badges · Stats",
            MAUVE,
            sub_text,
        )))
        .child(section_label("Progress Indicators", sub_text))
        .child(shared::progress_bars(dark))
        .child(section_label("Avatars", sub_text))
        .child(shared::avatars(dark))
        .child(section_label("Badges", sub_text))
        .child(shared::badges(dark))
        .child(section_label("Stat Cards", sub_text))
        .child(shared::stat_cards(dark))
        .child(section_label("Skeleton Loaders", sub_text))
        .child(shared::skeleton_loaders(dark))
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
