//! Form example screen — demonstrates Material Design 3 input components
//! composed into a realistic form layout with interactive state.

use gpui::{div, prelude::*, px, rgb, Context, MouseButton, MouseMoveEvent, MouseUpEvent};
use gpui_mobile::components::material::{
    Card, Checkbox, CircularProgressIndicator, FilledButton, MaterialTheme, OutlinedButton, Radio,
    RadioGroup, Slider, Switch, TextButton, TextInput,
};
use gpui_mobile::KeyboardType;
use std::cell::RefCell;

use super::Router;

thread_local! {
    /// Pending text from the software keyboard, accumulated between frames.
    /// Each entry is a string fragment (or "\x08" for backspace).
    static PENDING_TEXT: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

/// Install the keyboard callback that pushes typed text into PENDING_TEXT.
fn install_keyboard_callback() {
    gpui_mobile::set_text_input_callback(Some(Box::new(|text: &str| {
        PENDING_TEXT.with(|pending| {
            pending.borrow_mut().push(text.to_string());
        });
    })));
}

/// Drain pending keyboard text and apply it to the Router's focused field.
pub fn drain_pending_text(router: &mut Router) {
    PENDING_TEXT.with(|pending| {
        let texts: Vec<String> = pending.borrow_mut().drain(..).collect();
        for text in texts {
            let field = match router.form.focused_field {
                Some(0) => &mut router.form.full_name,
                Some(1) => &mut router.form.email,
                Some(2) => &mut router.form.phone,
                _ => continue,
            };
            if text == "\x08" {
                // Backspace
                field.pop();
            } else {
                field.push_str(&text);
            }
        }
    });
}

/// Render the Material Form example screen with interactive controls.
pub fn render(router: &mut Router, cx: &mut Context<Router>) -> impl IntoElement {
    // Drain any pending keyboard input into the focused field.
    drain_pending_text(router);

    let dark = router.dark_mode;
    let theme = MaterialTheme::from_appearance(dark);
    let sub_text: u32 = if dark { 0xa6adc8 } else { 0x6c6f85 };
    let form = &router.form;
    let pull_distance = router.pull_distance;
    let refreshing = router.refreshing;

    div()
        .id("form-pull-container")
        .flex()
        .flex_col()
        .flex_1()
        .gap_4()
        .px_4()
        .py_6()
        .on_mouse_move(
            cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                // Pull-to-refresh: track downward finger movement.
                // on_mouse_move fires during drag gestures with pressed_button set.
                if event.pressed_button.is_some()
                    && this.current_screen == super::Screen::Form
                    && !this.refreshing
                {
                    let y = event.position.y.as_f32();
                    if let Some(start_y) = this.pull_start_y {
                        let delta = y - start_y;
                        if delta > 0.0 {
                            this.pull_distance = (delta * 0.4).min(120.0);
                            cx.notify();
                        }
                    } else {
                        // First move of a new drag — record start position
                        this.pull_start_y = Some(y);
                    }
                }
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                if this.pull_distance > 60.0 {
                    // Trigger refresh
                    this.refreshing = true;
                    this.form = super::FormState::default();
                    this.pull_distance = 0.0;
                    cx.notify();
                    // Clear refreshing state (no timer — immediate for now)
                    this.refreshing = false;
                }
                this.pull_start_y = None;
                this.pull_distance = 0.0;
                cx.notify();
            }),
        )
        // ── Pull-to-refresh indicator ──────────────────────────────────
        .when(pull_distance > 10.0 || refreshing, |d| {
            let indicator_opacity = if refreshing { 1.0 } else { (pull_distance / 80.0).min(1.0) };
            let indicator_scale = if refreshing { 1.0 } else { (pull_distance / 80.0).min(1.0) };
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .h(px(pull_distance.max(if refreshing { 60.0 } else { 0.0 })))
                    .opacity(indicator_opacity)
                    .child(
                        CircularProgressIndicator::new(theme)
                            .progress(indicator_scale)
                            .diameter(28.0)
                            .stroke_width(3.0),
                    )
                    .when(pull_distance > 80.0, |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(rgb(sub_text))
                                .mt_1()
                                .child("Release to refresh"),
                        )
                    }),
            )
        })
        // ── Section: Personal Info ───────────────────────────────────────
        .child(section_header("Personal Information", sub_text))
        .child(
            Card::outlined(theme)
                .full_width()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .p_4()
                        .child(
                            TextInput::<Router>::new("input-name", theme)
                                .label("Full Name")
                                .value(&form.full_name)
                                .placeholder("Enter your name")
                                .keyboard_type(KeyboardType::Default)
                                .focused(form.focused_field == Some(0))
                                .on_tap(|this, _, _, cx| {
                                    this.form.focused_field = Some(0);
                                    install_keyboard_callback();
                                    gpui_mobile::show_keyboard_with_type(KeyboardType::Default);
                                    cx.notify();
                                })
                                .render(cx),
                        )
                        .child(
                            TextInput::<Router>::new("input-email", theme)
                                .label("Email")
                                .value(&form.email)
                                .placeholder("user@example.com")
                                .keyboard_type(KeyboardType::EmailAddress)
                                .focused(form.focused_field == Some(1))
                                .on_tap(|this, _, _, cx| {
                                    this.form.focused_field = Some(1);
                                    install_keyboard_callback();
                                    gpui_mobile::show_keyboard_with_type(KeyboardType::EmailAddress);
                                    cx.notify();
                                })
                                .render(cx),
                        )
                        .child(
                            TextInput::<Router>::new("input-phone", theme)
                                .label("Phone")
                                .value(&form.phone)
                                .placeholder("+1 (555) 000-0000")
                                .keyboard_type(KeyboardType::Phone)
                                .focused(form.focused_field == Some(2))
                                .on_tap(|this, _, _, cx| {
                                    this.form.focused_field = Some(2);
                                    install_keyboard_callback();
                                    gpui_mobile::show_keyboard_with_type(KeyboardType::Phone);
                                    cx.notify();
                                })
                                .render(cx),
                        ),
                ),
        )
        // ── Section: Preferences ─────────────────────────────────────────
        .child(section_header("Preferences", sub_text))
        .child(
            Card::outlined(theme)
                .full_width()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_4()
                        .child(
                            Switch::new(theme)
                                .on(form.notifications)
                                .label("Enable notifications")
                                .on_toggle(cx.listener(|this, _, _, cx| {
                                    this.form.notifications = !this.form.notifications;
                                    cx.notify();
                                }))
                                .id("notif-switch"),
                        )
                        .child(div().h(px(1.0)).mx_4().bg(rgb(theme.outline_variant)))
                        .child(
                            Switch::new(theme)
                                .on(router.dark_mode)
                                .label("Dark mode")
                                .on_toggle(cx.listener(|this, _, _, cx| {
                                    this.dark_mode = !this.dark_mode;
                                    cx.notify();
                                }))
                                .id("dark-switch"),
                        )
                        .child(div().h(px(1.0)).mx_4().bg(rgb(theme.outline_variant)))
                        .child(
                            Switch::new(theme)
                                .on(form.auto_update)
                                .label("Auto-update")
                                .with_icons()
                                .on_toggle(cx.listener(|this, _, _, cx| {
                                    this.form.auto_update = !this.form.auto_update;
                                    cx.notify();
                                }))
                                .id("update-switch"),
                        ),
                ),
        )
        // ── Section: Account Type ────────────────────────────────────────
        .child(section_header("Account Type", sub_text))
        .child(
            Card::outlined(theme)
                .full_width()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_4()
                        .child(
                            RadioGroup::new(theme)
                                .option(
                                    "Personal",
                                    form.account_type == 0,
                                    cx.listener(|this, _, _, cx| {
                                        this.form.account_type = 0;
                                        cx.notify();
                                    }),
                                )
                                .option(
                                    "Business",
                                    form.account_type == 1,
                                    cx.listener(|this, _, _, cx| {
                                        this.form.account_type = 1;
                                        cx.notify();
                                    }),
                                )
                                .option(
                                    "Education",
                                    form.account_type == 2,
                                    cx.listener(|this, _, _, cx| {
                                        this.form.account_type = 2;
                                        cx.notify();
                                    }),
                                ),
                        ),
                ),
        )
        // ── Section: Interests ───────────────────────────────────────────
        .child(section_header("Interests", sub_text))
        .child(
            Card::outlined(theme)
                .full_width()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_4()
                        .child(
                            Checkbox::new(theme)
                                .checked(form.interests[0])
                                .label("Technology")
                                .on_toggle(cx.listener(|this, _, _, cx| {
                                    this.form.interests[0] = !this.form.interests[0];
                                    cx.notify();
                                }))
                                .id("cb-tech"),
                        )
                        .child(
                            Checkbox::new(theme)
                                .checked(form.interests[1])
                                .label("Design")
                                .on_toggle(cx.listener(|this, _, _, cx| {
                                    this.form.interests[1] = !this.form.interests[1];
                                    cx.notify();
                                }))
                                .id("cb-design"),
                        )
                        .child(
                            Checkbox::new(theme)
                                .checked(form.interests[2])
                                .label("Science")
                                .on_toggle(cx.listener(|this, _, _, cx| {
                                    this.form.interests[2] = !this.form.interests[2];
                                    cx.notify();
                                }))
                                .id("cb-science"),
                        )
                        .child(
                            Checkbox::new(theme)
                                .checked(form.interests[3])
                                .label("Music")
                                .on_toggle(cx.listener(|this, _, _, cx| {
                                    this.form.interests[3] = !this.form.interests[3];
                                    cx.notify();
                                }))
                                .id("cb-music"),
                        ),
                ),
        )
        // ── Section: Experience Level ────────────────────────────────────
        .child(section_header("Experience Level", sub_text))
        .child(
            Card::outlined(theme)
                .full_width()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_4()
                        .child(
                            Slider::new(theme)
                                .value(form.skill_level)
                                .label("Skill level")
                                .show_value_label(true)
                                .range_labels("Beginner", "Expert")
                                .id("skill-slider"),
                        )
                        .child(
                            Slider::new(theme)
                                .value(form.experience)
                                .label("Years of experience")
                                .steps(10)
                                .show_value_label(true)
                                .range_labels("0", "10+")
                                .id("exp-slider"),
                        ),
                ),
        )
        // ── Section: Terms ───────────────────────────────────────────────
        .child(
            Card::outlined(theme)
                .full_width()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_4()
                        .child(
                            Checkbox::new(theme)
                                .checked(form.terms_accepted)
                                .label("I agree to the Terms of Service")
                                .on_toggle(cx.listener(|this, _, _, cx| {
                                    this.form.terms_accepted = !this.form.terms_accepted;
                                    cx.notify();
                                }))
                                .id("cb-terms"),
                        )
                        .child(
                            Checkbox::new(theme)
                                .checked(form.newsletter)
                                .label("Subscribe to newsletter")
                                .on_toggle(cx.listener(|this, _, _, cx| {
                                    this.form.newsletter = !this.form.newsletter;
                                    cx.notify();
                                }))
                                .id("cb-newsletter"),
                        ),
                ),
        )
        // ── Action buttons ───────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_row()
                .gap_3()
                .justify_end()
                .child(TextButton::new("Cancel", theme).id("btn-cancel"))
                .child(OutlinedButton::new("Save Draft", theme).id("btn-draft"))
                .child(FilledButton::new("Submit", theme).id("btn-submit")),
        )
        // ── Disabled state examples ──────────────────────────────────────
        .child(section_header("Disabled States", sub_text))
        .child(
            Card::outlined(theme)
                .full_width()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_4()
                        .child(
                            Checkbox::new(theme)
                                .checked(true)
                                .label("Disabled checked")
                                .disabled(true)
                                .id("cb-disabled"),
                        )
                        .child(
                            Switch::new(theme)
                                .on(true)
                                .label("Disabled switch")
                                .disabled(true)
                                .id("sw-disabled"),
                        )
                        .child(
                            Radio::new(theme)
                                .selected(true)
                                .label("Disabled radio")
                                .disabled(true)
                                .id("radio-disabled"),
                        )
                        .child(
                            Slider::new(theme)
                                .value(0.5)
                                .label("Disabled slider")
                                .disabled(true)
                                .id("slider-disabled"),
                        ),
                ),
        )
        // ── Validation states ────────────────────────────────────────────
        .child(section_header("Validation States", sub_text))
        .child(
            Card::outlined(theme)
                .full_width()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_4()
                        .child(
                            TextInput::<Router>::new("input-username-err", theme)
                                .label("Username")
                                .value("ab")
                                .error(true)
                                .error_text("Username must be at least 3 characters")
                                .render(cx),
                        )
                        .child(
                            Checkbox::new(theme)
                                .checked(false)
                                .label("Accept terms (required)")
                                .error(true)
                                .id("cb-error"),
                        ),
                ),
        )
        // ── Footer ───────────────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .py_6()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(sub_text))
                        .child("Form built with Material Design 3 components"),
                ),
        )
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn section_header(label: &str, color: u32) -> impl IntoElement {
    div()
        .text_sm()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(color))
        .child(label.to_string())
}

