//! Counter screen — increment, decrement, and reset a shared tap counter.
//!
//! This screen demonstrates interactive buttons that mutate shared state on the
//! `Router` and trigger re-renders via `cx.notify()`.

use gpui::{div, prelude::*, rgb, App, MouseDownEvent, Window};

use super::{Router, BLUE, GREEN, MANTLE, MAUVE, PEACH, RED, SURFACE0, SURFACE1, TEXT, YELLOW};

/// Render the Counter screen content area.
///
/// Takes a mutable reference to the `Router` (for `cx.listener` closures that
/// mutate `tap_count`) and the GPUI context.
pub fn render(router: &Router, cx: &mut gpui::Context<Router>) -> impl IntoElement {
    let tap_count = router.tap_count;
    let dark_mode = router.dark_mode;
    let text_color = if dark_mode { TEXT } else { 0x4c4f69 };
    let sub_text = if dark_mode { 0xa6adc8_u32 } else { 0x6c6f85 };
    let card_bg = if dark_mode { SURFACE0 } else { 0xe6e9ef };

    // Determine the accent colour based on the count value.
    let count_color = match tap_count {
        0 => sub_text,
        1..=10 => BLUE,
        11..=50 => GREEN,
        51..=100 => YELLOW,
        101..=500 => PEACH,
        _ => RED,
    };

    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_6()
        .px_4()
        .py_6()
        .items_center()
        .justify_center()
        // ── Counter display ──────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(sub_text))
                        .child("Current count"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_32()
                        .rounded_full()
                        .bg(rgb(card_bg))
                        .border_2()
                        .border_color(rgb(count_color))
                        .child(
                            div()
                                .text_3xl()
                                .text_color(rgb(count_color))
                                .child(format!("{}", tap_count)),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child(count_label(tap_count)),
                ),
        )
        // ── Increment / Decrement row ────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_row()
                .gap_3()
                .items_center()
                // Decrement button
                .child(counter_button(
                    "-",
                    RED,
                    dark_mode,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count = this.tap_count.saturating_sub(1);
                        cx.notify();
                    }),
                ))
                // Increment button (large / primary)
                .child(primary_button(
                    "+1",
                    BLUE,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count += 1;
                        cx.notify();
                    }),
                ))
                // +5 button
                .child(counter_button(
                    "+5",
                    GREEN,
                    dark_mode,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count += 5;
                        cx.notify();
                    }),
                )),
        )
        // ── Quick-add row ────────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(pill_button(
                    "+10",
                    MAUVE,
                    dark_mode,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count += 10;
                        cx.notify();
                    }),
                ))
                .child(pill_button(
                    "+50",
                    PEACH,
                    dark_mode,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count += 50;
                        cx.notify();
                    }),
                ))
                .child(pill_button(
                    "+100",
                    YELLOW,
                    dark_mode,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count += 100;
                        cx.notify();
                    }),
                )),
        )
        // ── Reset button ─────────────────────────────────────────────────
        .child(
            div()
                .mt_4()
                .px_8()
                .py_3()
                .rounded_lg()
                .bg(rgb(SURFACE1))
                .text_color(rgb(sub_text))
                .text_sm()
                .child("Reset to zero")
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.tap_count = 0;
                        cx.notify();
                    }),
                ),
        )
        // ── Milestone indicator ──────────────────────────────────────────
        .child(milestone_bar(tap_count, text_color, sub_text, card_bg))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// A descriptive label for the current count.
fn count_label(count: u32) -> String {
    match count {
        0 => "Tap a button to start!".to_string(),
        1 => "1 tap so far".to_string(),
        n if n < 10 => format!("{} taps — keep going!", n),
        n if n < 50 => format!("{} taps — nice!", n),
        n if n < 100 => format!("{} taps — impressive!", n),
        n if n < 500 => format!("{} taps — unstoppable!", n),
        n => format!("{} taps — legendary!", n),
    }
}

/// A circular button for increment/decrement.
fn counter_button(
    label: &str,
    color: u32,
    dark_mode: bool,
    handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let bg = if dark_mode { SURFACE0 } else { 0xe6e9ef };

    div()
        .flex()
        .items_center()
        .justify_center()
        .size_12()
        .rounded_full()
        .bg(rgb(bg))
        .border_2()
        .border_color(rgb(color))
        .text_xl()
        .text_color(rgb(color))
        .child(label.to_string())
        .on_mouse_down(gpui::MouseButton::Left, handler)
}

/// A large primary action button.
fn primary_button(
    label: &str,
    color: u32,
    handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size_20()
        .rounded_full()
        .bg(rgb(color))
        .text_2xl()
        .text_color(rgb(MANTLE))
        .child(label.to_string())
        .on_mouse_down(gpui::MouseButton::Left, handler)
}

/// A small pill-shaped button for quick-add values.
fn pill_button(
    label: &str,
    color: u32,
    dark_mode: bool,
    handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let bg = if dark_mode { SURFACE0 } else { 0xe6e9ef };

    div()
        .px_5()
        .py_2()
        .rounded_full()
        .bg(rgb(bg))
        .border_1()
        .border_color(rgb(color))
        .text_sm()
        .text_color(rgb(color))
        .child(label.to_string())
        .on_mouse_down(gpui::MouseButton::Left, handler)
}

/// A horizontal milestone progress bar.
fn milestone_bar(count: u32, text_color: u32, sub_text: u32, card_bg: u32) -> impl IntoElement {
    let milestones: &[(u32, &str)] = &[(10, "10"), (50, "50"), (100, "100"), (500, "500")];

    let next_milestone = milestones
        .iter()
        .find(|(m, _)| count < *m)
        .map(|(m, label)| (*m, *label));

    let progress_text = if let Some((target, label)) = next_milestone {
        format!("{}/{} to next milestone ({})", count, target, label)
    } else {
        "All milestones reached! 🎉".to_string()
    };

    div()
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .px_2()
        .child(
            div()
                .text_xs()
                .text_color(rgb(sub_text))
                .child(progress_text),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap_1()
                .child(milestone_dot(10, count, card_bg, text_color))
                .child(milestone_dot(50, count, card_bg, text_color))
                .child(milestone_dot(100, count, card_bg, text_color))
                .child(milestone_dot(500, count, card_bg, text_color)),
        )
}

/// A single milestone dot — filled if the count has reached it.
fn milestone_dot(target: u32, current: u32, bg: u32, _text_color: u32) -> impl IntoElement {
    let reached = current >= target;
    let dot_color = if reached { GREEN } else { bg };
    let label_color = if reached { MANTLE } else { 0x6c7086 };

    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .py_1()
        .rounded_md()
        .bg(rgb(dot_color))
        .text_xs()
        .text_color(rgb(label_color))
        .child(format!("{}", target))
}
