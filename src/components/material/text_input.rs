//! Material Design 3 interactive text input component.
//!
//! Provides an outlined text field that shows the software keyboard on tap,
//! receives text input, and displays the current value with a cursor.

use gpui::{div, prelude::*, px, rgb, ElementId, MouseButton, MouseDownEvent};

use super::theme::MaterialTheme;

/// An interactive Material Design 3 text input field.
///
/// When tapped, this component triggers the software keyboard via
/// `gpui_mobile::show_keyboard()`. Text state is managed externally
/// by the parent component through the `on_change` callback.
///
/// # Example
///
/// ```rust,ignore
/// TextInput::new("email", theme)
///     .label("Email")
///     .value(&self.email)
///     .placeholder("user@example.com")
///     .on_change(cx.listener(|this, text: &String, _, cx| {
///         this.email = text.clone();
///         cx.notify();
///     }))
/// ```
pub struct TextInput<V: 'static> {
    id: ElementId,
    theme: MaterialTheme,
    label: Option<&'static str>,
    value: String,
    placeholder: &'static str,
    error: bool,
    error_text: Option<&'static str>,
    focused: bool,
    on_tap: Option<Box<dyn Fn(&mut V, &MouseDownEvent, &mut gpui::Window, &mut gpui::Context<V>)>>,
}

impl<V: 'static> TextInput<V> {
    /// Create a new text input with the given ID and theme.
    pub fn new(id: impl Into<ElementId>, theme: MaterialTheme) -> Self {
        Self {
            id: id.into(),
            theme,
            label: None,
            value: String::new(),
            placeholder: "",
            error: false,
            error_text: None,
            focused: false,
            on_tap: None,
        }
    }

    /// Set the floating label text.
    pub fn label(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
    }

    /// Set the current text value.
    pub fn value(mut self, value: &str) -> Self {
        self.value = value.to_string();
        self
    }

    /// Set the placeholder text shown when empty.
    pub fn placeholder(mut self, placeholder: &'static str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Mark the field as having an error.
    pub fn error(mut self, error: bool) -> Self {
        self.error = error;
        self
    }

    /// Set the error helper text shown below the field.
    pub fn error_text(mut self, text: &'static str) -> Self {
        self.error_text = Some(text);
        self
    }

    /// Mark the field as focused (shows active border color).
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Set a callback for when the field is tapped.
    ///
    /// The callback should call `gpui_mobile::show_keyboard()` and set
    /// the focused field state.
    pub fn on_tap(
        mut self,
        handler: impl Fn(&mut V, &MouseDownEvent, &mut gpui::Window, &mut gpui::Context<V>) + 'static,
    ) -> Self {
        self.on_tap = Some(Box::new(handler));
        self
    }

    /// Build the element. Must be called with a context to wire up event handlers.
    pub fn render(self, cx: &mut gpui::Context<V>) -> impl IntoElement {
        let t = self.theme;
        let border_color = if self.error {
            t.error
        } else if self.focused {
            t.primary
        } else {
            t.outline
        };
        let label_color = if self.error {
            t.error
        } else if self.focused {
            t.primary
        } else {
            t.on_surface_variant
        };

        let has_value = !self.value.is_empty();
        let display_text = if has_value {
            self.value.clone()
        } else {
            self.placeholder.to_string()
        };
        let text_color = if has_value {
            t.on_surface
        } else {
            t.on_surface_variant
        };

        let border_width = if self.focused { 2.0 } else { 1.0 };

        let mut field = div()
            .id(self.id)
            .flex()
            .flex_col()
            .gap_1()
            .w_full();

        // Label
        if let Some(label) = self.label {
            field = field.child(
                div()
                    .text_xs()
                    .text_color(rgb(label_color))
                    .child(label.to_string()),
            );
        }

        // Input container
        let mut input_box = div()
            .px_3()
            .py_2()
            .rounded_md()
            .border_color(rgb(border_color))
            .bg(rgb(t.surface));

        if border_width > 1.5 {
            input_box = input_box.border_2();
        } else {
            input_box = input_box.border_1();
        }

        // Text content with cursor
        let mut text_row = div()
            .flex()
            .flex_row()
            .items_center()
            .text_sm()
            .text_color(rgb(text_color))
            .child(display_text);

        // Show blinking cursor when focused
        if self.focused {
            text_row = text_row.child(
                div()
                    .w(px(2.0))
                    .h(px(16.0))
                    .bg(rgb(t.primary))
                    .ml_px(),
            );
        }

        input_box = input_box.child(text_row);

        // Wire up tap handler
        if let Some(on_tap) = self.on_tap {
            let on_tap = std::rc::Rc::new(on_tap);
            let on_tap_clone = on_tap.clone();
            input_box = input_box.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    (on_tap_clone)(this, event, window, cx);
                }),
            );
        }

        field = field.child(input_box);

        // Error text
        if let Some(error_text) = self.error_text {
            if self.error {
                field = field.child(
                    div()
                        .text_xs()
                        .text_color(rgb(t.error))
                        .child(error_text.to_string()),
                );
            }
        }

        field
    }
}
