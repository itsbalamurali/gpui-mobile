//! Android task dispatcher.
//!
//! Mirrors the two-queue model used in `gpui_linux` and `IosDispatcher`:
//!
//! * **Foreground / main-thread tasks** — posted via a pipe-backed `ALooper`
//!   callback so they run on the Android native-activity main thread (the same
//!   thread that owns the `ANativeWindow` and processes input events).
//!
//! * **Background tasks** — dispatched onto a fixed-size Rust thread-pool
//!   backed by `std::thread`.  The pool size defaults to
//!   `std::thread::available_parallelism()`.
//!
//! ## Design notes
//!
//! `ALooper` (NDK) is the native equivalent of GCD's main queue or a Linux
//! `epoll` loop.  We register a pipe file-descriptor with `ALooper_addFd` and
//! write a single byte to wake it up whenever a foreground task is enqueued.
//! The looper callback drains all pending tasks before returning.
//!
//! ## GPUI integration
//!
//! `AndroidDispatcher` implements `gpui::PlatformDispatcher` so it can be
//! used to construct `BackgroundExecutor` and `ForegroundExecutor` instances.

#![allow(unsafe_code)]

use std::{
    collections::VecDeque,
    os::unix::io::RawFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use gpui::{PlatformDispatcher, Priority, RunnableVariant};
use parking_lot::Mutex;

// ── NDK / libc symbols we need ────────────────────────────────────────────────

/// `ALOOPER_POLL_CALLBACK` — the fd was signalled and the callback fired.
#[allow(dead_code)]
const ALOOPER_POLL_CALLBACK: i32 = -2;

/// `ALOOPER_EVENT_INPUT` — data is available to read on the fd.
const ALOOPER_EVENT_INPUT: i32 = 1;

unsafe extern "C" {
    /// Returns the looper associated with the calling thread, or null.
    fn ALooper_forThread() -> *mut libc_looper_opaque;
    fn ALooper_acquire(looper: *mut libc_looper_opaque);
    fn ALooper_release(looper: *mut libc_looper_opaque);
    /// Adds a file-descriptor to the looper.
    fn ALooper_addFd(
        looper: *mut libc_looper_opaque,
        fd: RawFd,
        ident: i32,
        events: i32,
        callback: Option<
            unsafe extern "C" fn(fd: RawFd, events: i32, data: *mut std::ffi::c_void) -> i32,
        >,
        data: *mut std::ffi::c_void,
    ) -> i32;
    /// Removes a file-descriptor from the looper.
    fn ALooper_removeFd(looper: *mut libc_looper_opaque, fd: RawFd) -> i32;
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

/// Shared state between the dispatcher and the looper callback.
struct MainQueue {
    tasks: VecDeque<BoxedTask>,
    /// Write end of the wake pipe.
    write_fd: RawFd,
}

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
/// * Foreground tasks run on the Android main/native thread via `ALooper`.
/// * Background tasks run on a Rust thread-pool.
/// * Delayed tasks are checked on each `tick()` call (driven by the main loop).
pub struct AndroidDispatcher {
    /// Shared task queue + wake pipe for the main thread.
    main_queue: Arc<Mutex<MainQueue>>,
    /// Read end of the wake pipe (owned here for clean shutdown).
    read_fd: RawFd,
    /// Raw pointer to the main-thread `ALooper` (not `Send` — used only on the
    /// main thread).
    looper: *mut libc_looper_opaque,
    /// Background thread-pool.
    pool: ThreadPool,
    /// Delayed background tasks sorted by due time.
    delayed: Mutex<Vec<DelayedTask>>,
    /// Set to `true` once `shutdown()` is called.
    shutdown: AtomicBool,
}

// SAFETY: The `looper` pointer is only ever used on the main thread
// (in `register_with_looper` and `unregister`), except for `ALooper_wake`,
// which is documented as safe from any thread.  The rest of the fields are
// `Send`-safe via `Mutex` / `Arc`.
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
    /// Panics if:
    /// * `pipe(2)` fails.
    /// * `ALooper_forThread()` returns null (i.e. not called on main thread).
    pub fn new() -> Arc<Self> {
        // Create wake pipe.
        let (read_fd, write_fd) = create_pipe().expect("failed to create wake pipe");

        let looper = unsafe { ALooper_forThread() };
        assert!(
            !looper.is_null(),
            "AndroidDispatcher::new() must be called on the Android main thread"
        );

        unsafe { ALooper_acquire(looper) };

        let pool_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .max(2);

        let main_queue = Arc::new(Mutex::new(MainQueue {
            tasks: VecDeque::new(),
            write_fd,
        }));

        let dispatcher = Arc::new(Self {
            main_queue: Arc::clone(&main_queue),
            read_fd,
            looper,
            pool: ThreadPool::new(pool_threads),
            delayed: Mutex::new(Vec::new()),
            shutdown: AtomicBool::new(false),
        });

        // Register the read end of the pipe with the looper so we get a
        // callback whenever a foreground task is enqueued.
        dispatcher.register_with_looper();

        log::debug!("AndroidDispatcher created (pool_threads={})", pool_threads);

        dispatcher
    }

    /// Create a dispatcher without a real `ALooper`.
    ///
    /// Safe to call from any thread (including non-Android host environments
    /// and unit tests).  Foreground tasks accumulate in the queue and must
    /// be drained manually via `flush_main_thread_tasks()`.
    pub fn new_headless() -> Arc<Self> {
        let (read_fd, write_fd) = create_pipe().expect("failed to create wake pipe");

        let pool_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .max(1);

        let main_queue = Arc::new(Mutex::new(MainQueue {
            tasks: VecDeque::new(),
            write_fd,
        }));

        Arc::new(Self {
            main_queue,
            read_fd,
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
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
        let mut q = self.main_queue.lock();
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
        q.tasks.push_back(Box::new(f));
        wake_pipe(q.write_fd);
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
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
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

    /// Interrupts a main loop blocked on the looper so it re-reads
    /// `next_delayed_due()`; a delayed task posted from another thread would
    /// otherwise wait for an unrelated wakeup.
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
    /// Useful in tests or when the looper callback is not set up.
    pub fn flush_main_thread_tasks(&self) {
        loop {
            let task = {
                let mut q = self.main_queue.lock();
                q.tasks.pop_front()
            };
            match task {
                Some(f) => f(),
                None => break,
            }
        }
    }

    /// Stop accepting tasks, cancel queued work and detach the Looper callback.
    ///
    /// Call on the owning native thread before releasing its Looper. After this,
    /// outstanding executor handles may be dropped on other threads.
    pub fn shutdown(&self) {
        if !self.shutdown.swap(true, Ordering::SeqCst) {
            self.unregister_from_looper();
        }
        // Cancellation can enqueue work, so drop runnables outside queue locks.
        let tasks = std::mem::take(&mut self.main_queue.lock().tasks);
        let delayed = std::mem::take(&mut *self.delayed.lock());
        drop((tasks, delayed));
    }

    // ── private helpers ───────────────────────────────────────────────────────

    fn register_with_looper(&self) {
        // We pass `main_queue` as the callback data pointer.  The callback
        // is a free `extern "C"` function that reconstructs the Arc from the
        // raw pointer.
        //
        // SAFETY: the callback owns a queue Arc until shutdown unregisters it
        // on the native thread, before the Activity releases its Looper.
        let data_ptr = Arc::into_raw(Arc::clone(&self.main_queue)) as *mut std::ffi::c_void;

        let ret = unsafe {
            ALooper_addFd(
                self.looper,
                self.read_fd,
                0, // ident — unused when callback is Some
                ALOOPER_EVENT_INPUT,
                Some(looper_callback),
                data_ptr,
            )
        };

        if ret != 1 {
            log::warn!(
                "ALooper_addFd returned {} (expected 1); foreground dispatch may not work",
                ret
            );
        }
    }

    fn unregister_from_looper(&self) {
        if !self.looper.is_null() {
            assert!(
                self.is_main_thread(),
                "shutdown must run on the owning native thread"
            );
            unsafe {
                ALooper_removeFd(self.looper, self.read_fd);
                // Balance the Arc passed to ALooper_addFd, once the owning
                // native thread can no longer enter the callback.
                drop(Arc::from_raw(Arc::as_ptr(&self.main_queue)));
            }
        }
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

impl Drop for AndroidDispatcher {
    fn drop(&mut self) {
        if !self.shutdown.swap(true, Ordering::SeqCst) {
            self.unregister_from_looper();
        }
        if !self.looper.is_null() {
            unsafe { ALooper_release(self.looper) };
        }
        // Close file descriptors.
        unsafe {
            libc_close(self.read_fd);
            libc_close(self.main_queue.lock().write_fd);
        }
    }
}

// ── ALooper callback ──────────────────────────────────────────────────────────

/// Called by `ALooper` on the main thread when the wake pipe becomes readable.
///
/// Drains the entire task queue before returning 1 (keep the fd registered).
unsafe extern "C" fn looper_callback(fd: RawFd, _events: i32, data: *mut std::ffi::c_void) -> i32 {
    // Keep an independent reference while executing tasks. A task may release
    // the last dispatcher handle and unregister the callback reentrantly.
    let queue_ptr = data as *const Mutex<MainQueue>;
    unsafe { Arc::increment_strong_count(queue_ptr) };
    let queue_arc = unsafe { Arc::from_raw(queue_ptr) };

    // Drain the wake pipe (ignore errors — just consume the byte(s)).
    let mut buf = [0u8; 64];
    loop {
        let n = unsafe { libc_read(fd, buf.as_mut_ptr() as *mut std::ffi::c_void, buf.len()) };
        if n <= 0 {
            break;
        }
    }

    // Run all queued tasks.
    loop {
        let task = {
            let mut q = queue_arc.lock();
            q.tasks.pop_front()
        };
        match task {
            Some(f) => f(),
            None => break,
        }
    }

    1 // keep fd registered
}

// ── pipe helpers ──────────────────────────────────────────────────────────────

fn create_pipe() -> std::io::Result<(RawFd, RawFd)> {
    let mut fds = [0i32; 2];
    let ret = unsafe { libc_pipe(fds.as_mut_ptr()) };
    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // Set both ends non-blocking so we never block in the callback.
    set_nonblocking(fds[0]);
    set_nonblocking(fds[1]);
    Ok((fds[0], fds[1]))
}

fn set_nonblocking(fd: RawFd) {
    unsafe {
        let flags = libc_fcntl(fd, LIBC_F_GETFL, 0);
        if flags >= 0 {
            libc_fcntl(fd, LIBC_F_SETFL, flags | LIBC_O_NONBLOCK);
        }
    }
}

/// Write a single wake byte to the pipe.
fn wake_pipe(write_fd: RawFd) {
    let buf = [1u8; 1];
    unsafe {
        libc_write(write_fd, buf.as_ptr() as *const std::ffi::c_void, 1);
    }
}

// ── minimal libc bindings ─────────────────────────────────────────────────────
// We declare only the symbols we actually use, keeping the dependency surface
// small.  The `libc` crate could be used here instead, but this avoids adding
// a workspace dependency just for this module.

const LIBC_F_GETFL: i32 = 3;
const LIBC_F_SETFL: i32 = 4;
const LIBC_O_NONBLOCK: i32 = 0o4000;

unsafe extern "C" {
    fn pipe(pipefd: *mut i32) -> i32;
    fn read(fd: i32, buf: *mut std::ffi::c_void, count: usize) -> isize;
    fn write(fd: i32, buf: *const std::ffi::c_void, count: usize) -> isize;
    fn close(fd: i32) -> i32;
    fn fcntl(fd: i32, cmd: i32, ...) -> i32;
}

#[inline(always)]
unsafe fn libc_pipe(fds: *mut i32) -> i32 {
    unsafe { pipe(fds) }
}
#[inline(always)]
unsafe fn libc_read(fd: i32, buf: *mut std::ffi::c_void, count: usize) -> isize {
    unsafe { read(fd, buf, count) }
}
#[inline(always)]
unsafe fn libc_write(fd: i32, buf: *const std::ffi::c_void, count: usize) -> isize {
    unsafe { write(fd, buf, count) }
}
#[inline(always)]
unsafe fn libc_close(fd: i32) -> i32 {
    unsafe { close(fd) }
}
#[inline(always)]
unsafe fn libc_fcntl(fd: i32, cmd: i32, arg: i32) -> i32 {
    unsafe { fcntl(fd, cmd, arg) }
}

// ── impl PlatformDispatcher ───────────────────────────────────────────────────

impl PlatformDispatcher for AndroidDispatcher {
    fn is_main_thread(&self) -> bool {
        // Delegate to the existing `is_main_thread` method.
        AndroidDispatcher::is_main_thread(self)
    }

    fn dispatch(&self, runnable: RunnableVariant, _priority: Priority) {
        AndroidDispatcher::dispatch(self, move || {
            runnable.run();
        });
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, _priority: Priority) {
        AndroidDispatcher::dispatch_on_main_thread(self, move || {
            runnable.run();
        });
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        AndroidDispatcher::dispatch_after(self, duration, move || {
            runnable.run();
        });
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
        // Build a dispatcher without a real looper by constructing state manually.
        let (read_fd, write_fd) = create_pipe().unwrap();

        let main_queue = Arc::new(Mutex::new(MainQueue {
            tasks: VecDeque::new(),
            write_fd,
        }));

        let dispatcher = AndroidDispatcher {
            main_queue,
            read_fd,
            looper: std::ptr::null_mut(),
            pool: ThreadPool::new(1),
            delayed: Mutex::new(Vec::new()),
            shutdown: AtomicBool::new(false),
        };

        let ran = Arc::new(AtomicBool::new(false));
        let ran2 = Arc::clone(&ran);

        dispatcher.dispatch_after(Duration::from_secs(60), move || {
            ran2.store(true, Ordering::Relaxed);
        });

        dispatcher.tick();
        assert!(!ran.load(Ordering::Relaxed), "task should not run yet");

        // Headless dispatchers now support normal shutdown and Drop.
        dispatcher.shutdown();
    }

    #[test]
    fn shutdown_releases_queued_tasks_and_is_idempotent() {
        let dispatcher = AndroidDispatcher::new_headless();
        let payload = Arc::new(());
        let foreground = Arc::clone(&payload);
        let delayed = Arc::clone(&payload);
        dispatcher.dispatch_on_main_thread(move || drop(foreground));
        dispatcher.dispatch_after(Duration::from_secs(60), move || drop(delayed));
        assert_eq!(Arc::strong_count(&payload), 3);

        dispatcher.shutdown();
        dispatcher.shutdown();

        assert_eq!(Arc::strong_count(&payload), 1);
    }

    #[test]
    fn shutdown_rejects_new_foreground_and_delayed_tasks() {
        let dispatcher = AndroidDispatcher::new_headless();
        dispatcher.shutdown();
        let payload = Arc::new(());
        let foreground = Arc::clone(&payload);
        let delayed = Arc::clone(&payload);

        dispatcher.dispatch_on_main_thread(move || drop(foreground));
        dispatcher.dispatch_after(Duration::from_secs(60), move || drop(delayed));

        assert_eq!(Arc::strong_count(&payload), 1);
    }

    /// Verify pipe creation succeeds.
    #[test]
    fn pipe_creation() {
        let (r, w) = create_pipe().expect("pipe");
        unsafe {
            libc_close(r);
            libc_close(w);
        }
    }
}
