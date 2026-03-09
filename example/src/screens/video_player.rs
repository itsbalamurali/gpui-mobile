//! Video player screen with playback controls, seek bar, volume & speed.

use std::cell::RefCell;
use std::time::Duration;

use gpui::{div, prelude::*, px, rgb};
use gpui_mobile::packages::video_player::VideoPlayer;

use super::{Router, BLUE, GREEN, LIGHT_CARD_BG, LIGHT_SUBTEXT, LIGHT_TEXT, MAUVE, RED, SURFACE0, SUBTEXT, TEXT, YELLOW};

/// Sample video URLs for demo.
const VIDEOS: &[(&str, &str)] = &[
    ("Big Buck Bunny", "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4"),
    ("Elephant Dream", "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ElephantsDream.mp4"),
    ("Sintel Trailer", "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/Sintel.mp4"),
    ("Tears of Steel", "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/TearsOfSteel.mp4"),
];

pub(crate) struct VideoState {
    player: Option<VideoPlayer>,
    current_video: usize,
    duration_ms: u64,
    position_ms: u64,
    video_width: u32,
    video_height: u32,
    volume: f32,
    speed: f32,
    looping: bool,
    loading: bool,
    error: Option<String>,
    polling: bool,
    surface_visible: bool,
}

impl Default for VideoState {
    fn default() -> Self {
        Self {
            player: None,
            current_video: 0,
            duration_ms: 0,
            position_ms: 0,
            video_width: 0,
            video_height: 0,
            volume: 1.0,
            speed: 1.0,
            looping: false,
            loading: false,
            error: None,
            polling: false,
            surface_visible: false,
        }
    }
}

thread_local! {
    pub(crate) static VIDEO_STATE: RefCell<VideoState> = RefCell::new(VideoState::default());
}

pub fn reset_state() {
    VIDEO_STATE.with(|s| {
        let mut st = s.borrow_mut();
        // Hide surface before dropping player
        if st.surface_visible {
            if let Some(ref p) = st.player {
                let _ = p.hide_surface();
            }
        }
        st.player.take(); // Drop calls dispose
        *st = VideoState::default();
    });
}

/// Called when navigating away from this screen — hides the native surface overlay.
pub fn dismiss() {
    VIDEO_STATE.with(|s| {
        let mut st = s.borrow_mut();
        if st.surface_visible {
            if let Some(ref p) = st.player {
                let _ = p.hide_surface();
            }
            st.surface_visible = false;
        }
    });
}

fn format_time(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{}:{:02}", mins, secs)
}

pub fn render(router: &Router, window: &mut gpui::Window, cx: &mut gpui::Context<Router>) -> impl IntoElement {
    let dark = router.dark_mode;
    let text_color = if dark { TEXT } else { LIGHT_TEXT };
    let sub_text = if dark { SUBTEXT } else { LIGHT_SUBTEXT };
    let card_bg = if dark { SURFACE0 } else { LIGHT_CARD_BG };
    let safe_area = router.safe_area;
    let viewport_width = window.viewport_size().width.as_f32();

    let (position_ms, duration_ms, volume, speed, looping, current_video, loading, error, has_player, is_playing, video_width, video_height, surface_visible) =
        VIDEO_STATE.with(|s| {
            let st = s.borrow();
            let playing = st.player.as_ref()
                .and_then(|p| p.is_playing().ok())
                .unwrap_or(false);
            (
                st.position_ms,
                st.duration_ms,
                st.volume,
                st.speed,
                st.looping,
                st.current_video,
                st.loading,
                st.error.clone(),
                st.player.is_some(),
                playing,
                st.video_width,
                st.video_height,
                st.surface_visible,
            )
        });

    let progress = if duration_ms > 0 {
        (position_ms as f32 / duration_ms as f32).min(1.0)
    } else {
        0.0
    };

    // Video surface area height
    const VIDEO_AREA_HEIGHT: f32 = 220.0;

    // Update native surface position when visible.
    // Position: safe_top + topAppBar(56) + scroll_container_offset(0) + py_4(16)
    if has_player && surface_visible {
        let surface_y = safe_area.top + 56.0 + 16.0; // safe area + app bar + padding
        let surface_x = 16.0; // px_4
        let surface_w = viewport_width - 32.0; // full width minus 2 * px_4

        VIDEO_STATE.with(|s| {
            let st = s.borrow();
            if let Some(ref p) = st.player {
                let _ = p.show_surface(surface_x, surface_y, surface_w, VIDEO_AREA_HEIGHT);
            }
        });
    }

    div()
        .flex()
        .flex_col()
        .w_full()
        .gap_4()
        .px_4()
        .py_4()
        // ── Native video surface placeholder ────────────
        .when(has_player, |d| {
            d.child(
                div()
                    .w_full()
                    .h(px(VIDEO_AREA_HEIGHT))
                    .rounded_xl()
                    .bg(rgb(0x000000))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!surface_visible, |d| {
                        d.child(
                            div()
                                .text_sm()
                                .text_color(rgb(sub_text))
                                .child("Tap to show video"),
                        )
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|_this, _, _, cx| {
                            VIDEO_STATE.with(|s| {
                                let mut st = s.borrow_mut();
                                if !st.surface_visible {
                                    st.surface_visible = true;
                                } else {
                                    // Toggle off
                                    if let Some(ref p) = st.player {
                                        let _ = p.hide_surface();
                                    }
                                    st.surface_visible = false;
                                }
                            });
                            cx.notify();
                        }),
                    ),
            )
        })
        // ── Video list ──────────────────────────────────
        .child(
            div()
                .text_sm()
                .text_color(rgb(sub_text))
                .child("Select a video"),
        )
        .child({
            let mut list = div().flex().flex_col().gap_2();
            for (i, (name, _url)) in VIDEOS.iter().enumerate() {
                let is_current = i == current_video;
                let accent = if is_current { MAUVE } else { card_bg };
                let idx = i;
                list = list.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .p_3()
                        .rounded_xl()
                        .bg(rgb(card_bg))
                        .border_l_4()
                        .border_color(rgb(accent))
                        .child(
                            div()
                                .text_xl()
                                .text_color(rgb(if is_current { MAUVE } else { sub_text }))
                                .child("🎬"),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_base()
                                .text_color(rgb(if is_current { MAUVE } else { text_color }))
                                .child(name.to_string()),
                        )
                        .when(is_current, |d| {
                            d.child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MAUVE))
                                    .child("NOW"),
                            )
                        })
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |_this, _, _, cx| {
                                load_video(idx, cx);
                            }),
                        ),
                );
            }
            list
        })
        // ── Now playing card ────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .rounded_xl()
                .bg(rgb(card_bg))
                // Video title
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_lg()
                                .text_color(rgb(text_color))
                                .child(VIDEOS[current_video].0.to_string()),
                        ),
                )
                // Video info
                .when(video_width > 0, |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(sub_text))
                                    .child(format!("{}x{}", video_width, video_height)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(sub_text))
                                    .child(format_time(duration_ms)),
                            ),
                    )
                })
                // Status
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(if loading { YELLOW } else { sub_text }))
                                .child(if loading {
                                    "Loading...".to_string()
                                } else if is_playing {
                                    "Playing".to_string()
                                } else if has_player {
                                    "Paused".to_string()
                                } else {
                                    "Select a video".to_string()
                                }),
                        ),
                )
                // Progress bar
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .w_full()
                                .h(px(4.0))
                                .rounded_full()
                                .bg(rgb(if dark { 0x3A3A45 } else { 0xD0D0D8 }))
                                .child(
                                    div()
                                        .h(px(4.0))
                                        .rounded_full()
                                        .bg(rgb(MAUVE))
                                        .w(px(progress * 300.0)),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_between()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(sub_text))
                                        .child(format_time(position_ms)),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(sub_text))
                                        .child(format_time(duration_ms)),
                                ),
                        ),
                )
                // Playback controls
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap_6()
                        // Previous
                        .child(
                            div()
                                .text_2xl()
                                .text_color(rgb(text_color))
                                .child("⏮")
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        let prev = VIDEO_STATE.with(|s| {
                                            let st = s.borrow();
                                            if st.current_video == 0 { VIDEOS.len() - 1 } else { st.current_video - 1 }
                                        });
                                        load_video(prev, cx);
                                    }),
                                ),
                        )
                        // Rewind 10s
                        .child(
                            div()
                                .text_xl()
                                .text_color(rgb(text_color))
                                .child("-10s")
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        VIDEO_STATE.with(|s| {
                                            let st = s.borrow();
                                            if let Some(ref p) = st.player {
                                                let pos = st.position_ms.saturating_sub(10_000);
                                                let _ = p.seek(pos);
                                            }
                                        });
                                        cx.notify();
                                    }),
                                ),
                        )
                        // Play/Pause
                        .child(
                            div()
                                .size(px(56.0))
                                .rounded_full()
                                .bg(rgb(MAUVE))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .text_2xl()
                                        .text_color(rgb(0xFFFFFF))
                                        .child(if is_playing { "⏸" } else { "▶" }),
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        VIDEO_STATE.with(|s| {
                                            let st = s.borrow();
                                            if let Some(ref p) = st.player {
                                                if p.is_playing().unwrap_or(false) {
                                                    let _ = p.pause();
                                                } else {
                                                    let _ = p.play();
                                                }
                                            }
                                        });
                                        cx.notify();
                                    }),
                                ),
                        )
                        // Forward 10s
                        .child(
                            div()
                                .text_xl()
                                .text_color(rgb(text_color))
                                .child("+10s")
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        VIDEO_STATE.with(|s| {
                                            let st = s.borrow();
                                            if let Some(ref p) = st.player {
                                                let pos = (st.position_ms + 10_000).min(st.duration_ms);
                                                let _ = p.seek(pos);
                                            }
                                        });
                                        cx.notify();
                                    }),
                                ),
                        )
                        // Next
                        .child(
                            div()
                                .text_2xl()
                                .text_color(rgb(text_color))
                                .child("⏭")
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        let next = VIDEO_STATE.with(|s| {
                                            let st = s.borrow();
                                            (st.current_video + 1) % VIDEOS.len()
                                        });
                                        load_video(next, cx);
                                    }),
                                ),
                        ),
                ),
        )
        // ── Volume ──────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_4()
                .rounded_xl()
                .bg(rgb(card_bg))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(text_color))
                                .child("Volume"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(sub_text))
                                .child(format!("{}%", (volume * 100.0) as u32)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_center()
                        .gap_3()
                        .child(vol_btn("25%", 0.25, cx))
                        .child(vol_btn("50%", 0.5, cx))
                        .child(vol_btn("75%", 0.75, cx))
                        .child(vol_btn("100%", 1.0, cx)),
                ),
        )
        // ── Speed ───────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_4()
                .rounded_xl()
                .bg(rgb(card_bg))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(text_color))
                                .child("Playback Speed"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(sub_text))
                                .child(format!("{:.1}x", speed)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_center()
                        .gap_3()
                        .child(spd_btn("0.5x", 0.5, cx))
                        .child(spd_btn("1.0x", 1.0, cx))
                        .child(spd_btn("1.5x", 1.5, cx))
                        .child(spd_btn("2.0x", 2.0, cx)),
                ),
        )
        // ── Loop toggle ─────────────────────────────────
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .p_4()
                .rounded_xl()
                .bg(rgb(card_bg))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(text_color))
                        .child("Loop"),
                )
                .child(
                    div()
                        .px_4()
                        .py_1()
                        .rounded_lg()
                        .bg(rgb(if looping { GREEN } else { 0x3A3A45 }))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xFFFFFF))
                                .child(if looping { "ON" } else { "OFF" }),
                        )
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|_this, _, _, cx| {
                                VIDEO_STATE.with(|s| {
                                    let mut st = s.borrow_mut();
                                    st.looping = !st.looping;
                                    if let Some(ref p) = st.player {
                                        let _ = p.set_looping(st.looping);
                                    }
                                });
                                cx.notify();
                            }),
                        ),
                ),
        )
        // ── Error display ───────────────────────────────
        .when(error.is_some(), |d| {
            d.child(
                div()
                    .p_3()
                    .rounded_xl()
                    .bg(rgb(0x3D1111))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(RED))
                            .child(error.unwrap_or_default()),
                    ),
            )
        })
}

fn vol_btn(
    label: &str,
    vol: f32,
    cx: &mut gpui::Context<Router>,
) -> impl IntoElement {
    div()
        .px_3()
        .py_1()
        .rounded_lg()
        .bg(rgb(BLUE))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xFFFFFF))
                .child(label.to_string()),
        )
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |_this, _, _, cx| {
                VIDEO_STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.volume = vol;
                    if let Some(ref p) = st.player {
                        let _ = p.set_volume(vol);
                    }
                });
                cx.notify();
            }),
        )
}

fn spd_btn(
    label: &str,
    spd: f32,
    cx: &mut gpui::Context<Router>,
) -> impl IntoElement {
    div()
        .px_3()
        .py_1()
        .rounded_lg()
        .bg(rgb(MAUVE))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xFFFFFF))
                .child(label.to_string()),
        )
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |_this, _, _, cx| {
                VIDEO_STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.speed = spd;
                    if let Some(ref p) = st.player {
                        let _ = p.set_speed(spd);
                    }
                });
                cx.notify();
            }),
        )
}

fn load_video(index: usize, cx: &mut gpui::Context<Router>) {
    let url = VIDEOS[index].1;
    VIDEO_STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.current_video = index;
        st.loading = true;
        st.error = None;
        st.position_ms = 0;
        st.duration_ms = 0;
        st.video_width = 0;
        st.video_height = 0;

        // Create player if needed
        if st.player.is_none() {
            match VideoPlayer::new() {
                Ok(p) => st.player = Some(p),
                Err(e) => {
                    st.error = Some(format!("Failed to create player: {}", e));
                    st.loading = false;
                    return;
                }
            }
        }

        let vol = st.volume;
        let spd = st.speed;
        let lp = st.looping;
        if let Some(player) = st.player.take() {
            match player.set_url(url) {
                Ok(info) => {
                    st.duration_ms = info.duration_ms;
                    st.video_width = info.width;
                    st.video_height = info.height;
                    st.loading = false;
                    let _ = player.set_volume(vol);
                    let _ = player.set_speed(spd);
                    let _ = player.set_looping(lp);
                    let _ = player.play();
                    st.surface_visible = true;
                }
                Err(e) => {
                    st.error = Some(format!("Failed to load: {}", e));
                    st.loading = false;
                }
            }
            st.player = Some(player);
        }
    });

    start_position_polling(cx);
    cx.notify();
}

fn start_position_polling(cx: &mut gpui::Context<Router>) {
    let already_polling = VIDEO_STATE.with(|s| s.borrow().polling);
    if already_polling {
        return;
    }
    VIDEO_STATE.with(|s| s.borrow_mut().polling = true);

    cx.spawn(async |this, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;

            let should_stop = VIDEO_STATE.with(|s| {
                let st = s.borrow();
                if let Some(ref p) = st.player {
                    let pos = p.position().ok();
                    let dur = p.duration().ok();
                    drop(st);
                    let mut st = s.borrow_mut();
                    if let Some(pos) = pos {
                        st.position_ms = pos;
                    }
                    if let Some(dur) = dur {
                        if dur > 0 {
                            st.duration_ms = dur;
                        }
                    }
                    false
                } else {
                    true
                }
            });

            let update_ok = this.update(cx, |_this, cx| {
                cx.notify();
            });

            if should_stop || update_ok.is_err() {
                VIDEO_STATE.with(|s| s.borrow_mut().polling = false);
                break;
            }
        }
    })
    .detach();
}
