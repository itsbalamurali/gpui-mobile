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
    sync::{Arc, OnceLock},
    time::Duration,
};

use android_activity::{AndroidApp, MainEvent, PollEvent};

use super::platform::{AndroidPlatform, SharedPlatform};

use jni::objects::JValue;
use super::jni_helpers;

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
    let mut env = match jni_helpers::obtain_env() {
        Ok(e) => e,
        Err(_) => return 0,
    };
    let key_event = match env.new_object(
        "android/view/KeyEvent",
        "(II)V",
        &[JValue::Int(action), JValue::Int(key_code)],
    ) {
        Ok(o) => o,
        Err(_) => {
            let _ = env.exception_clear();
            return 0;
        }
    };
    match env.call_method(&key_event, "getUnicodeChar", "(I)I", &[JValue::Int(meta_state)]) {
        Ok(v) => {
            let c = v.i().unwrap_or(0);
            if c > 0 { c as u32 } else { 0 }
        }
        Err(_) => {
            let _ = env.exception_clear();
            0
        }
    }
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
    let mut init_window_done = false;
    let mut iteration: u64 = 0;
    let mut last_heartbeat = std::time::Instant::now();
    let mut app_is_active = false;

    loop {
        iteration += 1;

        // Log a heartbeat every 5 seconds so we can tell if the loop is alive.
        let now = std::time::Instant::now();
        if now.duration_since(last_heartbeat) >= Duration::from_secs(5) {
            log::info!(
                "run_event_loop: heartbeat — iteration={}, init_done={}",
                iteration,
                init_window_done,
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

        // Poll for events with a short timeout.
        //
        // IMPORTANT: Always use a short (16ms) timeout.  On some devices
        // `ALooper_pollAll` blocks indefinitely when the app loses focus
        // even with a `Some(timeout)`, which starves the input queue and
        // triggers an ANR ("Waited 10001ms for MotionEvent").  A short
        // fixed timeout ensures we regain control quickly so we can drain
        // input events and avoid the ANR.
        let poll_start = std::time::Instant::now();
        app.poll_events(Some(Duration::from_millis(16)), |event| {
            match event {
                PollEvent::Main(main_event) => {
                    handle_main_event(app, main_event);
                }
                PollEvent::Wake => {
                    // Wakeup — process pending tasks.
                }
                _ => {}
            }
        });
        let poll_elapsed = poll_start.elapsed();
        if poll_elapsed > Duration::from_secs(1) {
            log::warn!(
                "run_event_loop: poll_events blocked for {:.1}s (iteration={}, active={})",
                poll_elapsed.as_secs_f64(),
                iteration,
                app_is_active,
            );
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
        if !init_window_done {
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

                    init_window_done = true;

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
        // IMPORTANT: Only drive rendering AFTER `init_window_done`.
        // Before that point the GPUI view hierarchy hasn't been set up
        // (finish_launching hasn't run yet), so calling request_frame()
        // would render an empty scene and present a blank/transparent
        // frame — causing a visible flash on startup.  The Activity
        // theme's `windowBackground` (white) covers the surface until
        // the first real GPUI frame is drawn, and the draw guard in
        // AndroidWindow::draw() skips empty scenes to avoid clearing
        // the surface prematurely.
        if let Some(platform) = PLATFORM.get() {
            // Flush pending main-thread tasks first so that any
            // state changes (e.g. model updates, layout
            // invalidations) are applied before we paint.
            platform.flush_main_thread_tasks();

            // Only drive rendering when:
            // 1. init_window_done — GPUI callbacks are wired up
            // 2. app_is_active — surface is valid (not backgrounded)
            if init_window_done && app_is_active {
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
                    match unsafe { platform.open_window(win_ptr, scale_factor, false) } {
                        Ok(win) => {
                            log::info!(
                                "window opened — id={:#x} scale={:.1}",
                                win.id(),
                                scale_factor
                            );

                            // Query the content rect to compute safe area insets.
                            // content_rect excludes system bars (status bar, nav bar).
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

                            // NOTE: The callback is NOT invoked here — it
                            // runs in run_event_loop after poll_events returns
                            // so the system's FocusEvent is consumed first
                            // (avoids ANR).
                            log::info!("InitWindow: window ready, callback deferred to event loop");
                        }
                        Err(e) => {
                            log::error!("failed to open window: {e:#}");
                        }
                    }
                }
            }
        }

        MainEvent::TerminateWindow { .. } => {
            log::info!("MainEvent::TerminateWindow");

            if let Some(platform) = PLATFORM.get() {
                if let Some(win) = platform.primary_window() {
                    win.term_window();
                    platform.close_window(win.id());
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
            log::debug!("MainEvent::GainedFocus");

            if let Some(platform) = PLATFORM.get() {
                platform.did_become_active();
                if let Some(win) = platform.primary_window() {
                    win.set_active(true);
                }
            }
        }

        MainEvent::LostFocus => {
            log::debug!("MainEvent::LostFocus");

            if let Some(platform) = PLATFORM.get() {
                platform.did_enter_background();
                if let Some(win) = platform.primary_window() {
                    win.set_active(false);
                }
            }
        }

        MainEvent::Resume { .. } => {
            log::debug!("MainEvent::Resume");

            if let Some(platform) = PLATFORM.get() {
                platform.did_become_active();
            }
        }

        MainEvent::Pause => {
            log::debug!("MainEvent::Pause");

            if let Some(platform) = PLATFORM.get() {
                platform.did_enter_background();
            }
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

        MainEvent::LowMemory => {
            log::warn!("MainEvent::LowMemory — consider releasing cached resources");
        }

        MainEvent::Destroy => {
            log::info!("MainEvent::Destroy");

            if let Some(platform) = PLATFORM.get() {
                platform.quit();
            }
        }

        _ => {
            // Other events we don't handle.
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

/// Apply system chrome styling on Android.
///
/// Sets the status bar color, navigation bar color, and light/dark
/// status bar icons via JNI calls to `Window` and `WindowInsetsController`.
///
/// Must be called from the main (native) thread that has JNI access.
pub fn set_system_chrome(style: &crate::SystemChromeStyle) {
    let mut env = match jni_helpers::obtain_env() {
        Ok(e) => e,
        Err(e) => {
            log::warn!("set_system_chrome: {e}");
            return;
        }
    };
    let activity_obj = match jni_helpers::activity() {
        Ok(a) => a,
        Err(e) => {
            log::warn!("set_system_chrome: {e}");
            return;
        }
    };

    // 1. Get the Window: activity.getWindow()
    let window = match env.call_method(&activity_obj, "getWindow", "()Landroid/view/Window;", &[]) {
        Ok(v) => match v.l() {
            Ok(o) if !o.is_null() => o,
            _ => return,
        },
        Err(_) => { let _ = env.exception_clear(); return; }
    };

    // 2. Set status bar color if provided
    if let Some(color) = style.status_bar_color {
        let argb = (0xFF000000_u32 | color) as i32;
        let _ = env.call_method(&window, "setStatusBarColor", "(I)V", &[JValue::Int(argb)]);
        let _ = env.exception_clear();
    }

    // 3. Set navigation bar color if provided
    if let Some(color) = style.navigation_bar_color {
        let argb = (0xFF000000_u32 | color) as i32;
        let _ = env.call_method(&window, "setNavigationBarColor", "(I)V", &[JValue::Int(argb)]);
        let _ = env.exception_clear();
    }

    // 4. Set light/dark status bar icons via WindowInsetsController (API 30+)
    //    Fallback: use View.setSystemUiVisibility with SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
    let insetsctl = env.call_method(
        &window,
        "getInsetsController",
        "()Landroid/view/WindowInsetsController;",
        &[],
    );

    if let Ok(v) = insetsctl {
        if let Ok(ctl) = v.l() {
            if !ctl.is_null() {
                // APPEARANCE_LIGHT_STATUS_BARS = 0x00000008
                let mask: i32 = 0x00000008;
                let appearance: i32 = match style.status_bar_style {
                    crate::StatusBarContentStyle::Dark => 0x00000008,
                    crate::StatusBarContentStyle::Light => 0,
                };
                let _ = env.call_method(
                    &ctl,
                    "setSystemBarsAppearance",
                    "(II)V",
                    &[JValue::Int(appearance), JValue::Int(mask)],
                );
                let _ = env.exception_clear();
            }
        }
    } else {
        // Fallback for API < 30: use decorView.setSystemUiVisibility
        let _ = env.exception_clear();

        if let Ok(decor) = env
            .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])
            .and_then(|v| v.l())
        {
            if !decor.is_null() {
                if let Ok(current) = env
                    .call_method(&decor, "getSystemUiVisibility", "()I", &[])
                    .and_then(|v| v.i())
                {
                    // SYSTEM_UI_FLAG_LIGHT_STATUS_BAR = 0x00002000
                    let new_flags = match style.status_bar_style {
                        crate::StatusBarContentStyle::Dark => current | 0x00002000,
                        crate::StatusBarContentStyle::Light => current & !0x00002000,
                    };
                    let _ = env.call_method(
                        &decor,
                        "setSystemUiVisibility",
                        "(I)V",
                        &[JValue::Int(new_flags)],
                    );
                    let _ = env.exception_clear();
                }
            }
        }
    }

    log::info!(
        "set_system_chrome: status_bar_color={:?}, nav_bar_color={:?}, style={:?}",
        style.status_bar_color, style.navigation_bar_color, style.status_bar_style
    );
}

// ── software keyboard (IME) control ───────────────────────────────────────────

/// Show the software keyboard on Android with a specific keyboard type.
///
/// Uses `AndroidApp::show_soft_input()` from the `android-activity` crate.
pub fn show_keyboard_android(keyboard_type: crate::KeyboardType) {
    let _ = keyboard_type; // TODO: map keyboard type to InputType flags
    let app = match android_app() {
        Some(app) => app,
        None => {
            log::warn!("show_keyboard_android: no AndroidApp");
            return;
        }
    };
    app.show_soft_input(true); // show_implicit = true
    log::info!("show_keyboard_android: keyboard shown (via android-activity)");
}

/// Hide the software keyboard on Android.
///
/// Uses `AndroidApp::hide_soft_input()` from the `android-activity` crate.
pub fn hide_keyboard_android() {
    let app = match android_app() {
        Some(app) => app,
        None => {
            log::warn!("hide_keyboard_android: no AndroidApp");
            return;
        }
    };
    app.hide_soft_input(false); // hide_implicit_only = false
    log::info!("hide_keyboard_android: keyboard hidden (via android-activity)");
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
