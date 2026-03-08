//! Form example screen — demonstrates Material Design 3 input components
//! composed into a realistic form layout with interactive state.

use gpui::{div, prelude::*, px, rgb, Context, MouseDownEvent};
use gpui_mobile::components::material::{
    Card, Checkbox, CircularProgressIndicator, FilledButton, MaterialTheme, OutlinedButton, Radio,
    RadioGroup, Slider, Switch, TextButton, TextInput,
};
use gpui_mobile::KeyboardType;
use std::cell::RefCell;

use super::Router;

thread_local! {
    /// Pending text from the software keyboard, accumulated between frames.
    /// Each entry is a string fragment (or "\x08" for backspace, "\x1b[D" etc. for cursor).
    static PENDING_TEXT: RefCell<Vec<String>> = RefCell::new(Vec::new());

    /// Which field was tapped (set by on_tap_notify, consumed in drain_pending_text).
    static TAPPED_FIELD: RefCell<Option<u8>> = RefCell::new(None);

    /// X coordinate of the last tap on a text field (for cursor positioning).
    static TAPPED_X: RefCell<Option<f32>> = RefCell::new(None);
}

/// Install the keyboard callback that pushes typed text into PENDING_TEXT.
fn install_keyboard_callback() {
    gpui_mobile::set_text_input_callback(Some(Box::new(|text: &str| {
        PENDING_TEXT.with(|pending| {
            pending.borrow_mut().push(text.to_string());
        });
    })));
    // Mark dirty so the next frame picks up the focused field change.
    gpui_mobile::TEXT_INPUT_DIRTY.store(true, std::sync::atomic::Ordering::Release);
}

/// Approximate average character width in logical pixels for tap-to-cursor.
const AVG_CHAR_WIDTH: f32 = 8.0;
/// Approximate left padding of the text within the input field.
const TEXT_START_X: f32 = 12.0;

/// Drain pending keyboard text and apply it to the Router's focused field.
/// Also processes pending field-tap signals and tap-to-position.
pub fn drain_pending_text(router: &mut Router) {
    // Apply any pending field focus from on_tap_notify
    TAPPED_FIELD.with(|field| {
        if let Some(idx) = field.borrow_mut().take() {
            router.form.focused_field = Some(idx);
        }
    });

    // Process tap position for cursor placement
    TAPPED_X.with(|x_cell| {
        if let Some(x) = x_cell.borrow_mut().take() {
            let field = match router.form.focused_field {
                Some(0) => &mut router.form.full_name,
                Some(1) => &mut router.form.email,
                Some(2) => &mut router.form.phone,
                _ => return,
            };
            field.set_cursor_from_x(x, TEXT_START_X, AVG_CHAR_WIDTH);
        }
    });

    PENDING_TEXT.with(|pending| {
        let texts: Vec<String> = pending.borrow_mut().drain(..).collect();

        // Count consecutive backspaces — if many arrive in one frame the user
        // is holding the delete key, so clear the field entirely.
        let backspace_count = texts.iter().filter(|t| t.as_str() == "\x08").count();

        if backspace_count >= 6 {
            let field = match router.form.focused_field {
                Some(0) => &mut router.form.full_name,
                Some(1) => &mut router.form.email,
                Some(2) => &mut router.form.phone,
                _ => return,
            };
            field.text.clear();
            field.cursor = 0;
            field.selection = None;
        } else {
            for text in texts {
                let field = match router.form.focused_field {
                    Some(0) => &mut router.form.full_name,
                    Some(1) => &mut router.form.email,
                    Some(2) => &mut router.form.phone,
                    _ => continue,
                };
                match text.as_str() {
                    "\x08" => field.delete_at_cursor(),
                    "\x1b[D" => field.move_cursor_left(),
                    "\x1b[C" => field.move_cursor_right(),
                    "\x1b[H" => field.move_cursor_to_start(),
                    "\x1b[F" => field.move_cursor_to_end(),
                    other => field.insert_at_cursor(other),
                }
            }
        }
    });
}

/// Render the Material Form example screen with interactive controls.
pub fn render(router: &mut Router, cx: &mut Context<Router>) -> impl IntoElement {
    log::info!("Form: render() called");
    // Drain any pending keyboard input into the focused field.
    drain_pending_text(router);

    let dark = router.dark_mode;
    let theme = MaterialTheme::from_appearance(dark);
    let sub_text: u32 = if dark { super::SUBTEXT } else { super::LIGHT_SUBTEXT };
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
                                .value(&form.full_name.text)
                                .cursor(form.full_name.cursor)
                                .selection(form.full_name.normalized_selection())
                                .placeholder("Enter your name")
                                .keyboard_type(KeyboardType::Default)
                                .focused(form.focused_field == Some(0))
                                .on_tap_notify(|event: &MouseDownEvent| {
                                    log::info!("Form: name field tapped");
                                    TAPPED_FIELD.with(|f| *f.borrow_mut() = Some(0));
                                    TAPPED_X.with(|x| *x.borrow_mut() = Some(event.position.x.as_f32()));
                                    install_keyboard_callback();
                                    gpui_mobile::show_keyboard_with_type(KeyboardType::Default);
                                })
                                .render(cx),
                        )
                        .child(
                            TextInput::<Router>::new("input-email", theme)
                                .label("Email")
                                .value(&form.email.text)
                                .cursor(form.email.cursor)
                                .selection(form.email.normalized_selection())
                                .placeholder("user@example.com")
                                .keyboard_type(KeyboardType::EmailAddress)
                                .focused(form.focused_field == Some(1))
                                .on_tap_notify(|event: &MouseDownEvent| {
                                    log::info!("Form: email field tapped");
                                    TAPPED_FIELD.with(|f| *f.borrow_mut() = Some(1));
                                    TAPPED_X.with(|x| *x.borrow_mut() = Some(event.position.x.as_f32()));
                                    install_keyboard_callback();
                                    gpui_mobile::show_keyboard_with_type(KeyboardType::EmailAddress);
                                })
                                .render(cx),
                        )
                        .child(
                            TextInput::<Router>::new("input-phone", theme)
                                .label("Phone")
                                .value(&form.phone.text)
                                .cursor(form.phone.cursor)
                                .selection(form.phone.normalized_selection())
                                .placeholder("+1 (555) 000-0000")
                                .keyboard_type(KeyboardType::Phone)
                                .focused(form.focused_field == Some(2))
                                .on_tap_notify(|event: &MouseDownEvent| {
                                    log::info!("Form: phone field tapped");
                                    TAPPED_FIELD.with(|f| *f.borrow_mut() = Some(2));
                                    TAPPED_X.with(|x| *x.borrow_mut() = Some(event.position.x.as_f32()));
                                    install_keyboard_callback();
                                    gpui_mobile::show_keyboard_with_type(KeyboardType::Phone);
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
