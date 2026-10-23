//! Keeps a touch that lands during a fling from inheriting the fling's axis.
//!
//! GPUI's touch recognizer stops scroll momentum when a new contact begins
//! and treats that contact as a pan from its first pixel — but, up to
//! gpui-pre 0.3.5, it also locks the contact to the axis of the fling it
//! stopped. A quick sideways swipe (across a wide table, or just across text:
//! momentum starts on any fast release) followed by a swipe up therefore
//! produced only horizontal deltas, and the page looked frozen until the
//! finger lifted. The fix belongs in the recognizer (zed#64239: the caught
//! fling's axis is the new contact's own); this guard is what the platform
//! can do about it until that reaches a release.
//!
//! The same catch also swallows control drags: a contact that catches a
//! fling is never offered as a touch drag, so a scrollbar thumb — visible only
//! while its content scrolls and coasts — could not be grabbed until the
//! coasting had been stopped by an earlier touch.
//!
//! The recognizer takes its momentum on *every* `Started`, and a contact that
//! is cancelled while still pending emits nothing. So before a real contact
//! that may land on momentum, the guard relays a synthetic contact that begins
//! and is cancelled at the same point: it stops the momentum (a zero-delta
//! `Started`/`Cancelled` scroll pair the consumers treat as a no-op), and the
//! real contact then begins on an idle recognizer, which offers it as a drag
//! and otherwise decides its axis from the contact's own first movement past
//! the slop. When no momentum is left the synthetic pair is a no-op (a pending
//! contact cancelled emits nothing).
//!
//! The recognizer's momentum is not visible from the platform, so the guard
//! estimates it: momentum can only follow the release of a contact that
//! panned, and no fling in either physics outlives [`MOMENTUM_WINDOW`]. What
//! the guard cannot restore is the catching contact's no-tap rule: a contact
//! that stops a fling and lifts without moving is an ordinary tap here.
//!
//! Main thread only, like everything in GPUI.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use gpui::{px, Pixels, Point, TouchEvent, TouchId, TouchPhase};

/// Longer than any fling the recognizer runs: iOS physics coasts ~3.4 s from
/// its 8000 px/s cap to a stop; Android's spline is shorter.
const MOMENTUM_WINDOW: Duration = Duration::from_secs(4);

/// The recognizer's touch slop: a contact that moved past it panned and may
/// have flung on release; one that did not was a tap or a long press.
const PAN_SLOP: Pixels = px(8.);

/// Relays raw contacts to GPUI, disarming a possible fling before a contact
/// that would otherwise catch it. `next_id` allocates synthetic contact IDs
/// from the same sequence as the platform's real ones.
pub(crate) struct FlingGuard {
    /// Where each live contact began, and whether it has moved past the slop.
    contacts: HashMap<TouchId, Contact>,
    /// Momentum may be running until this instant.
    momentum_until: Option<Instant>,
}

struct Contact {
    start: Point<Pixels>,
    panned: bool,
}

impl FlingGuard {
    pub(crate) fn new() -> Self {
        Self {
            contacts: HashMap::new(),
            momentum_until: None,
        }
    }

    /// Relays `event`, preceded by a synthetic contact when it begins while
    /// momentum may still be running.
    pub(crate) fn relay(
        &mut self,
        event: TouchEvent,
        next_id: impl FnOnce() -> TouchId,
        mut emit: impl FnMut(TouchEvent),
    ) {
        self.relay_at(event, Instant::now(), next_id, &mut emit);
    }

    fn relay_at(
        &mut self,
        event: TouchEvent,
        now: Instant,
        next_id: impl FnOnce() -> TouchId,
        emit: &mut impl FnMut(TouchEvent),
    ) {
        match event.phase {
            TouchPhase::Started => {
                if self.momentum_until.take().is_some_and(|until| now < until) {
                    let id = next_id();
                    for phase in [TouchPhase::Started, TouchPhase::Cancelled] {
                        emit(TouchEvent {
                            id,
                            phase,
                            position: event.position,
                            predicted_position: None,
                            force: None,
                        });
                    }
                }
                self.contacts.insert(
                    event.id,
                    Contact {
                        start: event.position,
                        panned: false,
                    },
                );
            }
            TouchPhase::Moved => {
                if let Some(contact) = self.contacts.get_mut(&event.id) {
                    if !contact.panned {
                        let travelled = event.position - contact.start;
                        contact.panned = travelled.magnitude() > f64::from(PAN_SLOP);
                    }
                }
            }
            TouchPhase::Ended => {
                if self
                    .contacts
                    .remove(&event.id)
                    .is_some_and(|contact| contact.panned)
                {
                    self.momentum_until = Some(now + MOMENTUM_WINDOW);
                }
            }
            // A cancelled pan never flings.
            TouchPhase::Cancelled => {
                self.contacts.remove(&event.id);
            }
        }
        emit(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::point;

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> TouchEvent {
        TouchEvent {
            id: TouchId(id),
            phase,
            position: point(px(x), px(y)),
            predicted_position: None,
            force: None,
        }
    }

    /// Relays `events` at `now`, returning what reached GPUI as
    /// `(id, phase, position)`.
    fn relay_all(
        guard: &mut FlingGuard,
        now: Instant,
        events: impl IntoIterator<Item = TouchEvent>,
    ) -> Vec<(u64, TouchPhase, Point<Pixels>)> {
        let mut out = Vec::new();
        let mut next_id = 100;
        for event in events {
            guard.relay_at(
                event,
                now,
                || {
                    next_id += 1;
                    TouchId(next_id)
                },
                &mut |event| out.push((event.id.0, event.phase, event.position)),
            );
        }
        out
    }

    fn pan(guard: &mut FlingGuard, now: Instant) {
        let relayed = relay_all(
            guard,
            now,
            [
                touch(1, TouchPhase::Started, 300., 300.),
                touch(1, TouchPhase::Moved, 250., 300.),
                touch(1, TouchPhase::Ended, 200., 300.),
            ],
        );
        assert_eq!(relayed.len(), 3, "a pan is relayed as is");
    }

    #[test]
    fn a_contact_after_a_pan_release_is_preceded_by_a_cancelled_contact() {
        let mut guard = FlingGuard::new();
        let now = Instant::now();
        pan(&mut guard, now);

        let relayed = relay_all(
            &mut guard,
            now + Duration::from_millis(200),
            [touch(2, TouchPhase::Started, 200., 300.)],
        );

        assert_eq!(
            relayed,
            [
                (101, TouchPhase::Started, point(px(200.), px(300.))),
                (101, TouchPhase::Cancelled, point(px(200.), px(300.))),
                (2, TouchPhase::Started, point(px(200.), px(300.))),
            ]
        );
    }

    #[test]
    fn the_guard_is_armed_once_per_release() {
        let mut guard = FlingGuard::new();
        let now = Instant::now();
        pan(&mut guard, now);

        let first = relay_all(
            &mut guard,
            now + Duration::from_millis(200),
            [
                touch(2, TouchPhase::Started, 200., 300.),
                touch(2, TouchPhase::Ended, 200., 300.),
            ],
        );
        let second = relay_all(
            &mut guard,
            now + Duration::from_millis(400),
            [touch(3, TouchPhase::Started, 200., 300.)],
        );

        assert_eq!(first.len(), 4);
        assert_eq!(
            second,
            [(3, TouchPhase::Started, point(px(200.), px(300.)))]
        );
    }

    #[test]
    fn a_contact_after_the_momentum_window_is_relayed_as_is() {
        let mut guard = FlingGuard::new();
        let now = Instant::now();
        pan(&mut guard, now);

        let relayed = relay_all(
            &mut guard,
            now + MOMENTUM_WINDOW,
            [touch(2, TouchPhase::Started, 200., 300.)],
        );

        assert_eq!(
            relayed,
            [(2, TouchPhase::Started, point(px(200.), px(300.)))]
        );
    }

    #[test]
    fn a_tap_does_not_arm_the_guard() {
        let mut guard = FlingGuard::new();
        let now = Instant::now();
        relay_all(
            &mut guard,
            now,
            [
                touch(1, TouchPhase::Started, 300., 300.),
                touch(1, TouchPhase::Moved, 303., 302.),
                touch(1, TouchPhase::Ended, 303., 302.),
            ],
        );

        let relayed = relay_all(
            &mut guard,
            now + Duration::from_millis(100),
            [touch(2, TouchPhase::Started, 300., 300.)],
        );

        assert_eq!(
            relayed,
            [(2, TouchPhase::Started, point(px(300.), px(300.)))]
        );
    }

    #[test]
    fn a_cancelled_pan_does_not_arm_the_guard() {
        let mut guard = FlingGuard::new();
        let now = Instant::now();
        relay_all(
            &mut guard,
            now,
            [
                touch(1, TouchPhase::Started, 300., 300.),
                touch(1, TouchPhase::Moved, 200., 300.),
                touch(1, TouchPhase::Cancelled, 200., 300.),
            ],
        );

        let relayed = relay_all(
            &mut guard,
            now + Duration::from_millis(100),
            [touch(2, TouchPhase::Started, 300., 300.)],
        );

        assert_eq!(
            relayed,
            [(2, TouchPhase::Started, point(px(300.), px(300.)))]
        );
    }

    /// The guard's purpose, end to end: GPUI's recognizer never offers a
    /// contact that catches a fling as a touch drag, so a control that claims
    /// drags (a scrollbar thumb) cannot be grabbed while its content coasts.
    #[gpui::test]
    fn a_contact_that_lands_on_a_fling_is_still_offered_as_a_drag(cx: &mut gpui::TestAppContext) {
        use gpui::{
            canvas, div, Context, IntoElement, ParentElement, Render, Styled, TouchDragEvent,
            Window,
        };
        use std::{cell::Cell, rc::Rc};

        /// Claims drags that begin in a strip along the right edge, the way
        /// a scrollbar thumb does; the rest of the window pans.
        struct DragClaimer {
            claimed: Rc<Cell<usize>>,
        }

        const THUMB_LEFT: Pixels = px(90.);

        impl Render for DragClaimer {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let claimed = self.claimed.clone();
                div().size_full().child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            let claimed = claimed.clone();
                            window.on_mouse_event(
                                move |event: &TouchDragEvent, phase, window, cx| {
                                    if phase.bubble()
                                        && event.phase == TouchPhase::Started
                                        && event.start_position.x >= THUMB_LEFT
                                    {
                                        claimed.set(claimed.get() + 1);
                                        window.prevent_default();
                                        cx.stop_propagation();
                                    }
                                },
                            );
                        },
                    )
                    .size_full(),
                )
            }
        }

        for guarded in [false, true] {
            let claimed = Rc::new(Cell::new(0));
            let (_, cx) = cx.add_window_view({
                let claimed = claimed.clone();
                move |_, _| DragClaimer { claimed }
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let mut guard = FlingGuard::new();
            let mut next_id = 100;
            let mut relay = |event: TouchEvent, cx: &mut gpui::VisualTestContext| {
                if guarded {
                    guard.relay(
                        event,
                        || {
                            next_id += 1;
                            TouchId(next_id)
                        },
                        |event| cx.simulate_event(event),
                    );
                } else {
                    cx.simulate_event(event);
                }
            };

            // A fast upward swipe, released with velocity: the recognizer is
            // now coasting.
            relay(touch(1, TouchPhase::Started, 50., 400.), cx);
            for step in 1..=6 {
                relay(
                    touch(1, TouchPhase::Moved, 50., 400. - 40. * step as f32),
                    cx,
                );
            }
            relay(touch(1, TouchPhase::Ended, 50., 160.), cx);
            assert_eq!(claimed.get(), 0, "a swipe is a pan, not a drag");

            // A contact that lands while it coasts.
            relay(touch(2, TouchPhase::Started, 95., 20.), cx);
            relay(touch(2, TouchPhase::Ended, 95., 20.), cx);
            assert_eq!(
                claimed.get(),
                usize::from(guarded),
                "guarded={guarded}: only a guarded contact is offered as a drag"
            );
        }
    }
}
