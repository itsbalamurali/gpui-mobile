//! A local chat interaction demo; no model service or credentials are required.
use gpui::{prelude::*, *};
use gpui_kit::component::{
    bubble::{Bubble, BubbleVariant},
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::{Input, InputEvent, InputState},
    marker::{Marker, MarkerContent, MarkerLoadingStyle},
    message::{Message, MessageAlignment, MessageContent, MessageFooter},
    text::{TextView, TextViewStyle},
    ActiveTheme, Disableable, Icon, Sizable, StyledExt,
};

const EXAMPLE: &str = include_str!("assistant.md");

struct ChatMessage {
    id: u64,
    user: bool,
    body: SharedString,
}

pub struct AiChat {
    input: Entity<InputState>,
    messages: Vec<ChatMessage>,
    next_id: u64,
    scroll: ScrollHandle,
    pending: bool,
    copied: Option<u64>,
    copy_reset_task: Option<Task<()>>,
    expanded_thought: Option<u64>,
    response_task: Option<Task<()>>,
    _subscription: Subscription,
}

impl AiChat {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Ask anything"));
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                InputEvent::Focus => gpui_mobile::show_keyboard(),
                InputEvent::Blur => gpui_mobile::hide_keyboard(),
                InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
                _ => cx.notify(),
            });
        Self {
            input,
            messages: Self::example_messages(),
            next_id: 9,
            scroll: ScrollHandle::new(),
            pending: false,
            copied: None,
            copy_reset_task: None,
            expanded_thought: Some(2),
            response_task: None,
            _subscription: subscription,
        }
    }

    fn example_messages() -> Vec<ChatMessage> {
        vec![
            ChatMessage {
                id: 1,
                user: true,
                body: "Help me plan an AI chat app with GPUI Kit.".into(),
            },
            ChatMessage {
                id: 2,
                user: false,
                body: EXAMPLE.into(),
            },
            ChatMessage {
                id: 3,
                user: true,
                body: "What belongs in the first release?".into(),
            },
            ChatMessage {
                id: 4,
                user: false,
                body: include_str!("assistant-plan.md").into(),
            },
            ChatMessage {
                id: 5,
                user: true,
                body: "Show me the message rendering code.".into(),
            },
            ChatMessage {
                id: 6,
                user: false,
                body: include_str!("assistant-code.md").into(),
            },
            ChatMessage {
                id: 7,
                user: true,
                body: "What if a reply gets interrupted?".into(),
            },
            ChatMessage {
                id: 8,
                user: false,
                body: include_str!("assistant-recovery.md").into(),
            },
        ]
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = self.input.read(cx).value().trim().to_owned();
        if prompt.is_empty() || self.pending {
            return;
        }
        self.messages.push(ChatMessage {
            id: self.next_id,
            user: true,
            body: prompt.into(),
        });
        self.next_id += 1;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        window.blur(cx);
        gpui_mobile::hide_keyboard();
        self.pending = true;
        self.scroll.scroll_to_bottom();
        cx.notify();
        self.response_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(1400)).await;
            let _ = this.update(cx, |this, cx| {
                this.messages.push(ChatMessage {
                    id: this.next_id,
                    user: false,
                    body: "This is a **local demo reply**. Connect your model to turn this conversation into a working assistant.\n\nThe interface is built with GPUI Kit’s Message, TextView and Input components.".into(),
                });
                this.next_id += 1;
                this.pending = false;
                this.scroll.scroll_to_bottom();
                cx.notify();
            });
        }));
    }

    fn message(&self, message: &ChatMessage, cx: &Context<Self>) -> AnyElement {
        if message.user {
            return Message::new()
                .text_base()
                .alignment(MessageAlignment::End)
                .content(
                    MessageContent::new().bubble(
                        Bubble::new()
                            .with_variant(BubbleVariant::Muted)
                            .child(message.body.clone()),
                    ),
                )
                .into_any_element();
        }
        let id = message.id;
        let body = message.body.clone();
        Message::new()
            .text_base()
            .content(
                MessageContent::new()
                    .child(
                        div().w_full().min_w_0().flex().flex_col().items_start().gap_2().mb_3()
                            .child(
                                Button::new(("thought", id))
                                    .custom(ButtonCustomVariant::new(cx).foreground(cx.theme().muted_foreground))
                                    .small().h_10().px_0()
                                    .accessibility_label("Toggle thought summary")
                                    .child("Thought summary")
                                    .child(Icon::default().data(if self.expanded_thought == Some(id) {
                                        include_bytes!("../../assets/chevron-down.svg")
                                    } else {
                                        include_bytes!("../../assets/chevron-right.svg")
                                    }))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.expanded_thought = if this.expanded_thought == Some(id) { None } else { Some(id) };
                                        cx.notify();
                                    })))
                            .when(self.expanded_thought == Some(id), |this| {
                                this.child(div().w_full().min_w_0()
                                    .text_sm().line_height(relative(1.5))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(match id {
                                        2 => "Focus on the conversation first. Keep the interface quiet and make progress easy to follow.",
                                        4 => "Prioritize the complete reply flow before adding more ways to interact.",
                                        6 => "Keep message content separate from the surrounding conversation controls.",
                                        8 => "Preserve the partial reply and the draft. Make recovery clear without losing context.",
                                        _ => "Show a local response using the existing conversation components.",
                                    }))
                            })
                    )
                    .child(
                    TextView::markdown(("assistant", id), body.clone())
                        .w_full()
                        .min_w_0()
                        .text_base()
                        .line_height(relative(1.5))
                        .style(TextViewStyle::default().paragraph_gap(rems(0.75)))
                        .selectable(true)
                        .scrollable(false),
                ),
            )
            .footer(
                MessageFooter::new().content_inset(false).child(
                    Button::new(("copy", id))
                        .custom(ButtonCustomVariant::new(cx).foreground(cx.theme().muted_foreground))
                        .accessibility_label("Copy message")
                        .small()
                        .w_10()
                        .h_10()
                        .px_0()
                        .child(div().w_full().flex().items_center().child(
                            Icon::default().small().data(if self.copied == Some(id) {
                                include_bytes!("../../assets/check.svg")
                            } else {
                                include_bytes!("../../assets/copy.svg")
                            })
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(body.to_string()));
                            this.copied = Some(id);
                            this.copy_reset_task = Some(cx.spawn(async move |this, cx| {
                                cx.background_executor().timer(std::time::Duration::from_millis(1500)).await;
                                let _ = this.update(cx, |this, cx| {
                                    this.copied = None;
                                    cx.notify();
                                });
                            }));
                            cx.notify();
                        })),
                ),
            )
            .into_any_element()
    }
}

impl Render for AiChat {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .id("conversation")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .when(self.messages.is_empty(), |this| {
                                this.child(
                                    div()
                                        .py_8()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap_3()
                                        .child(
                                            div()
                                                .text_lg()
                                                .font_semibold()
                                                .child("What can I help with?"),
                                        )
                                        .child(
                                            Button::new("show-demo")
                                                .outline()
                                                .label("Plan an AI chat app")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.messages = Self::example_messages();
                                                    this.expanded_thought = Some(2);
                                                    this.next_id = 9;
                                                    this.scroll.set_offset(point(px(0.), px(0.)));
                                                    cx.notify();
                                                })),
                                        ),
                                )
                            })
                            .children(
                                self.messages
                                    .iter()
                                    .map(|message| self.message(message, cx)),
                            )
                            .when(self.pending, |this| {
                                this.child(
                                    Marker::new()
                                        .id("thinking-status")
                                        .role(Role::Status)
                                        .loading(true)
                                        .with_loading_style(MarkerLoadingStyle::Shimmer)
                                        .content(MarkerContent::new().text("Thinking…")),
                                )
                            }),
                    ),
            )
            .child(
                div().flex_none().px_3().pt_2().pb_2().child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .p_2()
                        .rounded(cx.theme().radius_3xl())
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().background)
                        .shadow_2xs()
                        .child(Input::new(&self.input).appearance(false).flex_1().min_w_0())
                        .child(
                            Button::new("send")
                                .primary()
                                .icon(
                                    Icon::default()
                                        .data(include_bytes!("../../assets/arrow-up.svg")),
                                )
                                .w_10()
                                .h_10()
                                .rounded(cx.theme().radius_full())
                                .accessibility_label("Send message")
                                .disabled(
                                    self.pending || self.input.read(cx).value().trim().is_empty(),
                                )
                                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                        ),
                ),
            )
    }
}
