//! Android task dispatcher.
//!
//! Mirrors the two-queue model used in `gpui_linux` and `IosDispatcher`:
//!
//! * **Foreground / main-thread tasks** — queued for the Android
//!   native-activity main thread (the same thread that owns the
//!   `ANativeWindow` and processes input events), which runs them from its
//!   event loop.
//!
//! * **Background tasks** — dispatched onto a fixed-size Rust thread-pool
//!   backed by `std::thread`.  The pool size defaults to
//!   `std::thread::available_parallelism()`.
//!
//! ## Design notes
//!
//! `ALooper` (NDK) is the native equivalent of GCD's main queue or a Linux
//! `epoll` loop.  Enqueueing a foreground task calls `ALooper_wake`, so a main
//! loop blocked in `ALooper_pollOnce` returns, and the loop then drains the
//! queue with [`AndroidDispatcher::flush_main_thread_tasks`].
//!
//! The wake carries no callback on purpose. `android-activity` polls this
//! looper itself and logs every source it did not register as an error: a
//! looper callback comes back as `Spurious ALOOPER_POLL_CALLBACK`, and an fd
//! registered with an ident as `Ignoring spurious ALooper event source`.
//! `ALOOPER_POLL_WAKE` is the one result it accepts silently. Running the
//! tasks from the loop also gives them a fixed place in each iteration
//! instead of inside `android-activity`'s poll.
//!
//! ## GPUI integration
//!
//! `AndroidDispatcher` implements `gpui::PlatformDispatcher` so it can be
//! used to construct `BackgroundExecutor` and `ForegroundExecutor` instances.

#![allow(unsafe_code)]

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use gpui::{PlatformDispatcher, Priority, RunnableVariant};
use parking_lot::Mutex;

// ── NDK symbols we need ───────────────────────────────────────────────────────

unsafe extern "C" {
    /// Returns the looper associated with the calling thread, or null.
    fn ALooper_forThread() -> *mut libc_looper_opaque;
    /// Returns from a blocking poll on the looper; safe from any thread.
    fn ALooper_wake(looper: *mut libc_looper_opaque);
}

// Opaque C type placeholder.
#[repr(C)]
struct libc_looper_opaque {
    _priv: [u8; 0],
}

// ── task queue ────────────────────────────────────────────────────────────────

type BoxedTask = Box<dyn FnOnce() + Send + 'static>;

// ── thread-pool ───────────────────────────────────────────────────────────────

/// A minimal fixed-size thread-pool for background tasks.
struct ThreadPool {
    sender: std::sync::mpsc::Sender<BoxedTask>,
}

impl ThreadPool {
    fn new(threads: usize) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel::<BoxedTask>();
        let receiver = Arc::new(Mutex::new(receiver));

        for i in 0..threads {
            let rx = Arc::clone(&receiver);
            std::thread::Builder::new()
                .name(format!("gpui-bg-{}", i))
                .spawn(move || {
                    loop {
                        let task = {
                            let lock = rx.lock();
                            lock.recv()
                        };
                        match task {
                            Ok(f) => f(),
                            Err(_) => break, // channel closed
                        }
                    }
                })
                .expect("failed to spawn background thread");
        }

        ThreadPool { sender }
    }

    fn dispatch(&self, task: BoxedTask) {
        // If the pool is shutting down the send will fail silently.
        let _ = self.sender.send(task);
    }
}

// ── delayed task queue ────────────────────────────────────────────────────────

struct DelayedTask {
    due: Instant,
    task: BoxedTask,
}

// ── AndroidDispatcher ─────────────────────────────────────────────────────────

/// GPUI dispatcher for Android.
///
/// * Foreground tasks run on the Android main/native thread, from the main
///   loop's call to `flush_main_thread_tasks()` after the looper wakes it.
/// * Background tasks run on a Rust thread-pool.
/// * Delayed tasks are checked on each `tick()` call (driven by the main loop).
pub struct AndroidDispatcher {
    /// Tasks waiting to run on the main thread.
    main_queue: Mutex<VecDeque<BoxedTask>>,
    /// Raw pointer to the main-thread `ALooper`: compared in
    /// `is_main_thread` and woken (from any thread) by `wake_looper`.
    looper: *mut libc_looper_opaque,
    /// Background thread-pool.
    pool: ThreadPool,
    /// Delayed background tasks sorted by due time.
    delayed: Mutex<Vec<DelayedTask>>,
    /// Set to `true` once `shutdown()` is called.
    shutdown: AtomicBool,
}

// SAFETY: The `looper` pointer is only compared against `ALooper_forThread()`
// and passed to `ALooper_wake`, which is documented as safe from any thread;
// the looper outlives the dispatcher.  The rest of the fields are `Send`-safe
// via `Mutex`.
unsafe impl Send for AndroidDispatcher {}
unsafe impl Sync for AndroidDispatcher {}

impl AndroidDispatcher {
    /// Create a new dispatcher.
    ///
    /// Must be called on the **main thread** so that `ALooper_forThread()`
    /// returns a valid looper.
    ///
    /// # Panics
    ///
    /// Panics if `ALooper_forThread()` returns null (i.e. not called on the
    /// main thread).
    pub fn new() -> Arc<Self> {
        let looper = unsafe { ALooper_forThread() };
        assert!(
            !looper.is_null(),
            "AndroidDispatcher::new() must be called on the Android main thread"
        );

        let pool_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .max(2);

        let dispatcher = Arc::new(Self {
            main_queue: Mutex::new(VecDeque::new()),
            looper,
            pool: ThreadPool::new(pool_threads),
            delayed: Mutex::new(Vec::new()),
            shutdown: AtomicBool::new(false),
        });

        log::debug!("AndroidDispatcher created (pool_threads={})", pool_threads);

        dispatcher
    }

    /// Create a dispatcher without a real `ALooper`.
    ///
    /// Safe to call from any thread (including non-Android host environments
    /// and unit tests).  Foreground tasks accumulate in the queue and must
    /// be drained manually via `flush_main_thread_tasks()`.
    pub fn new_headless() -> Arc<Self> {
        let pool_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .max(1);

        Arc::new(Self {
            main_queue: Mutex::new(VecDeque::new()),
            looper: std::ptr::null_mut(), // no real looper in headless mode
            pool: ThreadPool::new(pool_threads),
            delayed: Mutex::new(Vec::new()),
            shutdown: AtomicBool::new(false),
        })
    }

    // ── public API ────────────────────────────────────────────────────────────

    /// Returns `true` if the calling thread is the main/UI thread.
    pub fn is_main_thread(&self) -> bool {
        // On Android the main thread is the one that owns the looper we
        // registered with.  `ALooper_forThread()` returns the *same* pointer
        // if we're on that thread, or a *different* (or null) pointer otherwise.
        let current = unsafe { ALooper_forThread() };
        !current.is_null() && current == self.looper
    }

    /// Enqueue a task to run on the **main** (foreground) thread.
    pub fn dispatch_on_main_thread<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.push_main_task(Box::new(f));
    }

    /// Queues `task` for the main thread and wakes its loop. Wakes even when
    /// called on the main thread: a task queued outside
    /// `flush_main_thread_tasks` (from input or rendering) must not wait for
    /// an unrelated wakeup.
    fn push_main_task(&self, task: BoxedTask) {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
        self.main_queue.lock().push_back(task);
        self.wake_looper();
    }

    /// Enqueue a task on the **background** thread-pool.
    pub fn dispatch<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
        self.pool.dispatch(Box::new(f));
    }

    /// Enqueue a task on the **background** thread-pool after `delay`.
    pub fn dispatch_after<F>(&self, delay: Duration, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
        let due = Instant::now() + delay;
        let mut delayed = self.delayed.lock();
        delayed.push(DelayedTask {
            due,
            task: Box::new(f),
        });
        // Keep sorted by ascending due time.
        delayed.sort_by_key(|d| d.due);
        drop(delayed);
        self.wake_looper();
    }

    /// When the earliest delayed task is due, so the main loop can sleep
    /// until then instead of polling `tick()` blindly.
    pub fn next_delayed_due(&self) -> Option<Instant> {
        self.delayed.lock().first().map(|d| d.due)
    }

    /// Interrupts a main loop blocked on the looper so it drains the main
    /// queue and re-reads `next_delayed_due()`; work posted from another
    /// thread would otherwise wait for an unrelated wakeup.
    fn wake_looper(&self) {
        if !self.looper.is_null() {
            // SAFETY: `ALooper_wake` is documented as safe from any thread,
            // and the looper outlives the dispatcher's thread.
            unsafe { ALooper_wake(self.looper) };
        }
    }

    /// Process any delayed background tasks whose due time has passed.
    ///
    /// Should be called from the main loop on every iteration (e.g. just
    /// before calling `ALooper_pollOnce`).
    pub fn tick(&self) {
        let now = Instant::now();
        let mut ready: Vec<BoxedTask> = Vec::new();
        {
            let mut delayed = self.delayed.lock();
            while delayed.first().map(|d| d.due <= now).unwrap_or(false) {
                ready.push(delayed.remove(0).task);
            }
        }
        for task in ready {
            self.pool.dispatch(task);
        }
    }

    /// Drain all pending **main-thread** tasks synchronously.
    ///
    /// The main loop calls this every time the looper wakes it. Tasks queued
    /// while it runs (including by the tasks themselves) run in the same call.
    pub fn flush_main_thread_tasks(&self) {
        loop {
            // The guard is dropped before the task runs, so a task can queue
            // more tasks.
            let task = self.main_queue.lock().pop_front();
            match task {
                Some(f) => f(),
                None => break,
            }
        }
    }

    /// Stop accepting new tasks and signal the thread-pool to wind down.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

impl Default for AndroidDispatcher {
    fn default() -> Self {
        panic!(
            "AndroidDispatcher must be constructed via `AndroidDispatcher::new()` \
             on the main thread"
        );
    }
}

// ── impl PlatformDispatcher ───────────────────────────────────────────────────

impl PlatformDispatcher for AndroidDispatcher {
    fn is_main_thread(&self) -> bool {
        // Delegate to the existing `is_main_thread` method.
        AndroidDispatcher::is_main_thread(self)
    }

    fn dispatch(&self, runnable: RunnableVariant, _priority: Priority) {
        // All non-realtime background tasks go to the thread-pool.
        // Priority-based scheduling is not yet implemented; all tasks are
        // treated equally by the pool.
        self.pool.dispatch(Box::new(move || {
            runnable.run();
        }));
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, _priority: Priority) {
        self.push_main_task(Box::new(move || {
            runnable.run();
        }));
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
        let due = Instant::now() + duration;
        let mut delayed = self.delayed.lock();
        delayed.push(DelayedTask {
            due,
            task: Box::new(move || {
                runnable.run();
            }),
        });
        delayed.sort_by_key(|d| d.due);
        drop(delayed);
        self.wake_looper();
    }

    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        // Spawn a dedicated thread for realtime (audio) tasks, matching the
        // behaviour of the Linux dispatcher.
        std::thread::Builder::new()
            .name("gpui-realtime".to_string())
            .spawn(move || {
                f();
            })
            .expect("failed to spawn realtime thread");
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    /// Verify that the thread-pool executes background tasks.
    #[test]
    fn background_tasks_run() {
        let pool = ThreadPool::new(2);
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..10 {
            let c = Arc::clone(&counter);
            pool.dispatch(Box::new(move || {
                c.fetch_add(1, Ordering::Relaxed);
            }));
        }

        // Give threads time to process.
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(counter.load(Ordering::Relaxed), 10);
    }

    /// Verify that delayed tasks are not dispatched before their due time.
    #[test]
    fn delayed_tasks_not_early() {
        let dispatcher = AndroidDispatcher::new_headless();

        let ran = Arc::new(AtomicBool::new(false));
        let ran2 = Arc::clone(&ran);

        dispatcher.dispatch_after(Duration::from_secs(60), move || {
            ran2.store(true, Ordering::Relaxed);
        });

        dispatcher.tick();
        assert!(!ran.load(Ordering::Relaxed), "task should not run yet");
    }

    /// Main-thread tasks wait in the queue until the loop flushes it, and a
    /// task queued by a running task runs in the same flush.
    #[test]
    fn main_thread_tasks_run_on_flush() {
        let dispatcher = AndroidDispatcher::new_headless();
        let order = Arc::new(Mutex::new(Vec::new()));

        let first = Arc::clone(&order);
        let inner_dispatcher = Arc::clone(&dispatcher);
        dispatcher.dispatch_on_main_thread(move || {
            first.lock().push(1);
            let nested = Arc::clone(&first);
            inner_dispatcher.dispatch_on_main_thread(move || nested.lock().push(3));
        });
        let second = Arc::clone(&order);
        dispatcher.dispatch_on_main_thread(move || second.lock().push(2));

        assert!(order.lock().is_empty(), "nothing runs before the flush");
        dispatcher.flush_main_thread_tasks();
        assert_eq!(*order.lock(), vec![1, 2, 3]);
    }

    /// After `shutdown()` no new main-thread tasks are accepted.
    #[test]
    fn shutdown_drops_new_main_thread_tasks() {
        let dispatcher = AndroidDispatcher::new_headless();
        dispatcher.shutdown();

        let ran = Arc::new(AtomicBool::new(false));
        let ran2 = Arc::clone(&ran);
        dispatcher.dispatch_on_main_thread(move || ran2.store(true, Ordering::Relaxed));
        dispatcher.flush_main_thread_tasks();
        assert!(!ran.load(Ordering::Relaxed));
    }
}
