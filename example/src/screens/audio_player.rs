//! Audio player screen with playback controls, seek bar, volume, speed & loop mode.

use std::cell::RefCell;
use std::time::Duration;

use gpui::{div, prelude::*, px, rgb};
use gpui_mobile::packages::audio::{AudioPlayer, LoopMode, PlayerState};

use super::{Router, BLUE, GREEN, LIGHT_CARD_BG, LIGHT_SUBTEXT, LIGHT_TEXT, MAUVE, RED, SURFACE0, SUBTEXT, TEXT, YELLOW};

/// Sample audio tracks for demo.
const TRACKS: &[(&str, &str)] = &[
    ("Chill Beat", "https://www.soundhelix.com/examples/mp3/SoundHelix-Song-1.mp3"),
    ("Acoustic", "https://www.soundhelix.com/examples/mp3/SoundHelix-Song-2.mp3"),
    ("Electronica", "https://www.soundhelix.com/examples/mp3/SoundHelix-Song-3.mp3"),
    ("Jazz Vibes", "https://www.soundhelix.com/examples/mp3/SoundHelix-Song-4.mp3"),
];

pub(crate) struct AudioState {
    player: Option<AudioPlayer>,
    current_track: usize,
    duration_ms: u64,
    position_ms: u64,
    volume: f32,
    speed: f32,
    loop_mode: LoopMode,
    loading: bool,
    error: Option<String>,
    polling: bool,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            player: None,
            current_track: 0,
            duration_ms: 0,
            position_ms: 0,
            volume: 1.0,
            speed: 1.0,
            loop_mode: LoopMode::Off,
            loading: false,
            error: None,
            polling: false,
        }
    }
}

thread_local! {
    pub(crate) static AUDIO_STATE: RefCell<AudioState> = RefCell::new(AudioState::default());
}

pub fn reset_state() {
    AUDIO_STATE.with(|s| {
        let mut st = s.borrow_mut();
        // Drop the player (Drop impl calls dispose)
        st.player.take();
        *st = AudioState::default();
    });
}

fn player_state_label(state: &PlayerState) -> &'static str {
    match state {
        PlayerState::Idle => "Idle",
        PlayerState::Loading => "Loading",
        PlayerState::Ready => "Ready",
        PlayerState::Playing => "Playing",
        PlayerState::Paused => "Paused",
        PlayerState::Completed => "Completed",
    }
}

fn format_time(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{}:{:02}", mins, secs)
}

pub fn render(router: &Router, cx: &mut gpui::Context<Router>) -> impl IntoElement {
    let dark = router.dark_mode;
    let text_color = if dark { TEXT } else { LIGHT_TEXT };
    let sub_text = if dark { SUBTEXT } else { LIGHT_SUBTEXT };
    let card_bg = if dark { SURFACE0 } else { LIGHT_CARD_BG };

    let (position_ms, duration_ms, volume, speed, loop_mode_idx, current_track, loading, error, _has_player, player_state_str) =
        AUDIO_STATE.with(|s| {
            let st = s.borrow();
            let ps = st.player.as_ref()
                .and_then(|p| p.state().ok())
                .unwrap_or(PlayerState::Idle);
            let lm = match st.loop_mode {
                LoopMode::Off => 0u8,
                LoopMode::One => 1,
                LoopMode::All => 2,
            };
            (
                st.position_ms,
                st.duration_ms,
                st.volume,
                st.speed,
                lm,
                st.current_track,
                st.loading,
                st.error.clone(),
                st.player.is_some(),
                player_state_label(&ps).to_string(),
            )
        });

    let is_playing = player_state_str == "Playing";
    let progress = if duration_ms > 0 {
        (position_ms as f32 / duration_ms as f32).min(1.0)
    } else {
        0.0
    };

    div()
        .flex()
        .flex_col()
        .w_full()
        .gap_4()
        .px_4()
        .py_4()
        // ── Track list ──────────────────────────────────
        .child(
            div()
                .text_sm()
                .text_color(rgb(sub_text))
                .child("Select a track"),
        )
        .child({
            let mut list = div().flex().flex_col().gap_2();
            for (i, (name, _url)) in TRACKS.iter().enumerate() {
                let is_current = i == current_track;
                let accent = if is_current { BLUE } else { card_bg };
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
                                .text_color(rgb(if is_current { BLUE } else { sub_text }))
                                .child("♫"),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_base()
                                .text_color(rgb(if is_current { BLUE } else { text_color }))
                                .child(name.to_string()),
                        )
                        .when(is_current, |d| {
                            d.child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(BLUE))
                                    .child("NOW"),
                            )
                        })
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |_this, _, _, cx| {
                                load_track(idx, cx);
                            }),
                        ),
                );
            }
            list
        })
        // ── Now playing ─────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .rounded_xl()
                .bg(rgb(card_bg))
                // Track name
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
                                .child(TRACKS[current_track].0.to_string()),
                        ),
                )
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
                                .child(if loading { "Loading...".to_string() } else { player_state_str.clone() }),
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
                                        .bg(rgb(BLUE))
                                        .w(px(progress * 300.0)),  // approximate width
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
                                        let prev = AUDIO_STATE.with(|s| {
                                            let st = s.borrow();
                                            if st.current_track == 0 { TRACKS.len() - 1 } else { st.current_track - 1 }
                                        });
                                        load_track(prev, cx);
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
                                        AUDIO_STATE.with(|s| {
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
                                .bg(rgb(BLUE))
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
                                        AUDIO_STATE.with(|s| {
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
                                        AUDIO_STATE.with(|s| {
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
                                        let next = AUDIO_STATE.with(|s| {
                                            let st = s.borrow();
                                            (st.current_track + 1) % TRACKS.len()
                                        });
                                        load_track(next, cx);
                                    }),
                                ),
                        ),
                )
                // Stop button
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_center()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(RED))
                                .child("⏹ Stop")
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        AUDIO_STATE.with(|s| {
                                            let st = s.borrow();
                                            if let Some(ref p) = st.player {
                                                let _ = p.stop();
                                            }
                                        });
                                        cx.notify();
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
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(sub_text))
                                .child("🔈"),
                        )
                        .child(volume_bar(volume, dark))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(sub_text))
                                .child("🔊"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_center()
                        .gap_3()
                        .child(volume_btn("25%", 0.25, cx))
                        .child(volume_btn("50%", 0.5, cx))
                        .child(volume_btn("75%", 0.75, cx))
                        .child(volume_btn("100%", 1.0, cx)),
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
                        .child(speed_btn("0.5x", 0.5, cx))
                        .child(speed_btn("1.0x", 1.0, cx))
                        .child(speed_btn("1.5x", 1.5, cx))
                        .child(speed_btn("2.0x", 2.0, cx)),
                ),
        )
        // ── Loop mode ───────────────────────────────────
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
                                .child("Loop Mode"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(MAUVE))
                                .child(match loop_mode_idx {
                                    0 => "Off",
                                    1 => "Repeat One",
                                    _ => "Repeat All",
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_center()
                        .gap_3()
                        .child(loop_btn("Off", LoopMode::Off, loop_mode_idx == 0, cx))
                        .child(loop_btn("One", LoopMode::One, loop_mode_idx == 1, cx))
                        .child(loop_btn("All", LoopMode::All, loop_mode_idx == 2, cx)),
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

fn volume_bar(volume: f32, dark: bool) -> impl IntoElement {
    let bar_bg = if dark { 0x3A3A45 } else { 0xD0D0D8 };
    div()
        .flex_1()
        .h(px(4.0))
        .rounded_full()
        .bg(rgb(bar_bg))
        .child(
            div()
                .h(px(4.0))
                .rounded_full()
                .bg(rgb(GREEN))
                .w(px(volume * 200.0)),
        )
}

fn volume_btn(
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
                AUDIO_STATE.with(|s| {
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

fn speed_btn(
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
                AUDIO_STATE.with(|s| {
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

fn loop_btn(
    label: &str,
    mode: LoopMode,
    active: bool,
    cx: &mut gpui::Context<Router>,
) -> impl IntoElement {
    let bg = if active { GREEN } else { 0x3A3A45 };
    div()
        .px_3()
        .py_1()
        .rounded_lg()
        .bg(rgb(bg))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xFFFFFF))
                .child(label.to_string()),
        )
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |_this, _, _, cx| {
                AUDIO_STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.loop_mode = mode;
                    if let Some(ref p) = st.player {
                        let _ = p.set_loop_mode(mode);
                    }
                });
                cx.notify();
            }),
        )
}

fn load_track(index: usize, cx: &mut gpui::Context<Router>) {
    let url = TRACKS[index].1;
    AUDIO_STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.current_track = index;
        st.loading = true;
        st.error = None;
        st.position_ms = 0;
        st.duration_ms = 0;

        // Create player if needed
        if st.player.is_none() {
            match AudioPlayer::new() {
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
        let lm = st.loop_mode;
        if let Some(player) = st.player.take() {
            match player.set_url(url) {
                Ok(dur) => {
                    st.duration_ms = dur.unwrap_or(0);
                    st.loading = false;
                    let _ = player.set_volume(vol);
                    let _ = player.set_speed(spd);
                    let _ = player.set_loop_mode(lm);
                    let _ = player.play();
                }
                Err(e) => {
                    st.error = Some(format!("Failed to load: {}", e));
                    st.loading = false;
                }
            }
            st.player = Some(player);
        }
    });

    // Start polling for position updates
    start_position_polling(cx);
    cx.notify();
}

fn start_position_polling(cx: &mut gpui::Context<Router>) {
    let already_polling = AUDIO_STATE.with(|s| s.borrow().polling);
    if already_polling {
        return;
    }
    AUDIO_STATE.with(|s| s.borrow_mut().polling = true);

    cx.spawn(async |this, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;

            let should_stop = AUDIO_STATE.with(|s| {
                let st = s.borrow();
                if let Some(ref p) = st.player {
                    let pos = p.position().ok();
                    let dur = p.duration().ok();
                    let idle = matches!(p.state(), Ok(PlayerState::Idle));
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
                    idle
                } else {
                    true
                }
            });

            let update_ok = this.update(cx, |_this, cx| {
                cx.notify();
            });

            if should_stop || update_ok.is_err() {
                AUDIO_STATE.with(|s| s.borrow_mut().polling = false);
                break;
            }
        }
    })
    .detach();
}
