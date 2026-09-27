//! Frame demand gated to a vsync source, for hosts that run their own loop
//! (the Android loops in `android::jni` and `android::host`).
//!
//! [`frame_demand::FrameDemand`](crate::frame_demand::FrameDemand) is the
//! shape for a host whose frame source ticks by itself (a `CADisplayLink`):
//! demand resumes the source, the source ticks until a tick finds no demand.
//! A loop that polls an `ALooper` has no source of its own; it wants to know
//! *when* to draw. `FramePacer` answers that: demand posts one vsync callback
//! (`AChoreographer` on Android), the callback marks a frame as due, and the
//! loop draws exactly one frame per marked vsync. Demand raised while a
//! callback is posted or a frame is already due is folded into it, so a
//! window that is notified fifty times between two vsyncs still draws once.
//!
//! Waiting for a vsync only makes sense while the loop keeps up with them.
//! Once the previous frame took a whole interval or longer, the vsync it
//! would wait for has already passed, and waiting for the next one only
//! delays the frame (and the input batched behind it — GPUI stamps touches
//! when they are dispatched, and a gap over 40 ms between samples kills a
//! fling). Such demand is served at once; a loop that is behind draws
//! back-to-back, exactly as it did before pacing, and returns to vsync
//! pacing as soon as a frame fits the interval again. The first frame after
//! an idle period is served at once for the same reason.
//!
//! Without a vsync source (no looper on the thread, tests) the pacer falls
//! back to a wall clock: demand becomes due one frame interval after the
//! previous frame, and [`poll_timeout`](Self::poll_timeout) tells the loop
//! how long it may sleep before then.
//!
//! Main thread only, like everything in GPUI.

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

/// Posts one vsync callback that will call [`FramePacer::on_vsync`].
pub(crate) type PostVsync = Box<dyn Fn()>;

pub(crate) struct FramePacer {
    /// `None` paces by wall clock at `interval`.
    post_vsync: Option<PostVsync>,
    /// The display's refresh interval: how long a frame may take before the
    /// next one stops waiting for a vsync, and the spacing of the fallback
    /// clock.
    interval: Duration,
    /// A vsync callback is posted and has not fired yet.
    posted: Cell<bool>,
    /// A vsync fired (or the fallback clock ran out) and the frame it owes has
    /// not been taken yet.
    due: Cell<bool>,
    /// Fallback only: demand is waiting for this instant.
    fallback_due_at: Cell<Option<Instant>>,
    /// Fallback only: when the previous frame was taken, to space the next.
    last_frame_at: Cell<Option<Instant>>,
}

impl FramePacer {
    /// A pacer driven by a vsync source: `post_vsync` posts one callback that
    /// must end in [`on_vsync`](Self::on_vsync). `interval` is the display's
    /// refresh interval.
    pub(crate) fn with_vsync(post_vsync: PostVsync, interval: Duration) -> Self {
        Self {
            post_vsync: Some(post_vsync),
            interval,
            posted: Cell::new(false),
            due: Cell::new(false),
            fallback_due_at: Cell::new(None),
            last_frame_at: Cell::new(None),
        }
    }

    /// A pacer without a vsync source, spacing frames `interval` apart.
    pub(crate) fn with_clock(interval: Duration) -> Self {
        Self {
            post_vsync: None,
            interval,
            posted: Cell::new(false),
            due: Cell::new(false),
            fallback_due_at: Cell::new(None),
            last_frame_at: Cell::new(None),
        }
    }

    /// Records demand for a frame. The first demand since the last frame
    /// posts a vsync callback; later ones ride on it. Demand that arrives an
    /// interval or more after the last frame began is due at once (see the
    /// module docs).
    pub(crate) fn schedule(&self, now: Instant) {
        if self.posted.get() || self.due.get() || self.fallback_due_at.get().is_some() {
            return;
        }
        let interval_elapsed = self
            .last_frame_at
            .get()
            .is_none_or(|last| now.duration_since(last) >= self.interval);
        match &self.post_vsync {
            Some(_) if interval_elapsed => self.due.set(true),
            Some(post) => {
                self.posted.set(true);
                post();
            }
            None => {
                let earliest = self
                    .last_frame_at
                    .get()
                    .map_or(now, |last| last + self.interval);
                self.fallback_due_at.set(Some(earliest.max(now)));
            }
        }
    }

    /// The posted vsync callback fired: the frame it owes is now due.
    pub(crate) fn on_vsync(&self) {
        self.posted.set(false);
        self.due.set(true);
    }

    /// Takes the due frame, if any. The loop draws when this returns `true`
    /// and only then; a frame left untaken (window inactive) stays due.
    pub(crate) fn take_frame(&self, now: Instant) -> bool {
        if self.due.replace(false) {
            self.last_frame_at.set(Some(now));
            return true;
        }
        match self.fallback_due_at.get() {
            Some(due_at) if due_at <= now => {
                self.fallback_due_at.set(None);
                self.last_frame_at.set(Some(now));
                true
            }
            _ => false,
        }
    }

    /// Whether a frame is wanted: posted, due, or waiting on the fallback
    /// clock.
    #[cfg(test)]
    fn has_demand(&self) -> bool {
        self.posted.get() || self.due.get() || self.fallback_due_at.get().is_some()
    }

    /// Forgets a posted callback (the vsync source may have been paused
    /// while the app was in the background) and posts afresh so demand that
    /// piled up meanwhile is served. A stale callback that fires later marks
    /// one extra frame due, which GPUI answers with a no-op when nothing is
    /// dirty.
    pub(crate) fn resume(&self, now: Instant) {
        self.posted.set(false);
        self.schedule(now);
    }

    /// How long the loop may block before it has something to do: zero while
    /// a frame is due, the time until the fallback frame otherwise, and
    /// `None` when only the vsync callback (delivered through the looper) or
    /// an unrelated wakeup can bring work.
    pub(crate) fn poll_timeout(&self, now: Instant) -> Option<Duration> {
        if self.due.get() {
            return Some(Duration::ZERO);
        }
        self.fallback_due_at
            .get()
            .map(|due_at| due_at.saturating_duration_since(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    const INTERVAL: Duration = Duration::from_millis(16);

    /// A vsync pacer that has just drawn a frame at `now`, so the next
    /// demand is inside the interval and waits for a vsync.
    fn vsync_pacer_after_frame(now: Instant) -> (FramePacer, Rc<Cell<u32>>) {
        let posts = Rc::new(Cell::new(0));
        let counter = Rc::clone(&posts);
        let pacer =
            FramePacer::with_vsync(Box::new(move || counter.set(counter.get() + 1)), INTERVAL);
        pacer.schedule(now);
        assert!(pacer.take_frame(now), "the first frame is served at once");
        (pacer, posts)
    }

    #[test]
    fn first_frame_does_not_wait_for_a_vsync() {
        let posts = Rc::new(Cell::new(0));
        let counter = Rc::clone(&posts);
        let pacer =
            FramePacer::with_vsync(Box::new(move || counter.set(counter.get() + 1)), INTERVAL);
        let now = Instant::now();

        pacer.schedule(now);

        assert_eq!(posts.get(), 0);
        assert_eq!(pacer.poll_timeout(now), Some(Duration::ZERO));
        assert!(pacer.take_frame(now));
    }

    #[test]
    fn demand_posts_one_callback_until_it_fires() {
        let now = Instant::now();
        let (pacer, posts) = vsync_pacer_after_frame(now);
        let now = now + Duration::from_millis(1);

        pacer.schedule(now);
        pacer.schedule(now);
        pacer.schedule(now);

        assert_eq!(posts.get(), 1);
        assert!(pacer.has_demand());
        assert!(!pacer.take_frame(now), "nothing is due before the vsync");
        assert_eq!(
            pacer.poll_timeout(now),
            None,
            "the looper delivers the vsync"
        );
    }

    #[test]
    fn vsync_makes_one_frame_due_and_the_next_demand_posts_again() {
        let now = Instant::now();
        let (pacer, posts) = vsync_pacer_after_frame(now);
        let now = now + Duration::from_millis(1);

        pacer.schedule(now);
        pacer.on_vsync();
        assert_eq!(pacer.poll_timeout(now), Some(Duration::ZERO));
        assert!(pacer.take_frame(now));
        assert!(!pacer.take_frame(now), "a vsync owes exactly one frame");
        assert!(!pacer.has_demand());

        pacer.schedule(now + Duration::from_millis(1));
        assert_eq!(posts.get(), 2);
    }

    #[test]
    fn demand_while_a_frame_is_due_does_not_post() {
        let now = Instant::now();
        let (pacer, posts) = vsync_pacer_after_frame(now);
        let now = now + Duration::from_millis(1);

        pacer.schedule(now);
        pacer.on_vsync();
        pacer.schedule(now);

        assert_eq!(posts.get(), 1);
        assert!(pacer.take_frame(now));
        assert!(!pacer.take_frame(now));
    }

    #[test]
    fn a_frame_that_overran_the_interval_is_served_at_once() {
        let now = Instant::now();
        let (pacer, posts) = vsync_pacer_after_frame(now);

        // The frame taken at `now` took 40 ms; the vsync it would wait for
        // has passed, so the demand it re-raised is due right away.
        let later = now + Duration::from_millis(40);
        pacer.schedule(later);

        assert_eq!(posts.get(), 0);
        assert_eq!(pacer.poll_timeout(later), Some(Duration::ZERO));
        assert!(pacer.take_frame(later));

        // Back within the interval: vsync pacing again.
        pacer.schedule(later + Duration::from_millis(5));
        assert_eq!(posts.get(), 1);
    }

    #[test]
    fn untaken_frame_stays_due_for_an_inactive_window() {
        let now = Instant::now();
        let (pacer, _posts) = vsync_pacer_after_frame(now);
        let now = now + Duration::from_millis(1);

        pacer.schedule(now);
        pacer.on_vsync();
        // The loop skips drawing while inactive …
        assert!(pacer.has_demand());
        // … and draws the owed frame once it is active again.
        assert!(pacer.take_frame(now + Duration::from_secs(5)));
    }

    #[test]
    fn resume_reposts_a_callback_that_may_never_fire() {
        let now = Instant::now();
        let (pacer, posts) = vsync_pacer_after_frame(now);
        let now = now + Duration::from_millis(1);

        pacer.schedule(now);
        assert_eq!(posts.get(), 1);

        pacer.resume(now);
        assert_eq!(posts.get(), 2);

        // The stale callback and the fresh one both fire: still one frame.
        pacer.on_vsync();
        pacer.on_vsync();
        assert!(pacer.take_frame(now));
        assert!(!pacer.take_frame(now));
    }

    #[test]
    fn clock_fallback_spaces_frames_by_the_interval() {
        let pacer = FramePacer::with_clock(INTERVAL);
        let t0 = Instant::now();

        pacer.schedule(t0);
        assert_eq!(pacer.poll_timeout(t0), Some(Duration::ZERO));
        assert!(pacer.take_frame(t0), "the first frame is due at once");

        pacer.schedule(t0 + Duration::from_millis(1));
        assert_eq!(
            pacer.poll_timeout(t0 + Duration::from_millis(1)),
            Some(Duration::from_millis(15))
        );
        assert!(!pacer.take_frame(t0 + Duration::from_millis(10)));
        assert!(pacer.take_frame(t0 + INTERVAL));
        assert!(!pacer.has_demand());
    }

    #[test]
    fn clock_fallback_folds_demand_into_the_pending_frame() {
        let pacer = FramePacer::with_clock(INTERVAL);
        let t0 = Instant::now();

        pacer.schedule(t0);
        pacer.schedule(t0 + Duration::from_millis(5));
        assert!(pacer.take_frame(t0 + Duration::from_millis(5)));
        assert!(!pacer.take_frame(t0 + Duration::from_millis(5)));
    }
}
