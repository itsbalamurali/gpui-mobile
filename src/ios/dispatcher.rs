//! iOS task dispatcher using Grand Central Dispatch (GCD).
//!
//! iOS shares the same GCD infrastructure as macOS, so this implementation
//! is nearly identical to the macOS dispatcher. Tasks are scheduled onto
//! libdispatch queues; the main-thread queue is used for UI work and a
//! global high-priority queue is used for background work.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::{
    ffi::c_void,
    ptr::NonNull,
    time::{Duration, Instant},
};

// ---------------------------------------------------------------------------
// libdispatch C types / constants
// ---------------------------------------------------------------------------

type dispatch_queue_t = *mut c_void;
type dispatch_time_t = u64;

const DISPATCH_TIME_NOW: dispatch_time_t = 0;
const DISPATCH_QUEUE_PRIORITY_HIGH: i64 = 2;

// SAFETY: These symbols are provided by libdispatch, which is always linked
// on iOS / macOS via the System framework.
unsafe extern "C" {
    static _dispatch_main_q: c_void;

    fn dispatch_async_f(
        queue: dispatch_queue_t,
        context: *mut c_void,
        work: Option<unsafe extern "C" fn(*mut c_void)>,
    );

    fn dispatch_after_f(
        when: dispatch_time_t,
        queue: dispatch_queue_t,
        context: *mut c_void,
        work: Option<unsafe extern "C" fn(*mut c_void)>,
    );

    fn dispatch_get_global_queue(identifier: i64, flags: u64) -> dispatch_queue_t;

    fn dispatch_time(when: dispatch_time_t, delta: i64) -> dispatch_time_t;
}

/// Returns a pointer to the libdispatch main queue.
pub(crate) fn dispatch_get_main_queue() -> dispatch_queue_t {
    std::ptr::addr_of!(_dispatch_main_q) as *const _ as dispatch_queue_t
}

// ---------------------------------------------------------------------------
// Task timing bookkeeping
// ---------------------------------------------------------------------------

/// A record of when a single async task ran.
#[derive(Clone, Debug)]
pub struct TaskTiming {
    pub location: &'static std::panic::Location<'static>,
    pub start: Instant,
    pub end: Option<Instant>,
}

thread_local! {
    /// Per-thread ring of recent task timings.
    static THREAD_TIMINGS: parking_lot::Mutex<ThreadTimingState> =
        parking_lot::Mutex::new(ThreadTimingState::new());
}

/// Global snapshot of every thread's timings.
static GLOBAL_THREAD_TIMINGS: std::sync::LazyLock<parking_lot::Mutex<Vec<ThreadTaskTimings>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(Vec::new()));

/// Per-thread state kept in `THREAD_TIMINGS`.
pub struct ThreadTimingState {
    pub timings: std::collections::VecDeque<TaskTiming>,
}

impl ThreadTimingState {
    fn new() -> Self {
        Self {
            timings: std::collections::VecDeque::with_capacity(64),
        }
    }
}

/// A snapshot of all task timings for one thread.
#[derive(Clone, Debug)]
pub struct ThreadTaskTimings {
    pub timings: Vec<TaskTiming>,
}

impl ThreadTaskTimings {
    /// Flatten a `Vec<ThreadTimingState>` snapshot into `Vec<ThreadTaskTimings>`.
    pub fn convert(global: &Vec<ThreadTaskTimings>) -> Vec<ThreadTaskTimings> {
        global.clone()
    }
}

// ---------------------------------------------------------------------------
// Runnable wrappers
// ---------------------------------------------------------------------------

/// Opaque metadata stored alongside each queued task.
pub struct RunnableMeta {
    pub location: &'static std::panic::Location<'static>,
}

/// Either a "meta" runnable (carries location info) or a plain compat runnable.
pub enum RunnableVariant {
    Meta(async_task::Runnable<RunnableMeta>),
    Compat(async_task::Runnable<()>),
}

// ---------------------------------------------------------------------------
// IosDispatcher
// ---------------------------------------------------------------------------

/// GPUI platform dispatcher for iOS, built on Grand Central Dispatch.
///
/// Background tasks are sent to a high-priority global queue;
/// main-thread tasks are sent to the libdispatch main queue so they run
/// on the iOS run loop.
pub struct IosDispatcher;

impl IosDispatcher {
    /// Whether the calling thread is the UI / main thread.
    pub fn is_main_thread(&self) -> bool {
        use objc::{class, msg_send, runtime::YES, sel, sel_impl};
        unsafe {
            let is_main: objc::runtime::BOOL = msg_send![class!(NSThread), isMainThread];
            is_main == YES
        }
    }

    /// Schedule a runnable on the global high-priority background queue.
    pub fn dispatch(&self, runnable: RunnableVariant) {
        let (ctx, trampoline) = runnable_to_raw(runnable);
        unsafe {
            dispatch_async_f(
                dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0),
                ctx,
                trampoline,
            );
        }
    }

    /// Schedule a runnable on the main thread queue.
    pub fn dispatch_on_main_thread(&self, runnable: RunnableVariant) {
        let (ctx, trampoline) = runnable_to_raw(runnable);
        unsafe {
            dispatch_async_f(dispatch_get_main_queue(), ctx, trampoline);
        }
    }

    /// Schedule a runnable on the background queue after `duration`.
    pub fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        let (ctx, trampoline) = runnable_to_raw(runnable);
        unsafe {
            let queue = dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0);
            let when = dispatch_time(DISPATCH_TIME_NOW, duration.as_nanos() as i64);
            dispatch_after_f(when, queue, ctx, trampoline);
        }
    }

    /// Snapshot all per-thread timing records.
    pub fn get_all_timings(&self) -> Vec<ThreadTaskTimings> {
        let global = GLOBAL_THREAD_TIMINGS.lock();
        ThreadTaskTimings::convert(&global)
    }

    /// Return the calling thread's task timing records.
    pub fn get_current_thread_timings(&self) -> Vec<TaskTiming> {
        THREAD_TIMINGS.with(|state| {
            let s = state.lock();
            let (s1, s2) = s.timings.as_slices();
            let mut v = Vec::with_capacity(s.timings.len());
            v.extend_from_slice(s1);
            v.extend_from_slice(s2);
            v
        })
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert a `RunnableVariant` into a raw context pointer and trampoline
/// function pointer suitable for `dispatch_async_f`.
fn runnable_to_raw(
    runnable: RunnableVariant,
) -> (*mut c_void, Option<unsafe extern "C" fn(*mut c_void)>) {
    match runnable {
        RunnableVariant::Meta(r) => (
            r.into_raw().as_ptr() as *mut c_void,
            Some(trampoline_meta as unsafe extern "C" fn(*mut c_void)),
        ),
        RunnableVariant::Compat(r) => (
            r.into_raw().as_ptr() as *mut c_void,
            Some(trampoline_compat as unsafe extern "C" fn(*mut c_void)),
        ),
    }
}

/// GCD trampoline for `Runnable<RunnableMeta>` tasks.
unsafe extern "C" fn trampoline_meta(raw: *mut c_void) {
    let task = unsafe {
        async_task::Runnable::<RunnableMeta>::from_raw(NonNull::new_unchecked(raw as *mut ()))
    };

    let location = task.metadata().location;
    record_start(location);
    task.run();
    record_end();
}

/// GCD trampoline for plain `Runnable<()>` tasks.
unsafe extern "C" fn trampoline_compat(raw: *mut c_void) {
    let task =
        unsafe { async_task::Runnable::<()>::from_raw(NonNull::new_unchecked(raw as *mut ())) };

    let location = core::panic::Location::caller();
    record_start(location);
    task.run();
    record_end();
}

#[inline]
fn record_start(location: &'static std::panic::Location<'static>) {
    THREAD_TIMINGS.with(|state| {
        let mut s = state.lock();
        // De-duplicate consecutive identical locations to save memory.
        if s.timings
            .back()
            .is_none_or(|last| last.location != location)
        {
            s.timings.push_back(TaskTiming {
                location,
                start: Instant::now(),
                end: None,
            });
        }
    });
}

#[inline]
fn record_end() {
    let end = Instant::now();
    THREAD_TIMINGS.with(|state| {
        let mut s = state.lock();
        if let Some(last) = s.timings.back_mut() {
            last.end = Some(end);
        }
    });
}
