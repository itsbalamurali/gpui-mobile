//! Tinder-style swipeable card stack with swipe animations.
//!
//! Drag cards left (nope) or right (like). Cards are stacked with a slight
//! offset. The top card follows the finger and rotates proportionally to
//! the horizontal drag distance. Release triggers a fly-off animation when
//! past the swipe threshold, or a snap-back otherwise.

use std::cell::RefCell;
use std::time::Duration;

use gpui::{div, img, prelude::*, px, rgb, Animation, AnimationExt};

use super::{Router, BLUE, GREEN, LIGHT_CARD_BG, LIGHT_TEXT, RED, SURFACE0, TEXT, YELLOW};

/// Swipe distance threshold (in px) to trigger card dismissal.
const SWIPE_THRESHOLD: f32 = 100.0;

/// Demo profile cards.
const PROFILES: &[Profile] = &[
    Profile { name: "Alex", age: 28, bio: "Coffee enthusiast. Hiking on weekends.", color: 0xE91E63, photo_id: 1027 },
    Profile { name: "Jordan", age: 25, bio: "Photographer & world traveler.", color: 0x9C27B0, photo_id: 1025 },
    Profile { name: "Casey", age: 31, bio: "Software engineer. Cat person.", color: 0x3F51B5, photo_id: 1005 },
    Profile { name: "Morgan", age: 27, bio: "Yoga instructor. Plant parent.", color: 0x009688, photo_id: 1011 },
    Profile { name: "Riley", age: 24, bio: "Music producer. Night owl.", color: 0xFF9800, photo_id: 1012 },
    Profile { name: "Taylor", age: 29, bio: "Chef by day, gamer by night.", color: 0x795548, photo_id: 1015 },
    Profile { name: "Quinn", age: 26, bio: "Surfer. Beach lover. Dog dad.", color: 0x00BCD4, photo_id: 1039 },
    Profile { name: "Avery", age: 30, bio: "Startup founder. Marathon runner.", color: 0x4CAF50, photo_id: 1074 },
];

struct Profile {
    name: &'static str,
    age: u32,
    bio: &'static str,
    color: u32,
    /// Picsum photo ID for the profile card background.
    photo_id: u32,
}

/// All mutable state for the swiper screen, stored in a thread-local.
#[derive(Default)]
pub struct SwiperState {
    pub index: usize,
    pub drag_x: f32,
    pub drag_start_x: Option<f32>,
    pub dragging: bool,
    pub fly_direction: f32,
    pub anim_id: u32,
}

thread_local! {
    pub(crate) static SWIPER_STATE: RefCell<SwiperState> = RefCell::new(SwiperState::default());
}

/// Reset swiper state to defaults.
pub fn reset_state() {
    SWIPER_STATE.with(|s| *s.borrow_mut() = SwiperState::default());
}

pub fn render(router: &Router, cx: &mut gpui::Context<Router>) -> impl IntoElement {
    let dark = router.dark_mode;
    let text_color = if dark { TEXT } else { LIGHT_TEXT };
    let _card_bg = if dark { SURFACE0 } else { LIGHT_CARD_BG };

    let (idx, drag_x, fly_dir, anim_id) = SWIPER_STATE.with(|s| {
        let s = s.borrow();
        (s.index, s.drag_x, s.fly_direction, s.anim_id)
    });

    let all_swiped = idx >= PROFILES.len();
    let is_flying = fly_dir != 0.0;

    let mut root = div()
        .flex()
        .flex_col()
        .flex_1()
        .items_center()
        .gap_4()
        .px_4()
        .py_4();

    if all_swiped {
        // All cards swiped — show reset
        root = root
            .child(div().h(px(100.0)))
            .child(
                div()
                    .text_xl()
                    .text_color(rgb(text_color))
                    .child("No more profiles!"),
            )
            .child(div().h(px(20.0)))
            .child(
                div()
                    .px_6()
                    .py_3()
                    .rounded_xl()
                    .bg(rgb(BLUE))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xFFFFFF))
                            .child("Start Over"),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|_this, _, _, cx| {
                            SWIPER_STATE.with(|s| {
                                let mut s = s.borrow_mut();
                                s.index = 0;
                                s.fly_direction = 0.0;
                            });
                            cx.notify();
                        }),
                    ),
            );
        return root;
    }

    // Card stack — show up to 3 cards (back to front)
    let stack_end = (idx + 3).min(PROFILES.len());
    let visible = &PROFILES[idx..stack_end];

    let mut stack = div()
        .w(px(320.0))
        .h(px(420.0))
        .relative();

    for (i, profile) in visible.iter().enumerate().rev() {
        let is_top = i == 0;
        let offset_y = (i as f32) * 8.0;
        let scale_factor = 1.0 - (i as f32) * 0.04;

        // When top card is flying, the second card should animate to top position
        let (base_offset_y, base_scale) = if !is_top && is_flying && i == 1 {
            // Second card inherits first card's shrinkage less
            (offset_y, scale_factor)
        } else {
            (offset_y, scale_factor)
        };

        let card_offset_x = if is_top && !is_flying { drag_x } else { 0.0 };

        // Overlay label based on drag direction (only during manual drag)
        let label_element = if is_top && !is_flying && drag_x.abs() > 30.0 {
            let (label, label_color) = if drag_x > 0.0 {
                ("LIKE", GREEN)
            } else {
                ("NOPE", RED)
            };
            let opacity = (drag_x.abs() / 120.0).min(1.0);
            Some(
                div()
                    .absolute()
                    .top(px(20.0))
                    .when(drag_x > 0.0, |d| d.left(px(20.0)))
                    .when(drag_x <= 0.0, |d| d.right(px(20.0)))
                    .px_4()
                    .py_2()
                    .rounded_lg()
                    .border_3()
                    .border_color(rgb(label_color))
                    .opacity(opacity)
                    .child(
                        div()
                            .text_xl()
                            .text_color(rgb(label_color))
                            .child(label),
                    ),
            )
        } else {
            None
        };

        // Picsum photo URL
        let photo_url: gpui::SharedString = format!(
            "https://picsum.photos/id/{}/640/840",
            profile.photo_id
        ).into();

        let card = div()
            .absolute()
            .top(px(base_offset_y))
            .left(px(card_offset_x + (1.0 - base_scale) * 160.0))
            .w(px(320.0 * base_scale))
            .h(px(420.0 * base_scale))
            .rounded_3xl()
            .overflow_hidden()
            .bg(rgb(profile.color))
            .flex()
            .flex_col()
            // Background image from picsum.photos
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(
                        img(photo_url)
                            .size_full()
                            .object_fit(gpui::ObjectFit::Cover)
                            .id(format!("swiper-img-{}", idx + i)),
                    ),
            )
            .child(div().flex_1())
            // Profile info overlay at bottom
            .child(
                div()
                    .w_full()
                    .px_5()
                    .py_4()
                    .bg(gpui::rgba(0x00000088))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_end()
                            .gap_2()
                            .child(
                                div()
                                    .text_xl()
                                    .text_color(rgb(0xFFFFFF))
                                    .child(profile.name.to_string()),
                            )
                            .child(
                                div()
                                    .text_lg()
                                    .text_color(rgb(0xCCCCCC))
                                    .child(format!("{}", profile.age)),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xDDDDDD))
                            .mt_1()
                            .child(profile.bio.to_string()),
                    ),
            )
            .children(label_element);

        // Apply fly-off animation to the top card
        if is_top && is_flying {
            let fly = fly_dir;
            stack = stack.child(
                card.with_animation(
                    format!("swipe-fly-{anim_id}"),
                    Animation::new(Duration::from_millis(300))
                        .with_easing(gpui::ease_in_out),
                    move |el, delta| {
                        // Fly off screen: 0→400px in the swipe direction
                        let offset = delta * 400.0 * fly;
                        let opacity = 1.0 - delta;
                        el.left(px(offset + (1.0 - base_scale) * 160.0))
                            .opacity(opacity)
                    },
                ),
            );
        } else {
            stack = stack.child(card);
        }
    }

    // Wrap stack in a drag area (disable drag during fly animation)
    let drag_area = div()
        .w(px(320.0))
        .h(px(420.0))
        .child(stack)
        .when(!is_flying, |el| {
            el.on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_this, event: &gpui::MouseDownEvent, _window, cx| {
                    SWIPER_STATE.with(|s| {
                        let mut s = s.borrow_mut();
                        s.dragging = true;
                        s.drag_start_x = Some(event.position.x.as_f32());
                        s.drag_x = 0.0;
                    });
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|_this, event: &gpui::MouseMoveEvent, _window, cx| {
                SWIPER_STATE.with(|s| {
                    let mut s = s.borrow_mut();
                    if let Some(start_x) = s.drag_start_x {
                        s.drag_x = event.position.x.as_f32() - start_x;
                    }
                });
                cx.notify();
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|_this, _event: &gpui::MouseUpEvent, _window, cx| {
                    let should_fly = SWIPER_STATE.with(|s| {
                        let mut s = s.borrow_mut();
                        if s.dragging {
                            s.dragging = false;
                            s.drag_start_x = None;
                            if s.drag_x.abs() > SWIPE_THRESHOLD {
                                // Trigger fly-off animation
                                s.fly_direction = if s.drag_x > 0.0 { 1.0 } else { -1.0 };
                                s.anim_id += 1;
                                let direction = if s.fly_direction > 0.0 { "LIKED" } else { "NOPED" };
                                if s.index < PROFILES.len() {
                                    log::info!("Swiper: {} {}", direction, PROFILES[s.index].name);
                                }
                                s.drag_x = 0.0;
                                return true;
                            } else {
                                // Snap back
                                s.drag_x = 0.0;
                            }
                        }
                        false
                    });
                    if should_fly {
                        // Schedule advance after animation
                        cx.spawn(async |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(320))
                                .await;
                            let _ = this.update(cx, |_this, cx| {
                                SWIPER_STATE.with(|s| {
                                    let mut s = s.borrow_mut();
                                    s.index += 1;
                                    s.fly_direction = 0.0;
                                    s.drag_x = 0.0;
                                });
                                cx.notify();
                            });
                        }).detach();
                    }
                    cx.notify();
                }),
            )
        });

    root = root.child(drag_area);

    // Action buttons row
    root = root.child(
        div()
            .flex()
            .flex_row()
            .gap_6()
            .mt_4()
            .child(action_btn("X", RED, cx.listener(|_this, _, _, cx| {
                let should_fly = SWIPER_STATE.with(|s| {
                    let mut s = s.borrow_mut();
                    if s.index < PROFILES.len() && s.fly_direction == 0.0 {
                        log::info!("Swiper: NOPED {}", PROFILES[s.index].name);
                        s.fly_direction = -1.0;
                        s.anim_id += 1;
                        s.drag_x = 0.0;
                        return true;
                    }
                    false
                });
                if should_fly {
                    cx.spawn(async |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(320))
                            .await;
                        let _ = this.update(cx, |_this, cx| {
                            SWIPER_STATE.with(|s| {
                                let mut s = s.borrow_mut();
                                s.index += 1;
                                s.fly_direction = 0.0;
                            });
                            cx.notify();
                        });
                    }).detach();
                }
                cx.notify();
            })))
            .child(action_btn("*", YELLOW, cx.listener(|_this, _, _, cx| {
                log::info!("Swiper: SUPERLIKED");
                cx.notify();
            })))
            .child(action_btn("~", GREEN, cx.listener(|_this, _, _, cx| {
                let should_fly = SWIPER_STATE.with(|s| {
                    let mut s = s.borrow_mut();
                    if s.index < PROFILES.len() && s.fly_direction == 0.0 {
                        log::info!("Swiper: LIKED {}", PROFILES[s.index].name);
                        s.fly_direction = 1.0;
                        s.anim_id += 1;
                        s.drag_x = 0.0;
                        return true;
                    }
                    false
                });
                if should_fly {
                    cx.spawn(async |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(320))
                            .await;
                        let _ = this.update(cx, |_this, cx| {
                            SWIPER_STATE.with(|s| {
                                let mut s = s.borrow_mut();
                                s.index += 1;
                                s.fly_direction = 0.0;
                            });
                            cx.notify();
                        });
                    }).detach();
                }
                cx.notify();
            }))),
    );

    root
}

fn action_btn(
    icon: &str,
    color: u32,
    handler: impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(56.0))
        .rounded_full()
        .border_2()
        .border_color(rgb(color))
        .child(
            div()
                .text_xl()
                .text_color(rgb(color))
                .child(icon.to_string()),
        )
        .on_mouse_down(gpui::MouseButton::Left, handler)
}
