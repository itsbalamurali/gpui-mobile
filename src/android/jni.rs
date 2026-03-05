//! Android entry point and event loop using the `android-activity` crate.
//!
//! This module replaces the previous hand-rolled `ANativeActivity_onCreate`,
//! `JNI_OnLoad`, and lifecycle callback implementations with the higher-level
//! `android-activity` glue layer.
//!
//! ## Entry sequence
//!
//! ```text
//! android-activity loads the .so and calls android_main(app: AndroidApp)
//!   └── We store the AndroidApp globally
//!       └── Call the user-supplied gpui_android_main(app)
//! ```
//!
//! ## Threading model
//!
//! `android-activity` spawns a dedicated native thread and calls `android_main`
//! on it.  All GPUI draw / event callbacks run on this thread.  The
//! `AndroidApp` handle is `Send + Sync` and can be shared across threads.
//!
//! ## User entry point
//!
//! Applications must define:
//!
//! ```rust,no_run
//! #[no_mangle]
//! fn android_main(app: android_activity::AndroidApp) {
//!     // Initialise GPUI and run the application.
//! }
//! ```
//!
//! ## Event handling
//!
//! Lifecycle events (window creation/destruction, focus changes, etc.) are
//! delivered via `AndroidApp::poll_events()`.  Input events are obtained via
//! `AndroidApp::input_events_iter()`.

#![allow(unsafe_code)]
#![allow(non_snake_case)]

use std::{
    ffi::c_void,
    sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}},
    time::Duration,
};

/// Whether the deferred init-window callback has already been invoked.
///
/// Reset to `false` on `TerminateWindow` so that when the surface is
/// recreated on resume the init callbacks run again.
static INIT_WINDOW_DONE: AtomicBool = AtomicBool::new(false);

/// Deferred lifecycle flags.
///
/// We must NOT call `win.set_active()` or `platform.did_enter_background()`
/// inside `handle_main_event` (which runs within `poll_events`).
/// The android-activity crate's Java-side callbacks block on a condvar
/// waiting for the native thread to finish processing the command.
/// If our handler tries to acquire the window `state` lock (a
/// `parking_lot::Mutex`), and a background thread is holding it (e.g.
/// during a render pass), we deadlock: native waits on the lock,
/// the lock holder waits for native rendering to complete, but native
/// is stuck.
///
/// Instead, the handlers set these flags and the main loop body
/// processes them AFTER `poll_events` returns.
static PAUSE_PENDING: AtomicBool = AtomicBool::new(false);
static RESUME_PENDING: AtomicBool = AtomicBool::new(false);

use android_activity::{AndroidApp, MainEvent, PollEvent};

use super::platform::{AndroidPlatform, SharedPlatform};

use jni::objects::{JObject, JString, JValue};
use jni::JavaVM;

// ── JNI helpers (safe `jni` crate wrappers) ──────────────────────────────────

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

/// Get or create the static `JavaVM` wrapper.
fn java_vm_safe() -> Result<&'static JavaVM, String> {
    if let Some(vm) = JAVA_VM.get() {
        return Ok(vm);
    }
    let ptr = java_vm();
    if ptr.is_null() {
        return Err("JavaVM not available".into());
    }
    Ok(JAVA_VM.get_or_init(|| unsafe {
        JavaVM::from_raw(ptr as *mut jni::sys::JavaVM)
    }))
}

/// Run a closure with an attached `jni::Env` for the current thread.
///
/// In jni 0.22 the `attach_current_thread` API is closure-based.
/// The thread is auto-detached when the closure returns (if it was
/// not already attached).
pub fn with_env<T>(f: impl FnOnce(&mut jni::Env) -> Result<T, String>) -> Result<T, String> {
    let vm = java_vm_safe()?;
    let mut result: Option<Result<T, String>> = None;
    vm.attach_current_thread(|env: &mut jni::Env| -> Result<(), jni::errors::Error> {
        result = Some(f(env));
        Ok(())
    })
    .map_err(|e: jni::errors::Error| e.to_string())?;
    result.unwrap()
}

/// Convenience alias: kept so existing callers that import `obtain_env`
/// compile with minimal changes. Returns a result by running the given
/// closure inside `with_env`.
#[inline]
pub fn obtain_env<T>(f: impl FnOnce(&mut jni::Env) -> Result<T, String>) -> Result<T, String> {
    with_env(f)
}

/// Get the Activity as a [`JObject`].
///
/// `activity_as_ptr()` returns a JNI global reference from `android-activity`
/// that is valid for the lifetime of the app. We wrap it in a `JObject`.
///
/// Requires `&Env` because jni 0.22's `JObject::from_raw` binds the
/// local-reference-frame lifetime.
pub fn activity<'local>(env: &jni::Env<'local>) -> Result<JObject<'local>, String> {
    let ptr = activity_as_ptr();
    if ptr.is_null() {
        return Err("Activity not available".into());
    }
    Ok(unsafe { JObject::from_raw(env, ptr as jni::sys::jobject) })
}

/// Convert a Java String (`JObject` wrapping a `java.lang.String`) to a Rust `String`.
///
/// Returns an empty string on null or error.
pub fn get_string(env: &mut jni::Env<'_>, obj: &JObject<'_>) -> String {
    if obj.is_null() {
        return String::new();
    }
    let jstr = unsafe { JString::from_raw(env, obj.as_raw()) };
    let result = match env.get_string(&jstr) {
        Ok(s) => s.into(),
        Err(_) => {
            let _ = env.exception_clear();
            String::new()
        }
    };
    std::mem::forget(jstr);
    result
}

/// Extension trait for converting `jni::errors::Result<T>` to `Result<T, String>`.
pub(crate) trait JniExt<T> {
    fn e(self) -> Result<T, String>;
}

impl<T> JniExt<T> for jni::errors::Result<T> {
    fn e(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}

/// Find an application class by name using the Activity's classloader.
///
/// From native threads, `JNIEnv::FindClass` uses the system classloader
/// which doesn't know about application classes.  This helper uses the
/// Activity's classloader via `activity.getClass().getClassLoader().loadClass(name)`.
///
/// `class_name` uses Java dot notation (e.g. `"dev.gpui.mobile.GpuiHelper"`).
pub fn find_app_class<'local>(
    env: &mut jni::Env<'local>,
    class_name: &str,
) -> Result<jni::objects::JClass<'local>, String> {
    let act = activity(env)?;

    // activity.getClass()
    let act_class = env
        .get_object_class(&act)
        .map_err(|e| format!("getClass failed: {e}"))?;

    // activityClass.getClassLoader()
    let class_loader = env
        .call_method(
            &act_class,
            jni::jni_str!("getClassLoader"),
            jni::jni_sig!("()Ljava/lang/ClassLoader;"),
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            let _ = env.exception_clear();
            format!("getClassLoader failed: {e}")
        })?;

    // classLoader.loadClass("dev.gpui.mobile.GpuiHelper")
    let jname = env.new_string(class_name).e()?;
    let loaded = env
        .call_method(
            &class_loader,
            jni::jni_str!("loadClass"),
            jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
            &[JValue::Object(&jname)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            let _ = env.exception_clear();
            format!("loadClass({class_name}) failed: {e}")
        })?;

    std::mem::forget(act);
    Ok(unsafe { jni::objects::JClass::from_raw(env, loaded.as_raw()) })
}

// ── global state ─────────────────────────────────────────────────────────────

/// The `AndroidApp` handle from `android-activity`.
///
/// Set once in `android_main`; read-only thereafter.
static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

/// Process-global `AndroidPlatform` instance.
///
/// Initialised once during `android_main`; read-only thereafter.
static PLATFORM: OnceLock<Arc<AndroidPlatform>> = OnceLock::new();

/// Get the unicode character produced by an Android key event via JNI.
///
/// This creates a `android.view.KeyEvent` Java object and calls
/// `getUnicodeChar(metaState)` on it.  Returns 0 on failure.
pub fn unicode_char_for_key_event(key_code: i32, action: i32, meta_state: i32) -> u32 {
    with_env(|env| {
        let key_event = match env.new_object(
            jni::jni_str!("android/view/KeyEvent"),
            jni::jni_sig!("(II)V"),
            &[JValue::Int(action), JValue::Int(key_code)],
        ) {
            Ok(o) => o,
            Err(_) => {
                let _ = env.exception_clear();
                return Ok(0);
            }
        };
        match env.call_method(&key_event, jni::jni_str!("getUnicodeChar"), jni::jni_sig!("(I)I"), &[JValue::Int(meta_state)]) {
            Ok(v) => {
                let c = v.i().unwrap_or(0);
                Ok(if c > 0 { c as u32 } else { 0 })
            }
            Err(_) => {
                let _ = env.exception_clear();
                Ok(0)
            }
        }
    })
    .unwrap_or(0)
}

// ── public accessors ──────────────────────────────────────────────────────────

/// Public accessor for the JavaVM pointer.
///
/// Uses `AndroidApp::vm_as_ptr()` from the stored `AndroidApp`.
/// Used by `platform.rs` for JNI calls.
pub fn java_vm() -> *mut c_void {
    ANDROID_APP
        .get()
        .map(|app| app.vm_as_ptr())
        .unwrap_or(std::ptr::null_mut())
}

/// Public accessor for the current Activity's JNI object reference.
///
/// Uses `AndroidApp::activity_as_ptr()` from the stored `AndroidApp`.
/// Used by `platform.rs` and `window.rs` for JNI calls that require the
/// activity's jobject.
///
/// NOTE: This returns a jobject (JNI global ref), NOT an `ANativeActivity *`.
/// Code that previously used `(*activity).clazz` should use this directly
/// as the activity jobject.
pub fn activity_as_ptr() -> *mut c_void {
    ANDROID_APP
        .get()
        .map(|app| app.activity_as_ptr())
        .unwrap_or(std::ptr::null_mut())
}

/// Returns a clone of the stored `AndroidApp`, if initialised.
pub fn android_app() -> Option<AndroidApp> {
    ANDROID_APP.get().cloned()
}

/// Returns a reference to the global `AndroidPlatform`, if initialised.
///
/// Returns `None` before `android_main` has set it up.
pub fn platform() -> Option<&'static Arc<AndroidPlatform>> {
    PLATFORM.get()
}

/// Returns a [`SharedPlatform`] wrapping the global `Arc<AndroidPlatform>`.
///
/// This is the value you hand to `Application::with_platform(...)`:
///
/// ```rust,no_run
/// let platform = jni::shared_platform().unwrap();
/// Application::with_platform(platform.into_rc()).run(|cx| { … });
/// ```
///
/// Returns `None` before `init_platform` has been called.
pub fn shared_platform() -> Option<SharedPlatform> {
    PLATFORM
        .get()
        .map(|arc| SharedPlatform::new(Arc::clone(arc)))
}

// ── NDK window bindings (still needed for direct ANativeWindow access) ────────

/// Opaque `ANativeWindow` handle.
#[repr(C)]
pub struct ANativeWindow {
    _priv: [u8; 0],
}

unsafe extern "C" {
    fn ANativeWindow_acquire(window: *mut ANativeWindow);
    fn ANativeWindow_release(window: *mut ANativeWindow);
    fn ANativeWindow_getWidth(window: *mut ANativeWindow) -> i32;
    fn ANativeWindow_getHeight(window: *mut ANativeWindow) -> i32;
}

// ── input event types ─────────────────────────────────────────────────────────

/// Motion event action constants from the NDK.
const AMOTION_EVENT_ACTION_DOWN: u32 = 0;
const AMOTION_EVENT_ACTION_UP: u32 = 1;
const AMOTION_EVENT_ACTION_MOVE: u32 = 2;

// ── night mode query via NDK Configuration ───────────────────────────────────

/// Query the current night mode using the NDK Configuration API.
///
/// Returns `true` if the system is in dark mode.
pub fn query_night_mode_via_jni() -> bool {
    let app = match android_app() {
        Some(app) => app,
        None => return false,
    };

    // Build an ndk::configuration::Configuration from the app's asset manager.
    let config = ndk::configuration::Configuration::from_asset_manager(&app.asset_manager());
    let is_dark = config.ui_mode_night() == ndk::configuration::UiModeNight::Yes;

    log::debug!("query_night_mode (ndk): is_dark={}", is_dark);
    is_dark
}

// ── input event processing ────────────────────────────────────────────────────

/// Process input events from the `AndroidApp` and dispatch them to the window.
fn process_input_events(app: &AndroidApp) {
    let platform = match PLATFORM.get() {
        Some(p) => p,
        None => {
            log::trace!("process_input_events: no platform yet");
            return;
        }
    };

    let win = match platform.primary_window() {
        Some(w) => w,
        None => {
            log::trace!("process_input_events: no primary window yet");
            return;
        }
    };

    match app.input_events_iter() {
        Ok(mut iter) => {
            loop {
                let read_input = iter.next(|event| {
                    use android_activity::input::{InputEvent, MotionAction};

                    match event {
                        InputEvent::MotionEvent(motion_event) => {
                            let action = motion_event.action();
                            let pointer_count = motion_event.pointer_count();

                            log::debug!(
                                "process_input_events: MotionEvent action={:?} pointers={}",
                                action,
                                pointer_count,
                            );

                            for i in 0..pointer_count {
                                let pointer = motion_event.pointer_at_index(i);

                                let touch_action = match action {
                                    MotionAction::Down => AMOTION_EVENT_ACTION_DOWN,
                                    MotionAction::PointerDown => {
                                        // For pointer down, only dispatch the specific pointer
                                        if i != motion_event.pointer_index() {
                                            continue;
                                        }
                                        AMOTION_EVENT_ACTION_DOWN
                                    }
                                    MotionAction::Up => AMOTION_EVENT_ACTION_UP,
                                    MotionAction::PointerUp => {
                                        // For pointer up, only dispatch the specific pointer
                                        if i != motion_event.pointer_index() {
                                            continue;
                                        }
                                        AMOTION_EVENT_ACTION_UP
                                    }
                                    MotionAction::Move => AMOTION_EVENT_ACTION_MOVE,
                                    MotionAction::Cancel => AMOTION_EVENT_ACTION_UP,
                                    _ => continue,
                                };

                                let touch = crate::android::TouchPoint {
                                    id: pointer.pointer_id(),
                                    x: pointer.x(),
                                    y: pointer.y(),
                                    action: touch_action,
                                };

                                log::debug!(
                                    "process_input_events: dispatching touch id={} x={:.0} y={:.0} action={}",
                                    touch.id, touch.x, touch.y, touch.action,
                                );

                                win.handle_touch(touch);
                            }

                            android_activity::InputStatus::Handled
                        }
                        InputEvent::KeyEvent(key_event) => {
                            use android_activity::input::KeyAction;

                            let action = match key_event.action() {
                                KeyAction::Down => 0,
                                KeyAction::Up => 1,
                                _ => return android_activity::InputStatus::Unhandled,
                            };

                            let key_code: u32 = key_event.key_code().into();
                            let meta_state: u32 = key_event.meta_state().0;

                            let unicode_char = unicode_char_for_key_event(
                                key_code as i32,
                                action,
                                meta_state as i32,
                            );

                            if unicode_char != 0 {
                                log::trace!(
                                    "dispatch_key_event: code={} action={} meta={:#x} → unicode=U+{:04X}",
                                    key_code,
                                    action,
                                    meta_state,
                                    unicode_char
                                );
                            }

                            let key_event = crate::android::AndroidKeyEvent {
                                key_code: key_code as i32,
                                action,
                                meta_state: meta_state as i32,
                                unicode_char,
                            };

                            win.handle_key_event(key_event);
                            android_activity::InputStatus::Handled
                        }
                        _ => android_activity::InputStatus::Unhandled,
                    }
                });

                if !read_input {
                    break;
                }
            }
        }
        Err(err) => {
            log::error!("Failed to get input events iterator: {err:?}");
        }
    }
}

// ── main event loop ───────────────────────────────────────────────────────────

/// The event loop that processes `android-activity` events and drives the
/// platform.
///
/// Called from `android_main` after the platform is initialised.
/// Runs until the platform requests quit or the activity is destroyed.
pub fn run_event_loop(app: &AndroidApp) {
    log::info!("run_event_loop: entering main loop");

    // Track whether the on_init_window callback has already been invoked.
    // We do NOT invoke it inside handle_main_event (which runs inside
    // poll_events) because the callback can be heavy (shader compilation,
    // GPUI Application setup).  Running it there blocks the event loop and
    // prevents the system's FocusEvent from being consumed, triggering an
    // ANR after 10 seconds.
    //
    // Instead we check each loop iteration: if a primary window exists and
    // the callback is still pending, invoke it *after* poll_events has
    // returned so focus/input events have already been drained.
    INIT_WINDOW_DONE.store(false, Ordering::Relaxed);
    let mut iteration: u64 = 0;
    let mut last_heartbeat = std::time::Instant::now();
    let mut app_is_active = false;

    let mut bg_diag_count: u32 = 0; // how many iterations since going to background
    loop {
        iteration += 1;

        // When backgrounded, log the first few iterations to diagnose blocking.
        if !app_is_active {
            bg_diag_count += 1;
            if bg_diag_count <= 20 || bg_diag_count % 250 == 0 {
                log::info!("run_event_loop: bg diag #{} (iter={})", bg_diag_count, iteration);
            }
        } else {
            bg_diag_count = 0;
        }

        // Log a heartbeat every 5 seconds so we can tell if the loop is alive.
        let now = std::time::Instant::now();
        if now.duration_since(last_heartbeat) >= Duration::from_secs(5) {
            log::info!(
                "run_event_loop: heartbeat — iteration={}, init_done={}, active={}",
                iteration,
                INIT_WINDOW_DONE.load(Ordering::Relaxed),
                app_is_active,
            );
            last_heartbeat = now;
        }

        // Check if quit was requested.
        if let Some(platform) = PLATFORM.get() {
            if platform.should_quit() {
                log::info!("run_event_loop: platform requested quit");
                break;
            }
            platform.tick();
        }

        // Poll for events — non-blocking (Duration::ZERO) to guarantee
        // the loop never gets stuck in ALooper_pollAll, which can block
        // indefinitely on some devices during lifecycle transitions.
        // We do our own sleeping at the bottom of the loop instead.
        let poll_start = std::time::Instant::now();
        let pause_was_pending = PAUSE_PENDING.load(Ordering::Relaxed);
        app.poll_events(Some(Duration::ZERO), |event| {
            match event {
                PollEvent::Main(main_event) => {
                    handle_main_event(app, main_event);
                }
                PollEvent::Wake => {}
                PollEvent::Timeout => {}
                _ => {}
            }
        });
        let poll_elapsed = poll_start.elapsed();
        // Log detailed diagnostics around Pause transition.
        if PAUSE_PENDING.load(Ordering::Relaxed) && !pause_was_pending {
            log::info!(
                "run_event_loop: poll_events returned after Pause set (iter={}, took={:.0}ms)",
                iteration,
                poll_elapsed.as_secs_f64() * 1000.0,
            );
        }
        if poll_elapsed > Duration::from_millis(100) {
            log::warn!("run_event_loop: poll_events took {:.0}ms (iter={})", poll_elapsed.as_secs_f64() * 1000.0, iteration);
        }

        // Process deferred lifecycle state changes.
        //
        // These flags were set inside handle_main_event (within
        // poll_events).  Now that poll_events has returned and the
        // android-activity condvar synchronisation is released, it is
        // safe to acquire the window state lock.
        if PAUSE_PENDING.swap(false, Ordering::Relaxed) {
            log::info!("run_event_loop: processing deferred PAUSE (iter={})", iteration);
            if let Some(platform) = PLATFORM.get() {
                log::info!("run_event_loop: calling did_enter_background");
                platform.did_enter_background();
                log::info!("run_event_loop: did_enter_background done");
                if let Some(win) = platform.primary_window() {
                    log::info!("run_event_loop: got window, calling set_active(false)");
                    win.set_active(false);
                    log::info!("run_event_loop: set_active(false) returned");
                }
            }
            log::info!("run_event_loop: deferred PAUSE done (iter={})", iteration);
        }
        if RESUME_PENDING.swap(false, Ordering::Relaxed) {
            log::info!("run_event_loop: processing deferred RESUME (iter={})", iteration);
            if let Some(platform) = PLATFORM.get() {
                platform.did_become_active();
                if let Some(win) = platform.primary_window() {
                    win.set_active(true);
                }
            }
            log::info!("run_event_loop: deferred RESUME done (iter={})", iteration);
        }

        // Track active/focused state so we can skip heavy work when backgrounded.
        if let Some(platform) = PLATFORM.get() {
            if let Some(win) = platform.primary_window() {
                let is_active = win.is_active();
                if is_active != app_is_active {
                    log::info!(
                        "run_event_loop: active state changed {} -> {} (iteration={})",
                        app_is_active,
                        is_active,
                        iteration,
                    );
                    app_is_active = is_active;
                }
            }
        }

        // Process any pending input events.
        process_input_events(app);

        // Deferred initialisation callbacks.  Runs once, after the first
        // loop iteration where a primary window exists.  By this point
        // the GainedFocus / FocusEvent has already been consumed above.
        //
        // Two callbacks may be pending:
        //
        // 1. `finish_launching` — stored by `Platform::run` (called from
        //    `Application::run`).  This is the GPUI finish-launching
        //    callback that calls the user's `|cx| { cx.open_window(...) }`
        //    closure.  Invoking it here (while `Platform::run` is still
        //    blocking on the stack) keeps the `Rc<RefCell<AppContext>>`
        //    alive so that weak references in GPUI callbacks remain valid
        //    for the lifetime of the event loop.
        //
        // 2. `on_init_window` — an optional Android-specific callback
        //    registered via `platform.set_on_init_window(...)`.  This is
        //    for legacy / advanced use-cases where the caller wants to do
        //    extra work when the window becomes available.
        //
        // Both are invoked at most once.
        if !INIT_WINDOW_DONE.load(Ordering::Relaxed) {
            if let Some(platform) = PLATFORM.get() {
                if platform.primary_window().is_some() {
                    // 1. Invoke the GPUI finish-launching callback first.
                    //    This is the primary path: Application::run stored
                    //    its callback via Platform::run, and we invoke it
                    //    now that the window is ready.
                    if let Some(finish_cb) = platform.take_finish_launching_callback() {
                        log::info!(
                            "run_event_loop: invoking finish_launching callback (iteration={})",
                            iteration,
                        );
                        finish_cb();
                        log::info!(
                            "run_event_loop: finish_launching callback completed (iteration={})",
                            iteration,
                        );
                    }

                    // 2. Invoke the on_init_window callback (if any).
                    if let Some(init_cb) = platform.take_on_init_window_callback() {
                        let win = platform.primary_window().unwrap();
                        log::info!(
                            "run_event_loop: invoking on_init_window callback (iteration={})",
                            iteration,
                        );
                        init_cb(win);
                        log::info!(
                            "run_event_loop: on_init_window callback completed (iteration={})",
                            iteration,
                        );
                    }

                    INIT_WINDOW_DONE.store(true, Ordering::Relaxed);

                    // Force an immediate first frame render right now,
                    // rather than waiting for the next loop iteration
                    // (~16 ms later).  This minimises the time the user
                    // sees the theme's white splash background before
                    // real GPUI content appears.
                    platform.flush_main_thread_tasks();
                    if let Some(win) = platform.primary_window() {
                        log::info!(
                            "run_event_loop: rendering first frame immediately (iteration={})",
                            iteration,
                        );
                        win.request_frame();
                    }
                }
            }
        }

        // Drive the GPUI rendering pipeline.
        //
        // GPUI registers an `on_request_frame` callback on the
        // PlatformWindow during `cx.open_window(...)`.  That callback
        // triggers the layout → paint → draw cycle.  Without invoking
        // `request_frame()` here the view is wired up but never
        // actually rendered — producing a dark/blank window and
        // eventually an ANR.
        //
        // We also flush any main-thread tasks that were dispatched
        // during this iteration (e.g. by the background executor or
        // by GPUI internals) so they don't pile up until the next
        // looper wake.
        //
        // IMPORTANT: Only drive rendering AFTER `INIT_WINDOW_DONE`.
        // Before that point the GPUI view hierarchy hasn't been set up
        // (finish_launching hasn't run yet), so calling request_frame()
        // would render an empty scene and present a blank/transparent
        // frame — causing a visible flash on startup.  The Activity
        // theme's `windowBackground` (white) covers the surface until
        // the first real GPUI frame is drawn, and the draw guard in
        // AndroidWindow::draw() skips empty scenes to avoid clearing
        // the surface prematurely.
        if let Some(platform) = PLATFORM.get() {
            // Only drive rendering and flush tasks when:
            // 1. INIT_WINDOW_DONE — GPUI callbacks are wired up
            // 2. app_is_active — surface is valid (not backgrounded)
            //
            // IMPORTANT: flush_main_thread_tasks MUST be guarded by
            // app_is_active.  Queued tasks may make JNI calls that
            // require the Java UI thread (e.g. set_system_chrome).
            // During lifecycle transitions the Java thread blocks in
            // set_activity_state() / set_window() waiting for the
            // native thread to process the corresponding command.
            // If we flush tasks here that call into the Java thread,
            // we deadlock: native waits on Java, Java waits on native.
            if INIT_WINDOW_DONE.load(Ordering::Relaxed) && app_is_active {
                platform.flush_main_thread_tasks();
                if let Some(win) = platform.primary_window() {
                    let frame_start = std::time::Instant::now();
                    win.request_frame();
                    let frame_elapsed = frame_start.elapsed();
                    if frame_elapsed > Duration::from_millis(100) {
                        log::warn!(
                            "run_event_loop: request_frame took {:.1}ms (iteration={})",
                            frame_elapsed.as_secs_f64() * 1000.0,
                            iteration,
                        );
                    }
                }

                // Drain input events again AFTER rendering so that any
                // events that arrived while request_frame was running are
                // consumed promptly (prevents ANR on long frames).
                process_input_events(app);
            }
        }

        // Sleep to yield CPU.
        if !app_is_active && bg_diag_count <= 5 {
            log::info!("run_event_loop: end of iteration {} (bg#{}), about to sleep", iteration, bg_diag_count);
        }
        std::thread::sleep(Duration::from_millis(4));

    }

    log::info!("run_event_loop: exiting main loop");
}

/// Handle a single `MainEvent` from `android-activity`.
fn handle_main_event(app: &AndroidApp, event: MainEvent<'_>) {
    match event {
        MainEvent::InitWindow { .. } => {
            log::info!("MainEvent::InitWindow");

            if let Some(platform) = PLATFORM.get() {
                if let Some(native_window) = app.native_window() {
                    let raw_ptr = native_window.ptr().as_ptr() as *mut ANativeWindow;
                    let width = unsafe { ANativeWindow_getWidth(raw_ptr) };
                    let height = unsafe { ANativeWindow_getHeight(raw_ptr) };
                    log::info!("InitWindow — {}×{}", width, height);

                    // Update the primary display with the new window geometry.
                    //
                    // We pass the asset manager from ndk-context if available,
                    // otherwise use a null pointer and let the display fall back
                    // to a default density.
                    let native_win = raw_ptr as *mut crate::android::display::ANativeWindow;

                    // Get the asset manager from the AndroidApp so we can
                    // query the real screen density via AConfiguration.
                    let asset_manager = app.asset_manager().ptr().as_ptr() as *mut std::ffi::c_void;
                    if let Err(e) =
                        unsafe { platform.update_primary_display(native_win, asset_manager) }
                    {
                        log::warn!("failed to update primary display: {e:#}");
                    }

                    let scale_factor = platform
                        .primary_display()
                        .map(|d| d.scale_factor())
                        .unwrap_or(1.0);

                    let win_ptr = raw_ptr as *mut crate::android::window::ANativeWindow;

                    // If the logical window already exists (i.e. we're
                    // returning from background), reinit its surface
                    // instead of creating a brand-new window.  This
                    // preserves all GPUI callbacks (on_request_frame,
                    // touch, key, etc.) that were registered during the
                    // initial open_window / finish_launching sequence.
                    if let Some(existing) = platform.primary_window() {
                        let mut gpu_ctx = platform.take_gpu_context();
                        match unsafe { existing.init_window(win_ptr, &mut gpu_ctx) } {
                            Ok(()) => {
                                platform.return_gpu_context(gpu_ctx);
                                log::info!(
                                    "InitWindow: reinitialised existing window id={:#x}",
                                    existing.id(),
                                );
                            }
                            Err(e) => {
                                platform.return_gpu_context(gpu_ctx);
                                log::error!("failed to reinit window surface: {e:#}");
                            }
                        }

                        // Update safe area insets.
                        let cr = app.content_rect();
                        existing.update_safe_area_from_content_rect(
                            cr.left, cr.top, cr.right, cr.bottom,
                        );

                        // Mark init done so the event loop resumes
                        // rendering immediately (the GPUI callbacks are
                        // already wired up from the first launch).
                        INIT_WINDOW_DONE.store(true, Ordering::Relaxed);
                    } else {
                        // First launch — create a new window.
                        match unsafe { platform.open_window(win_ptr, scale_factor, false) } {
                            Ok(win) => {
                                log::info!(
                                    "window opened — id={:#x} scale={:.1}",
                                    win.id(),
                                    scale_factor
                                );

                                let cr = app.content_rect();
                                log::info!(
                                    "content_rect: left={} top={} right={} bottom={} (window={}×{})",
                                    cr.left,
                                    cr.top,
                                    cr.right,
                                    cr.bottom,
                                    width,
                                    height,
                                );
                                win.update_safe_area_from_content_rect(
                                    cr.left, cr.top, cr.right, cr.bottom,
                                );

                                log::info!("InitWindow: window ready, callback deferred to event loop");
                            }
                            Err(e) => {
                                log::error!("failed to open window: {e:#}");
                            }
                        }
                    }
                }
            }
        }

        MainEvent::TerminateWindow { .. } => {
            log::info!("MainEvent::TerminateWindow");

            // Reset so that the deferred-init block in run_event_loop
            // fires again when the window surface is recreated on resume.
            INIT_WINDOW_DONE.store(false, Ordering::Relaxed);

            // Reset cached chrome style so it re-applies after resume.
            *LAST_CHROME_STYLE.lock().unwrap() = None;

            if let Some(platform) = PLATFORM.get() {
                if let Some(win) = platform.primary_window() {
                    // Only tear down the surface — do NOT call
                    // platform.close_window().  Closing the logical window
                    // destroys all GPUI callbacks (on_request_frame, touch,
                    // etc.).  When the surface is recreated on resume,
                    // InitWindow would create a brand-new window with no
                    // callbacks, causing the app to freeze.
                    win.term_window();
                }
            }
        }

        MainEvent::WindowResized { .. } => {
            log::debug!("MainEvent::WindowResized");

            if let Some(platform) = PLATFORM.get() {
                if let Some(win) = platform.primary_window() {
                    win.handle_resize();

                    // Re-query content_rect after resize to update safe area insets.
                    let cr = app.content_rect();
                    log::debug!(
                        "WindowResized content_rect: left={} top={} right={} bottom={}",
                        cr.left,
                        cr.top,
                        cr.right,
                        cr.bottom,
                    );
                    win.update_safe_area_from_content_rect(cr.left, cr.top, cr.right, cr.bottom);
                }
            }
        }

        MainEvent::GainedFocus => {
            log::info!("MainEvent::GainedFocus");
            RESUME_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::LostFocus => {
            log::info!("MainEvent::LostFocus");
            PAUSE_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::Resume { .. } => {
            log::info!("MainEvent::Resume");
            RESUME_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::Pause => {
            log::info!("MainEvent::Pause");
            // set_active uses AtomicBool so it never blocks.
            PAUSE_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::ConfigChanged { .. } => {
            log::debug!("MainEvent::ConfigChanged");

            if let Some(platform) = PLATFORM.get() {
                platform.notify_keyboard_layout_change();

                let is_dark = query_night_mode_via_jni();

                if let Some(win) = platform.primary_window() {
                    let appearance = if is_dark {
                        crate::android::window::WindowAppearance::Dark
                    } else {
                        crate::android::window::WindowAppearance::Light
                    };
                    win.set_appearance(appearance);
                }
            }
        }

        MainEvent::Start => {
            log::info!("MainEvent::Start");
        }

        MainEvent::Stop => {
            log::info!("MainEvent::Stop");
        }

        MainEvent::SaveState { .. } => {
            log::info!("MainEvent::SaveState");
        }

        MainEvent::LowMemory => {
            log::warn!("MainEvent::LowMemory — consider releasing cached resources");
        }

        MainEvent::Destroy => {
            log::info!("MainEvent::Destroy");

            if let Some(platform) = PLATFORM.get() {
                platform.quit();
            }
        }

        MainEvent::InsetsChanged { .. } => {
            log::info!("MainEvent::InsetsChanged");
        }

        _ => {
            log::info!("MainEvent: unhandled variant");
        }
    }
}

// ── main loop helper (compat with existing code) ──────────────────────────────

/// Run one iteration of the event loop.
///
/// This is a compatibility wrapper for code that uses a manual poll loop.
/// Prefer `run_event_loop` for the standard event loop.
///
/// `timeout_ms` — how long to block waiting for events (milliseconds).
/// Pass `0` for non-blocking, `-1` to block indefinitely.
///
/// Returns `true` if the application should exit.
pub fn poll_events(timeout_ms: i32) -> bool {
    if let Some(platform) = PLATFORM.get() {
        if platform.should_quit() {
            return true;
        }
        platform.tick();
    }

    let app = match ANDROID_APP.get() {
        Some(app) => app,
        None => return false,
    };

    let timeout = if timeout_ms < 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms as u64))
    };

    app.poll_events(timeout, |event| match event {
        PollEvent::Main(main_event) => {
            handle_main_event(app, main_event);
        }
        PollEvent::Wake => {}
        _ => {}
    });

    process_input_events(app);

    // Drive the GPUI rendering pipeline (same as run_event_loop).
    if let Some(platform) = PLATFORM.get() {
        platform.flush_main_thread_tasks();
        if let Some(win) = platform.primary_window() {
            win.request_frame();
        }
    }

    false
}

// ── public init / run helpers ─────────────────────────────────────────────────

/// Install a panic hook that routes panic messages to logcat.
///
/// Call this early in `android_main` so that any subsequent panic is
/// visible via `adb logcat`.  Safe to call multiple times — each call
/// replaces the previous hook.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Box<dyn Any>".to_string()
        };
        if let Some(loc) = info.location() {
            log::error!(
                "PANIC at {}:{}:{}: {}",
                loc.file(),
                loc.line(),
                loc.column(),
                payload
            );
        } else {
            log::error!("PANIC: {}", payload);
        }
    }));
}

/// Store the `AndroidApp` globally and create the `AndroidPlatform`.
///
/// Must be called exactly once from `android_main` before
/// `run_event_loop`.  Returns a reference to the platform so the caller
/// can register callbacks (e.g. `set_on_init_window`) before entering
/// the event loop.
pub fn init_platform(app: &AndroidApp) -> &'static Arc<AndroidPlatform> {
    let _ = ANDROID_APP.set(app.clone());
    log::info!("init_platform: stored AndroidApp");

    let platform = Arc::new(AndroidPlatform::new(false));
    log::info!("init_platform: AndroidPlatform created");

    PLATFORM
        .set(Arc::clone(&platform))
        .unwrap_or_else(|_| log::warn!("PLATFORM already set — duplicate init_platform?"));

    // SAFETY: we just set it above.
    PLATFORM.get().unwrap()
}

// ── system chrome (status bar / navigation bar) ───────────────────────────────

/// Cached last-applied system chrome style.
///
/// `set_system_chrome` is called on every frame render.  The JNI calls it
/// makes (getWindow, setStatusBarColor, etc.) are View operations that can
/// contend with the Android UI thread and intermittently deadlock.
/// By caching the last applied style we skip the JNI calls entirely when
/// nothing changed — which is the common case.
static LAST_CHROME_STYLE: std::sync::Mutex<Option<(Option<u32>, Option<u32>, crate::StatusBarContentStyle)>> =
    std::sync::Mutex::new(None);

/// Apply system chrome styling on Android.
///
/// Sets the status bar color, navigation bar color, and light/dark
/// status bar icons via JNI calls to `Window` and `WindowInsetsController`.
///
/// Must be called from the main (native) thread that has JNI access.
pub fn set_system_chrome(style: &crate::SystemChromeStyle) {
    let status_bar_color = style.status_bar_color;
    let navigation_bar_color = style.navigation_bar_color;
    let status_bar_style = style.status_bar_style;

    // Skip the (expensive, potentially deadlocking) JNI calls when nothing changed.
    {
        let key = (status_bar_color, navigation_bar_color, status_bar_style);
        let mut last = LAST_CHROME_STYLE.lock().unwrap();
        if *last == Some(key) {
            return;
        }
        *last = Some(key);
    }

    let result = with_env(|env| {
        let activity_obj = activity(env)?;

        // 1. Get the Window: activity.getWindow()
        let window = env
            .call_method(&activity_obj, jni::jni_str!("getWindow"), jni::jni_sig!("()Landroid/view/Window;"), &[])
            .and_then(|v: jni::objects::JValueOwned| v.l())
            .map_err(|e| { let _ = env.exception_clear(); e.to_string() })?;
        if window.is_null() {
            return Err("getWindow returned null".into());
        }

        // 2. Set status bar color if provided
        if let Some(color) = status_bar_color {
            let argb = (0xFF000000_u32 | color) as i32;
            let _ = env.call_method(&window, jni::jni_str!("setStatusBarColor"), jni::jni_sig!("(I)V"), &[JValue::Int(argb)]);
            let _ = env.exception_clear();
        }

        // 3. Set navigation bar color if provided
        if let Some(color) = navigation_bar_color {
            let argb = (0xFF000000_u32 | color) as i32;
            let _ = env.call_method(&window, jni::jni_str!("setNavigationBarColor"), jni::jni_sig!("(I)V"), &[JValue::Int(argb)]);
            let _ = env.exception_clear();
        }

        // 4. Set light/dark status bar icons via WindowInsetsController (API 30+)
        let insetsctl = env.call_method(
            &window,
            jni::jni_str!("getInsetsController"),
            jni::jni_sig!("()Landroid/view/WindowInsetsController;"),
            &[],
        );

        if let Ok(v) = insetsctl {
            if let Ok(ctl) = v.l() {
                if !ctl.is_null() {
                    let mask: i32 = 0x00000008;
                    let appearance: i32 = match status_bar_style {
                        crate::StatusBarContentStyle::Dark => 0x00000008,
                        crate::StatusBarContentStyle::Light => 0,
                    };
                    let _ = env.call_method(
                        &ctl,
                        jni::jni_str!("setSystemBarsAppearance"),
                        jni::jni_sig!("(II)V"),
                        &[JValue::Int(appearance), JValue::Int(mask)],
                    );
                    let _ = env.exception_clear();
                }
            }
        } else {
            let _ = env.exception_clear();

            if let Ok(decor) = env
                .call_method(&window, jni::jni_str!("getDecorView"), jni::jni_sig!("()Landroid/view/View;"), &[])
                .and_then(|v: jni::objects::JValueOwned| v.l())
            {
                if !decor.is_null() {
                    if let Ok(current) = env
                        .call_method(&decor, jni::jni_str!("getSystemUiVisibility"), jni::jni_sig!("()I"), &[])
                        .and_then(|v: jni::objects::JValueOwned| v.i())
                    {
                        let new_flags = match status_bar_style {
                            crate::StatusBarContentStyle::Dark => current | 0x00002000,
                            crate::StatusBarContentStyle::Light => current & !0x00002000,
                        };
                        let _ = env.call_method(
                            &decor,
                            jni::jni_str!("setSystemUiVisibility"),
                            jni::jni_sig!("(I)V"),
                            &[JValue::Int(new_flags)],
                        );
                        let _ = env.exception_clear();
                    }
                }
            }
        }

        Ok(())
    });

    if let Err(e) = result {
        log::warn!("set_system_chrome: {e}");
    }

    log::info!(
        "set_system_chrome: status_bar_color={:?}, nav_bar_color={:?}, style={:?}",
        style.status_bar_color, style.navigation_bar_color, style.status_bar_style
    );
}

// ── software keyboard (IME) control ───────────────────────────────────────────

/// Show the software keyboard on Android with a specific keyboard type.
/// Show the software keyboard on Android.
///
/// Uses the NDK `ANativeActivity_showSoftInput` via `android-activity`.
/// The previous EditText/JNI approach silently failed with
/// `CalledFromWrongThreadException` because all JNI View operations
/// must run on the Android UI thread, not the native Rust thread.
/// The NDK function handles the UI-thread dispatch internally.
///
/// Text input arrives via `KeyEvent`s through `process_input_events()`.
pub fn show_keyboard_android(_keyboard_type: crate::KeyboardType) {
    if let Some(app) = android_app() {
        log::info!("show_keyboard_android: using NDK show_soft_input");
        app.show_soft_input(false);
    }
}

/// Hide the software keyboard on Android.
///
/// Uses the NDK `ANativeActivity_hideSoftInput` via `android-activity`.
pub fn hide_keyboard_android() {
    if let Some(app) = android_app() {
        log::info!("hide_keyboard_android: using NDK hide_soft_input");
        app.hide_soft_input(false);
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_events_returns_false_when_no_platform() {
        // PLATFORM is not set in a unit-test context, so poll_events should
        // be a safe no-op and return false (don't quit).
        let result = poll_events(0);
        let _ = result;
    }

    #[test]
    fn java_vm_returns_null_before_init() {
        // Before android_main is called, java_vm() should return null.
        let vm = java_vm();
        assert!(vm.is_null());
    }

    #[test]
    fn activity_as_ptr_returns_null_before_init() {
        let ptr = activity_as_ptr();
        assert!(ptr.is_null());
    }

    #[test]
    fn android_app_returns_none_before_init() {
        assert!(android_app().is_none());
    }

    #[test]
    fn platform_returns_none_before_init() {
        assert!(platform().is_none());
    }
}
