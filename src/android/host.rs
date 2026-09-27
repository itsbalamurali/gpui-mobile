//! Host-driven Android entry point: a plain Java `Activity` owns a `SurfaceView`,
//! and GPUI runs on a **process-lived** render thread of our own.
//!
//! # Why this exists next to [`super::jni`]
//!
//! The `android-activity` path in `jni.rs` spawns a fresh native thread per Activity
//! and blocks the Java `onDestroy` until that thread stops ([`notify_destroyed`]).
//! A recreated Activity therefore always runs on a *new* thread, while
//! `AndroidDispatcher::new()` captured `ALooper_forThread()` of the *old* one — so
//! GPUI's `App::new_app` trips `assert!(is_main_thread(), "must construct App on main
//! thread")` and the screen goes black with nothing but a log line.
//!
//! Here the render thread is created once and outlives every Activity. Surfaces are
//! handed in and taken back through [`surface_created`] / [`surface_destroyed`], which
//! is the same mechanism the background/foreground cycle already uses
//! (`AndroidWindow::term_window` keeps the renderer and its atlas alive, `init_window`
//! re-attaches a new surface to it).
//!
//! This module is **additive**: the `android-activity` path is untouched, so both
//! entry points can coexist while hosts migrate.
//!
//! # What the host has to do
//!
//! From the Java UI thread, in this order:
//!
//! 1. In `Activity.onCreate` — every time, recreation included — call
//!    [`super::jni::set_host_activity`] from a JNI entry point, then [`start`] (a no-op
//!    after the first call).
//! 2. From `SurfaceHolder.Callback`: [`surface_created`] in both `surfaceCreated` and
//!    `surfaceChanged`, [`surface_destroyed`] in `surfaceDestroyed`.
//! 3. From `onResume` / `onPause`: [`resumed`] / [`paused`].
//! 4. From `onTouchEvent`, `dispatchKeyEvent` and the `InputConnection`:
//!    [`motion_event`], [`key`], [`ime_event`].
//!
//! The Activity must also expose the Java methods `jni.rs` calls back into for the IME
//! (`gpuiShowKeyboard`, `gpuiHideKeyboard`, `gpuiResetComposition`); `GpuiInputActivity`
//! in the example project is the reference implementation.
//!
//! # Threading contract
//!
//! - `render thread` — owns the `ALooper`, the `AndroidPlatform`, GPUI's `App` and the
//!   `ApplicationHandle` that keeps it alive. Everything GPUI touches stays here.
//! - `Java UI thread` — calls the functions below. They only touch the command queue
//!   and a few atomics, never GPUI state.
//! - [`surface_destroyed`] **blocks** until the render thread has stopped using the
//!   surface. That wait is mandatory: destroying a `Surface` while a thread sits inside
//!   `ANativeWindow_lock` is a use-after-free.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

use gpui::{App, Application, ApplicationHandle};
use ndk::native_window::NativeWindow;

use super::platform::{AndroidPlatform, SharedPlatform};
use crate::android::{AndroidKeyEvent, TouchPoint};

/// The render thread's `ALooper`, so other threads can wake it.
static LOOPER: Mutex<Option<LooperPtr>> = Mutex::new(None);

/// Commands posted from the Java UI thread, drained on the render thread.
static COMMANDS: Mutex<Vec<Command>> = Mutex::new(Vec::new());

/// Set once the render thread is running; guards against a second spawn.
static STARTED: OnceLock<()> = OnceLock::new();

/// Cleared by the render thread once it has released the outgoing surface.
static SURFACE_RELEASED: AtomicBool = AtomicBool::new(true);

/// Scale factor of the current surface (`f32` bits), for the platform-view hit test
/// in [`motion_event`], which runs on the Java UI thread and must not touch the window.
static SCALE_BITS: AtomicU32 = AtomicU32::new(0);

/// The `ANativeWindow` the renderer is currently attached to.
///
/// `SurfaceHolder.Callback` fires `surfaceCreated` **and** `surfaceChanged` for the
/// same `Surface`, and Vulkan allows only one surface per `ANativeWindow`: building a
/// second one fails with `ERROR_NATIVE_WINDOW_IN_USE_KHR`. So a repeat of the window
/// we already hold is treated as a resize, not as a re-attach.
static CURRENT_SURFACE: AtomicPtr<ndk_sys::ANativeWindow> = AtomicPtr::new(std::ptr::null_mut());

struct LooperPtr(*mut ndk_sys::ALooper);
// SAFETY: `ALooper_wake` is documented as safe to call from any thread.
unsafe impl Send for LooperPtr {}

enum Command {
    SurfaceCreated {
        window: NativeWindow,
        scale: f32,
    },
    SurfaceDestroyed,
    Resumed,
    Paused,
    /// Input arrives on the Java UI thread but GPUI may only be touched from the
    /// render thread, so both go through the queue like everything else.
    Touch(TouchPoint),
    Key(AndroidKeyEvent),
}

/// `MotionEvent.getActionMasked()` values.
const ACTION_DOWN: u32 = 0;
const ACTION_UP: u32 = 1;
const ACTION_MOVE: u32 = 2;
const ACTION_CANCEL: u32 = 3;
const ACTION_POINTER_DOWN: u32 = 5;
const ACTION_POINTER_UP: u32 = 6;

/// One pointer of a `MotionEvent`, in physical pixels relative to the surface.
#[derive(Debug, Clone, Copy)]
pub struct Pointer {
    /// `MotionEvent.getPointerId(i)`.
    pub id: i32,
    pub x: f32,
    pub y: f32,
}

type Launch = Box<dyn FnOnce(&mut App) + Send>;

/// GPUI's application, owned by the render thread.
struct HostApp {
    /// Consumed when the first surface arrives.
    launch: Option<Launch>,
    /// Keeps the `App` alive: `Platform::run` returns immediately on this path, so
    /// nothing else holds the `Rc<AppCell>`.
    handle: Option<ApplicationHandle>,
}

fn post(command: Command) {
    COMMANDS.lock().expect("poisoned").push(command);
    wake_render_thread();
}

fn wake_render_thread() {
    if let Some(looper) = LOOPER.lock().expect("poisoned").as_ref() {
        // SAFETY: the pointer stays valid for the lifetime of the render thread,
        // which is the lifetime of the process.
        unsafe { ndk_sys::ALooper_wake(looper.0) };
    }
}

/// Start the render thread. Idempotent — later calls are no-ops, which is what makes
/// an Activity recreation cheap: the thread, the platform and the GPUI `App` all survive.
///
/// `launch` runs **on the render thread** once the first surface is available, with
/// the same `&mut App` an `Application::run` closure gets. The application is built
/// with `Application::run_embedded` and kept alive by this module.
pub fn start<F>(launch: F)
where
    F: FnOnce(&mut App) + Send + 'static,
{
    let mut launch: Option<Launch> = Some(Box::new(launch));
    STARTED.get_or_init(move || {
        std::thread::Builder::new()
            .name("gpui-main".into())
            .spawn(move || render_thread(launch.take().expect("launch closure")))
            .expect("spawn gpui-main");
    });
}

fn render_thread(launch: Launch) {
    // SAFETY: called once, on this thread, before anything registers with the looper.
    let looper = unsafe { ndk_sys::ALooper_prepare(0) };
    assert!(
        !looper.is_null(),
        "ALooper_prepare failed; AndroidDispatcher requires a looper on this thread"
    );
    *LOOPER.lock().expect("poisoned") = Some(LooperPtr(looper));
    log::info!("gpui-main: render thread started, looper={looper:p}");

    // The platform must be built **here**: `AndroidDispatcher::new()` captures
    // `ALooper_forThread()`, and GPUI compares it against the calling thread forever after.
    let platform = Arc::new(AndroidPlatform::new(false));
    super::jni::set_host_platform(Arc::clone(&platform));

    let mut app = HostApp {
        launch: Some(launch),
        handle: None,
    };

    loop {
        // Drain into a local first: a `for` over `COMMANDS.lock()…` would keep the
        // guard alive for the whole loop body (temporaries in the iterator expression
        // live until the loop ends), blocking every `post()` from the Java UI thread
        // while surfaces are rebuilt or the UI is first constructed.
        let commands: Vec<Command> = COMMANDS.lock().expect("poisoned").drain(..).collect();
        for command in commands {
            match command {
                Command::SurfaceCreated { window, scale } => {
                    on_surface_created(&platform, window, scale, &mut app);
                }
                Command::SurfaceDestroyed => {
                    if let Some(win) = platform.primary_window() {
                        // Keeps the renderer (and its atlas) alive; only the surface goes.
                        win.term_window();
                    }
                    CURRENT_SURFACE.store(std::ptr::null_mut(), Ordering::SeqCst);
                    SURFACE_RELEASED.store(true, Ordering::SeqCst);
                }
                Command::Resumed => {
                    platform.did_become_active();
                    if let Some(win) = platform.primary_window() {
                        win.set_active(true);
                    }
                }
                Command::Paused => {
                    platform.did_enter_background();
                    if let Some(win) = platform.primary_window() {
                        win.set_active(false);
                    }
                }
                Command::Touch(point) => match platform.primary_window() {
                    Some(win) => win.handle_touch(point),
                    None => log::warn!("gpui-main: touch dropped — no window"),
                },
                Command::Key(event) => {
                    if let Some(win) = platform.primary_window() {
                        win.handle_key_event(event);
                    }
                }
            }
        }

        platform.tick();
        platform.flush_main_thread_tasks();
        if let Some(win) = platform.primary_window() {
            if win.is_active() {
                win.request_frame();
            }
        }

        // Sleep on the looper until woken by a command or a dispatcher task.
        // The short timeout keeps animations ticking; a demand-driven version can
        // drop it once frame scheduling moves to `AChoreographer`.
        // SAFETY: called only from the thread that owns this looper.
        unsafe {
            ndk_sys::ALooper_pollOnce(
                8,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
    }
}

fn on_surface_created(
    platform: &Arc<AndroidPlatform>,
    window: NativeWindow,
    scale: f32,
    app: &mut HostApp,
) {
    SURFACE_RELEASED.store(false, Ordering::SeqCst);

    let incoming = window.ptr().as_ptr();
    if CURRENT_SURFACE.load(Ordering::SeqCst) == incoming {
        // `surfaceChanged` for a surface we are already rendering into: the size may
        // have changed, but the Vulkan surface must not be rebuilt.
        if let Some(existing) = platform.primary_window() {
            existing.handle_resize();
        }
        return;
    }

    if let Some(existing) = platform.primary_window() {
        // Recreated Activity (or a resumed one): re-attach the new surface to the
        // renderer we already have. GPUI's scene cache and the texture atlas survive.
        let gpu = platform.gpu_context();
        match existing.init_window(window, gpu) {
            Ok(()) => {
                CURRENT_SURFACE.store(incoming, Ordering::SeqCst);
                log::info!("gpui-main: surface re-attached to existing window");
                // The Java `InputProxy` died with the old Activity while GPUI still
                // considers the same field focused; ask the new Activity for the IME.
                super::jni::restore_keyboard();
            }
            Err(err) => log::error!("gpui-main: init_window failed: {err:#}"),
        }
        existing.set_active(true);
        return;
    }

    // First surface: create the window, then let the host build its UI.
    match platform.open_window(window, scale, false) {
        Ok(win) => {
            CURRENT_SURFACE.store(incoming, Ordering::SeqCst);
            win.set_active(true);
            log::info!("gpui-main: first window opened");
            if let Some(launch) = app.launch.take() {
                // Without an `AndroidApp` to drive, `AndroidPlatform::run` invokes the
                // callback immediately and returns — the shape `run_embedded` expects.
                // The handle is what keeps the `App` alive afterwards.
                let application =
                    Application::with_platform(SharedPlatform::new(Arc::clone(platform)).into_rc());
                app.handle = Some(application.run_embedded(launch));
            }
        }
        Err(err) => log::error!("gpui-main: open_window failed: {err:#}"),
    }
}

/// Hand a `Surface` to the render thread. Call from both `surfaceCreated` and
/// `surfaceChanged`; a repeat of the surface already held is treated as a resize.
pub fn surface_created(window: NativeWindow, scale: f32) {
    SCALE_BITS.store(scale.to_bits(), Ordering::Relaxed);
    post(Command::SurfaceCreated { window, scale });
}

/// Take the surface back and **block until the render thread has let go of it**.
///
/// Call this from `SurfaceHolder.Callback.surfaceDestroyed` before returning to the
/// framework, otherwise the `Surface` is torn down underneath a thread that may be
/// inside `ANativeWindow_lock`.
pub fn surface_destroyed() {
    post(Command::SurfaceDestroyed);

    // Bounded wait: a stuck render thread must not turn into an ANR. 2 s is far below
    // the 5 s input-dispatch timeout while being far above a worst-case frame.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !SURFACE_RELEASED.load(Ordering::SeqCst) {
        if std::time::Instant::now() >= deadline {
            log::error!(
                "gpui-main: timed out waiting for the render thread to release the surface"
            );
            return;
        }
        std::thread::sleep(Duration::from_micros(200));
    }
}

/// Deliver a whole `MotionEvent`.
///
/// `action` is `getActionMasked()`, `action_index` is `getActionIndex()`, and
/// `pointers` lists every pointer in index order (`getPointerId(i)`, `getX(i)`,
/// `getY(i)`), in physical pixels relative to the surface. The per-pointer fan-out
/// (`POINTER_DOWN`/`POINTER_UP` collapsed to `DOWN`/`UP` for the affected pointer
/// only) is the same one [`super::jni`] applies on the `android-activity` path.
///
/// Returns `false` when the touch lands on a platform view. The host should then
/// return `false` from `onTouchEvent` so the Java view hierarchy handles it.
pub fn motion_event(action: u32, action_index: usize, pointers: &[Pointer]) -> bool {
    let registry = crate::platform_view::PlatformViewRegistry::global();
    if registry.active_view_count() > 0 {
        let primary_index = match action {
            ACTION_POINTER_DOWN | ACTION_POINTER_UP => action_index,
            _ => 0,
        };
        if let Some(primary) = pointers.get(primary_index) {
            let scale = f32::from_bits(SCALE_BITS.load(Ordering::Relaxed)).max(f32::EPSILON);
            if registry.hit_test(primary.x / scale, primary.y / scale) {
                log::debug!("gpui-main: touch hits platform view, skipping GPUI dispatch");
                return false;
            }
        }
    }

    for (index, pointer) in pointers.iter().enumerate() {
        let touch_action = match action {
            ACTION_DOWN => ACTION_DOWN,
            ACTION_UP => ACTION_UP,
            ACTION_MOVE => ACTION_MOVE,
            ACTION_CANCEL => ACTION_CANCEL,
            ACTION_POINTER_DOWN if index == action_index => ACTION_DOWN,
            ACTION_POINTER_UP if index == action_index => ACTION_UP,
            _ => continue,
        };
        post(Command::Touch(TouchPoint {
            id: pointer.id,
            x: pointer.x,
            y: pointer.y,
            action: touch_action,
        }));
    }
    true
}

/// Deliver a key event. `action` is `0` for down and `1` for up.
///
/// The unicode character is resolved here rather than in Java so that the mapping stays
/// identical to the `android-activity` path.
pub fn key(key_code: i32, action: i32, meta_state: i32) {
    let unicode_char = super::jni::unicode_char_for_key_event(key_code, action, meta_state);
    post(Command::Key(AndroidKeyEvent {
        key_code,
        action,
        meta_state,
        unicode_char,
    }));
}

/// Forward an IME update from a host-owned `InputConnection`.
///
/// Mirrors what [`super::jni`]'s `nativeIme` does for `GpuiInputActivity`, exposed
/// publicly so a host with its own Activity class can route its own JNI entry point
/// here. Safe to call from the Java UI thread — the event is queued, not applied.
///
/// `kind` matches the Java side: `0` composing, `1` commit, `2` delete-surrounding,
/// `3` delete-in-code-points, `4` done/dismiss.
pub fn ime_event(session: u64, kind: i32, text: String, start: usize, end: usize) {
    super::text_input::enqueue(super::text_input::ImeEvent {
        session,
        kind,
        text,
        start,
        end,
    });
}

pub fn resumed() {
    post(Command::Resumed);
}

pub fn paused() {
    post(Command::Paused);
}
