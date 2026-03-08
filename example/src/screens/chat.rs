//! iMessage-style chat screen with bubbles, reactions, images, timestamps,
//! and a working text input composer bar.

use gpui::{div, prelude::*, px, rgb, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
use gpui_mobile::KeyboardType;
use std::cell::RefCell;

use super::Router;

// ── iMessage colour palette ─────────────────────────────────────────────────

const IMESSAGE_BLUE: u32 = 0x007AFF;
const BUBBLE_RECEIVED_DARK: u32 = 0x2C2C2E;
const BUBBLE_RECEIVED_LIGHT: u32 = 0xE9E9EB;
const COMPOSER_BG_DARK: u32 = 0x1C1C1E;
const COMPOSER_BG_LIGHT: u32 = 0xF2F2F7;
const COMPOSER_FIELD_DARK: u32 = 0x2C2C2E;
const COMPOSER_FIELD_LIGHT: u32 = 0xFFFFFF;
const TIMESTAMP_COLOR: u32 = 0x8E8E93;
const REACTION_BG_DARK: u32 = 0x3A3A3C;
const REACTION_BG_LIGHT: u32 = 0xE5E5EA;
const REACTION_PICKER_BG_DARK: u32 = 0x2C2C2E;
const REACTION_PICKER_BG_LIGHT: u32 = 0xFFFFFF;
const MIC_RECORDING_COLOR: u32 = 0xFF3B30;

// ── Reaction emoji palette ──────────────────────────────────────────────────

const REACTION_EMOJIS: &[&str] = &[
    "\u{2764}\u{fe0f}", // ❤️
    "\u{1f44d}",         // 👍
    "\u{1f44e}",         // 👎
    "\u{1f602}",         // 😂
    "\u{2755}",          // ❕
    "\u{2754}",          // ❔
];

// ── Sample data ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct ChatMessage {
    text: &'static str,
    is_me: bool,
    timestamp: &'static str,
    reactions: &'static [(&'static str, u8)],
    has_image: bool,
    image_color: u32,
    status: MessageStatus,
}

#[derive(Clone, Copy, PartialEq)]
enum MessageStatus {
    None,
    Delivered,
    Read,
}

const MESSAGES: &[ChatMessage] = &[
    ChatMessage {
        text: "",
        is_me: false,
        timestamp: "9:41 AM",
        reactions: &[],
        has_image: true,
        image_color: 0x1565C0,
        status: MessageStatus::None,
    },
    ChatMessage {
        text: "Look at this view from the hike today!",
        is_me: false,
        timestamp: "",
        reactions: &[("\u{2764}\u{fe0f}", 2), ("\u{1f525}", 1)],
        has_image: false,
        image_color: 0,
        status: MessageStatus::None,
    },
    ChatMessage {
        text: "Wow that's incredible!! Where is this?",
        is_me: true,
        timestamp: "",
        reactions: &[],
        has_image: false,
        image_color: 0,
        status: MessageStatus::Read,
    },
    ChatMessage {
        text: "Mount Tamalpais, just north of SF. The fog was rolling in perfectly",
        is_me: false,
        timestamp: "",
        reactions: &[("\u{1f60d}", 1)],
        has_image: false,
        image_color: 0,
        status: MessageStatus::None,
    },
    ChatMessage {
        text: "We should go together next weekend! I know a great trail that goes to the summit",
        is_me: true,
        timestamp: "9:45 AM",
        reactions: &[("\u{1f44d}", 1)],
        has_image: false,
        image_color: 0,
        status: MessageStatus::Read,
    },
    ChatMessage {
        text: "Yes!! I'm so down. Let me check my schedule",
        is_me: false,
        timestamp: "",
        reactions: &[],
        has_image: false,
        image_color: 0,
        status: MessageStatus::None,
    },
    ChatMessage {
        text: "Saturday works for me. Want to leave early? Like 7am?",
        is_me: false,
        timestamp: "",
        reactions: &[],
        has_image: false,
        image_color: 0,
        status: MessageStatus::None,
    },
    ChatMessage {
        text: "Perfect, Saturday 7am it is",
        is_me: true,
        timestamp: "9:48 AM",
        reactions: &[],
        has_image: false,
        image_color: 0,
        status: MessageStatus::Delivered,
    },
    ChatMessage {
        text: "I'll bring coffee and snacks",
        is_me: true,
        timestamp: "",
        reactions: &[("\u{2764}\u{fe0f}", 1), ("\u{2615}", 1)],
        has_image: false,
        image_color: 0,
        status: MessageStatus::Read,
    },
    ChatMessage {
        text: "",
        is_me: false,
        timestamp: "10:02 AM",
        reactions: &[("\u{1f923}", 3)],
        has_image: true,
        image_color: 0x2E7D32,
        status: MessageStatus::None,
    },
    ChatMessage {
        text: "Haha the trail map! Last time we got so lost",
        is_me: false,
        timestamp: "",
        reactions: &[],
        has_image: false,
        image_color: 0,
        status: MessageStatus::None,
    },
    ChatMessage {
        text: "That was YOUR fault for insisting we take the \"shortcut\" \u{1f602}",
        is_me: true,
        timestamp: "",
        reactions: &[("\u{1f602}", 2)],
        has_image: false,
        image_color: 0,
        status: MessageStatus::Read,
    },
    ChatMessage {
        text: "Ok ok fair point. This time we follow the actual trail markers",
        is_me: false,
        timestamp: "",
        reactions: &[],
        has_image: false,
        image_color: 0,
        status: MessageStatus::None,
    },
    ChatMessage {
        text: "Deal. Can't wait! \u{26f0}\u{fe0f}\u{2728}",
        is_me: true,
        timestamp: "10:05 AM",
        reactions: &[],
        has_image: false,
        image_color: 0,
        status: MessageStatus::Delivered,
    },
];

// ── Thread-local state for chat text input ──────────────────────────────────

thread_local! {
    static CHAT_PENDING_TEXT: RefCell<Vec<String>> = RefCell::new(Vec::new());
    static CHAT_FIELD_TAPPED: RefCell<bool> = RefCell::new(false);
}

fn install_chat_keyboard_callback() {
    gpui_mobile::set_text_input_callback(Some(Box::new(|text: &str| {
        CHAT_PENDING_TEXT.with(|pending| {
            pending.borrow_mut().push(text.to_string());
        });
    })));
    gpui_mobile::TEXT_INPUT_DIRTY.store(true, std::sync::atomic::Ordering::Release);
}

fn drain_chat_pending_text(router: &mut Router) {
    CHAT_FIELD_TAPPED.with(|tapped| {
        let mut val = tapped.borrow_mut();
        if *val {
            *val = false;
            router.chat_focused = true;
        }
    });

    CHAT_PENDING_TEXT.with(|pending| {
        let texts: Vec<String> = pending.borrow_mut().drain(..).collect();
        let backspace_count = texts.iter().filter(|t| t.as_str() == "\x08").count();

        if backspace_count >= 6 {
            router.chat_compose_text.clear();
        } else {
            for text in texts {
                match text.as_str() {
                    "\x08" => {
                        router.chat_compose_text.pop();
                    }
                    other => {
                        router.chat_compose_text.push_str(other);
                    }
                }
            }
        }
    });
}

// ── Render ──────────────────────────────────────────────────────────────────

pub fn render(router: &mut Router, cx: &mut gpui::Context<Router>) -> impl IntoElement {
    drain_chat_pending_text(router);

    let dark = router.dark_mode;
    let sent_messages = router.chat_sent_messages.clone();
    let compose_text = router.chat_compose_text.clone();
    let focused = router.chat_focused;
    let reaction_picker_msg = router.chat_reaction_picker;
    let user_reactions = router.chat_user_reactions.clone();
    let recording = router.chat_mic_recording;
    let swipe_offset = router.chat_swipe_offset;

    let kb_height = gpui_mobile::keyboard_height();
    let safe_bottom = router.safe_area.bottom;
    let kb_padding = (kb_height - safe_bottom).max(0.0);

    div()
        .flex()
        .flex_col()
        .flex_1()
        .size_full()
        // ── Messages area with swipe gesture ─────────────────────────────
        .child(
            div()
                .id("chat-messages-scroll")
                .flex_1()
                .overflow_y_scroll()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _window, _cx| {
                        this.chat_swipe_start_x = Some(event.position.x.as_f32());
                        this.chat_swipe_offset = 0.0;
                    }),
                )
                .on_mouse_move(
                    cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        if let Some(start_x) = this.chat_swipe_start_x {
                            let dx = event.position.x.as_f32() - start_x;
                            // Only activate swipe if horizontal movement > 10px
                            if dx.abs() > 10.0 {
                                // Clamp to reasonable range with rubber-band
                                this.chat_swipe_offset = dx.clamp(-80.0, 80.0);
                                cx.notify();
                            }
                        }
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                        this.chat_swipe_start_x = None;
                        this.chat_swipe_offset = 0.0;
                        cx.notify();
                    }),
                )
                .child(render_messages(
                    dark,
                    &sent_messages,
                    reaction_picker_msg,
                    &user_reactions,
                    swipe_offset,
                    cx,
                )),
        )
        // ── Composer bar ─────────────────────────────────────────────────
        .child(render_composer(dark, &compose_text, focused, recording, cx))
        // ── Keyboard spacer ──────────────────────────────────────────────
        .when(kb_padding > 0.0, |d| {
            d.child(div().w_full().h(px(kb_padding)))
        })
}

fn render_messages(
    dark: bool,
    sent_messages: &[String],
    reaction_picker_msg: Option<usize>,
    user_reactions: &[Vec<String>],
    swipe_offset: f32,
    cx: &mut gpui::Context<Router>,
) -> impl IntoElement {
    let mut container = div().flex().flex_col().px_3().pt_2().pb_2().gap_1();

    // Compute per-message timestamps for swipe reveal.
    // Static messages carry their own; we generate "Now" for sent ones.
    let show_swipe_ts = swipe_offset.abs() > 15.0;

    let mut prev_is_me: Option<bool> = None;
    for (i, msg) in MESSAGES.iter().enumerate() {
        if !msg.timestamp.is_empty() {
            container = container.child(timestamp_label(msg.timestamp));
        }

        let needs_spacing = prev_is_me.is_some() && prev_is_me != Some(msg.is_me);
        if needs_spacing {
            container = container.child(div().h(px(6.0)));
        }

        let extra_reactions = if i < user_reactions.len() {
            &user_reactions[i]
        } else {
            &[] as &[String]
        };

        // Determine inline timestamp for this message (shown on swipe)
        let inline_ts = if !msg.timestamp.is_empty() {
            msg.timestamp
        } else {
            // Find the nearest timestamp above
            MESSAGES[..i]
                .iter()
                .rev()
                .find(|m| !m.timestamp.is_empty())
                .map(|m| m.timestamp)
                .unwrap_or("9:41 AM")
        };

        container = container.child(render_bubble_interactive(
            msg,
            i,
            dark,
            reaction_picker_msg == Some(i),
            extra_reactions,
            if show_swipe_ts { Some((swipe_offset, inline_ts)) } else { None },
            cx,
        ));
        prev_is_me = Some(msg.is_me);
    }

    for (j, text) in sent_messages.iter().enumerate() {
        let sent_idx = MESSAGES.len() + j;
        let extra_reactions = if sent_idx < user_reactions.len() {
            &user_reactions[sent_idx]
        } else {
            &[] as &[String]
        };
        container = container.child(div().h(px(2.0)));
        container = container.child(render_sent_bubble_interactive(
            text,
            sent_idx,
            dark,
            reaction_picker_msg == Some(sent_idx),
            extra_reactions,
            if show_swipe_ts { Some(swipe_offset) } else { None },
            cx,
        ));
    }

    container = container.child(div().h(px(8.0)));
    container
}

fn timestamp_label(time: &str) -> impl IntoElement {
    div()
        .flex()
        .justify_center()
        .py_2()
        .child(
            div()
                .text_xs()
                .text_color(rgb(TIMESTAMP_COLOR))
                .child(time.to_string()),
        )
}

// ── Interactive bubble (static messages) ────────────────────────────────────

fn render_bubble_interactive(
    msg: &ChatMessage,
    idx: usize,
    dark: bool,
    show_picker: bool,
    extra_reactions: &[String],
    cx: &mut gpui::Context<Router>,
) -> impl IntoElement {
    let bubble_color = if msg.is_me {
        IMESSAGE_BLUE
    } else if dark {
        BUBBLE_RECEIVED_DARK
    } else {
        BUBBLE_RECEIVED_LIGHT
    };

    let text_color = if msg.is_me || dark { 0xFFFFFF } else { 0x000000 };
    let max_width = 280.0;

    let mut bubble = div()
        .id(format!("msg-{idx}"))
        .max_w(px(max_width))
        .rounded(px(18.0))
        .overflow_hidden();

    // Image with rounded corners
    if msg.has_image {
        bubble = bubble.child(
            div()
                .w(px(max_width))
                .h(px(180.0))
                .rounded(px(18.0))
                .overflow_hidden()
                .bg(rgb(msg.image_color))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_3xl()
                        .text_color(rgb(0xFFFFFF))
                        .child(if msg.image_color == 0x1565C0 {
                            "\u{1f3d4}\u{fe0f}"
                        } else {
                            "\u{1f5fa}\u{fe0f}"
                        }),
                ),
        );
    }

    if !msg.text.is_empty() {
        bubble = bubble
            .bg(rgb(bubble_color))
            .px(px(14.0))
            .py(px(8.0))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(text_color))
                    .child(msg.text.to_string()),
            );
    } else if msg.has_image {
        bubble = bubble.bg(rgb(bubble_color));
    }

    // Tap to toggle reaction picker
    bubble = bubble.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
            if this.chat_reaction_picker == Some(idx) {
                this.chat_reaction_picker = None;
            } else {
                this.chat_reaction_picker = Some(idx);
            }
            cx.notify();
        }),
    );

    // Wrapper
    let mut row = div().flex().flex_col().w_full();
    if msg.is_me {
        row = row.items_end();
    } else {
        row = row.items_start();
    }

    // Reaction picker (above the bubble)
    if show_picker {
        row = row.child(render_reaction_picker(idx, msg.is_me, dark, cx));
    }

    row = row.child(bubble);

    // Reactions display
    row = row.child(render_reactions_row(
        msg.reactions,
        extra_reactions,
        msg.is_me,
        dark,
    ));

    // Delivery status
    if msg.is_me && msg.status != MessageStatus::None {
        let status_text = match msg.status {
            MessageStatus::Delivered => "Delivered",
            MessageStatus::Read => "Read",
            MessageStatus::None => "",
        };
        row = row.child(
            div()
                .flex()
                .w_full()
                .justify_end()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(TIMESTAMP_COLOR))
                        .pr(px(4.0))
                        .child(status_text.to_string()),
                ),
        );
    }

    row
}

// ── Interactive sent bubble (user-composed messages) ────────────────────────

fn render_sent_bubble_interactive(
    text: &str,
    idx: usize,
    dark: bool,
    show_picker: bool,
    extra_reactions: &[String],
    cx: &mut gpui::Context<Router>,
) -> impl IntoElement {
    let mut row = div().flex().flex_col().w_full().items_end();

    if show_picker {
        row = row.child(render_reaction_picker(idx, true, dark, cx));
    }

    row = row.child(
        div()
            .id(format!("sent-msg-{idx}"))
            .max_w(px(280.0))
            .rounded(px(18.0))
            .bg(rgb(IMESSAGE_BLUE))
            .px(px(14.0))
            .py(px(8.0))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xFFFFFF))
                    .child(text.to_string()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                    if this.chat_reaction_picker == Some(idx) {
                        this.chat_reaction_picker = None;
                    } else {
                        this.chat_reaction_picker = Some(idx);
                    }
                    cx.notify();
                }),
            ),
    );

    // Reactions
    row = row.child(render_reactions_row(
        &[],
        extra_reactions,
        true,
        dark,
    ));

    row = row.child(
        div()
            .flex()
            .w_full()
            .justify_end()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(TIMESTAMP_COLOR))
                    .pr(px(4.0))
                    .child("Delivered"),
            ),
    );

    row
}

// ── Reaction picker bar ─────────────────────────────────────────────────────

fn render_reaction_picker(
    msg_idx: usize,
    is_me: bool,
    dark: bool,
    cx: &mut gpui::Context<Router>,
) -> impl IntoElement {
    let picker_bg = if dark {
        REACTION_PICKER_BG_DARK
    } else {
        REACTION_PICKER_BG_LIGHT
    };

    let mut picker = div()
        .flex()
        .flex_row()
        .gap(px(8.0))
        .bg(rgb(picker_bg))
        .rounded(px(20.0))
        .px(px(12.0))
        .py(px(6.0))
        .mb(px(4.0))
        .shadow_lg();

    for &emoji in REACTION_EMOJIS {
        let emoji_owned = emoji.to_string();
        picker = picker.child(
            div()
                .id(format!("react-{msg_idx}-{emoji}"))
                .text_xl()
                .child(emoji.to_string())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                        // Ensure the reactions vec is big enough
                        let total_msgs =
                            MESSAGES.len() + this.chat_sent_messages.len();
                        if this.chat_user_reactions.len() < total_msgs {
                            this.chat_user_reactions.resize(total_msgs, Vec::new());
                        }
                        if msg_idx < this.chat_user_reactions.len() {
                            let reactions = &mut this.chat_user_reactions[msg_idx];
                            // Toggle: remove if already reacted with this emoji
                            if let Some(pos) =
                                reactions.iter().position(|r| r == &emoji_owned)
                            {
                                reactions.remove(pos);
                            } else {
                                reactions.push(emoji_owned.clone());
                            }
                        }
                        this.chat_reaction_picker = None;
                        cx.notify();
                    }),
                ),
        );
    }

    let mut wrapper = div().flex().w_full();
    if is_me {
        wrapper = wrapper.justify_end().pr(px(8.0));
    } else {
        wrapper = wrapper.pl(px(8.0));
    }
    wrapper.child(picker)
}

// ── Reactions display row ───────────────────────────────────────────────────

fn render_reactions_row(
    static_reactions: &[(&str, u8)],
    extra_reactions: &[String],
    is_me: bool,
    dark: bool,
) -> impl IntoElement {
    let reaction_bg = if dark { REACTION_BG_DARK } else { REACTION_BG_LIGHT };

    let has_any = !static_reactions.is_empty() || !extra_reactions.is_empty();
    if !has_any {
        return div();
    }

    let mut reaction_row = div()
        .flex()
        .flex_row()
        .gap(px(4.0))
        .mt(px(-4.0));

    if is_me {
        reaction_row = reaction_row.mr(px(8.0));
    } else {
        reaction_row = reaction_row.ml(px(8.0));
    }

    // Static reactions from sample data
    for &(emoji, count) in static_reactions {
        reaction_row = reaction_row.child(reaction_pill(emoji, count, reaction_bg));
    }

    // User-added reactions
    for emoji in extra_reactions {
        reaction_row = reaction_row.child(reaction_pill(emoji, 1, reaction_bg));
    }

    let mut wrapper = div().flex().w_full();
    if is_me {
        wrapper = wrapper.justify_end();
    }
    wrapper.child(reaction_row)
}

fn reaction_pill(emoji: &str, count: u8, bg: u32) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(2.0))
        .bg(rgb(bg))
        .rounded(px(12.0))
        .px(px(6.0))
        .py(px(2.0))
        .child(div().text_xs().child(emoji.to_string()))
        .when(count > 1, |d| {
            d.child(
                div()
                    .text_xs()
                    .text_color(rgb(TIMESTAMP_COLOR))
                    .child(count.to_string()),
            )
        })
}

// ── Composer bar ────────────────────────────────────────────────────────────

fn render_composer(
    dark: bool,
    compose_text: &str,
    focused: bool,
    recording: bool,
    cx: &mut gpui::Context<Router>,
) -> impl IntoElement {
    let composer_bg = if dark { COMPOSER_BG_DARK } else { COMPOSER_BG_LIGHT };
    let field_bg = if dark { COMPOSER_FIELD_DARK } else { COMPOSER_FIELD_LIGHT };
    let text_color = if dark { 0xFFFFFF } else { 0x000000 };
    let placeholder_color = TIMESTAMP_COLOR;
    let has_text = !compose_text.is_empty();
    let border_color = if focused {
        IMESSAGE_BLUE
    } else if dark {
        0x3A3A3C
    } else {
        0xC7C7CC
    };

    div()
        .flex()
        .flex_row()
        .items_end()
        .gap_2()
        .px_3()
        .py_2()
        .bg(rgb(composer_bg))
        .border_t_1()
        .border_color(rgb(if dark { 0x38383A } else { 0xC6C6C8 }))
        // Camera / plus button
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(36.0))
                .h(px(36.0))
                .rounded_full()
                .bg(rgb(IMESSAGE_BLUE))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xFFFFFF))
                        .child("+"),
                ),
        )
        // Text field
        .child(
            div()
                .id("chat-composer-field")
                .flex_1()
                .flex()
                .flex_row()
                .items_center()
                .min_h(px(36.0))
                .rounded(px(18.0))
                .border_1()
                .border_color(rgb(border_color))
                .bg(rgb(field_bg))
                .px_3()
                .child({
                    let mut row =
                        div().flex_1().flex().flex_row().items_center().text_sm();

                    if has_text {
                        row = row
                            .text_color(rgb(text_color))
                            .child(compose_text.to_string());
                    } else if !focused {
                        row = row
                            .text_color(rgb(placeholder_color))
                            .child("iMessage".to_string());
                    }

                    if focused {
                        row = row.child(
                            div()
                                .w(px(2.0))
                                .h(px(16.0))
                                .bg(rgb(IMESSAGE_BLUE)),
                        );
                    }

                    row
                })
                .on_mouse_down(
                    MouseButton::Left,
                    move |_event: &MouseDownEvent, _window, _cx| {
                        CHAT_FIELD_TAPPED.with(|f| *f.borrow_mut() = true);
                        install_chat_keyboard_callback();
                        gpui_mobile::show_keyboard_with_type(KeyboardType::Default);
                    },
                ),
        )
        // Right side: send button OR mic button
        .child(if has_text {
            // Send button
            div()
                .id("chat-send-btn")
                .flex()
                .items_center()
                .justify_center()
                .w(px(36.0))
                .h(px(36.0))
                .rounded_full()
                .bg(rgb(IMESSAGE_BLUE))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xFFFFFF))
                        .child("\u{2191}"), // ↑ arrow
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                        if !this.chat_compose_text.is_empty() {
                            let text = std::mem::take(&mut this.chat_compose_text);
                            this.chat_sent_messages.push(text);
                            cx.notify();
                        }
                    }),
                )
                .into_any_element()
        } else {
            // Mic button
            let mic_bg = if recording {
                MIC_RECORDING_COLOR
            } else if dark {
                COMPOSER_FIELD_DARK
            } else {
                COMPOSER_FIELD_LIGHT
            };
            let mic_fg = if recording {
                0xFFFFFF
            } else {
                IMESSAGE_BLUE
            };
            div()
                .id("chat-mic-btn")
                .flex()
                .items_center()
                .justify_center()
                .w(px(36.0))
                .h(px(36.0))
                .rounded_full()
                .bg(rgb(mic_bg))
                .when(!recording, |d| {
                    d.border_1().border_color(rgb(if dark { 0x3A3A3C } else { 0xC7C7CC }))
                })
                .child(
                    div()
                        .text_base()
                        .text_color(rgb(mic_fg))
                        .child(if recording { "\u{23f9}" } else { "\u{1f3a4}" }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                        if this.chat_mic_recording {
                            let _ =
                                gpui_mobile::packages::microphone::stop_recording();
                            this.chat_mic_recording = false;
                        } else {
                            let path = format!(
                                "{}/voice_{}.m4a",
                                std::env::temp_dir().display(),
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs()
                            );
                            match gpui_mobile::packages::microphone::start_recording(
                                &path,
                            ) {
                                Ok(()) => {
                                    this.chat_mic_recording = true;
                                }
                                Err(e) => {
                                    log::error!("Mic start error: {e}");
                                }
                            }
                        }
                        cx.notify();
                    }),
                )
                .into_any_element()
        })
}
