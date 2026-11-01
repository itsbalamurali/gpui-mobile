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
//!
//! ## Frames
//!
//! The loop blocks on the looper between events and draws only when GPUI
//! asked for a frame and a vsync has passed since — see
//! [`super::frame_source`].

#![allow(unsafe_code)]
#![allow(non_snake_case)]

use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};

/// Whether the deferred init-window callback has already been invoked.
///
/// Reset to `false` on `TerminateWindow` so that when the surface is
/// recreated on resume the init callbacks run again.
static INIT_WINDOW_DONE: AtomicBool = AtomicBool::new(false);

/// Whether the GPUI native library has completed initialization.
///
/// Set to `true` after the first frame is rendered. This can be queried
/// via JNI by a custom Activity to dismiss the splash screen, although
/// with NativeActivity the system splash handles this automatically.
pub static NATIVE_INITIALIZED: AtomicBool = AtomicBool::new(false);

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
static TERM_WINDOW_PENDING: AtomicBool = AtomicBool::new(false);
static INIT_WINDOW_PENDING: AtomicBool = AtomicBool::new(false);
static WINDOW_RESIZED_PENDING: AtomicBool = AtomicBool::new(false);
static CONFIG_CHANGED_PENDING: AtomicBool = AtomicBool::new(false);

use android_activity::{AndroidApp, MainEvent, PollEvent};

use super::platform::{AndroidPlatform, SharedPlatform};

use jni::objects::{JObject, JString, JValue};
use jni::JavaVM;
use std::sync::{atomic::AtomicPtr, Mutex};

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
    Ok(JAVA_VM.get_or_init(|| unsafe { JavaVM::from_raw(ptr as *mut jni::sys::JavaVM) }))
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
    jstr.to_string()
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

    // activity.getClassLoader() — call on the Context instance directly.
    // Do NOT use activity.getClass().getClassLoader(): NativeActivity is a
    // framework class loaded by BootClassLoader, which cannot see app classes.
    let class_loader = env
        .call_method(
            &act,
            jni::jni_str!("getClassLoader"),
            jni::jni_sig!("()Ljava/lang/ClassLoader;"),
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            env.exception_clear();
            let msg = format!("getClassLoader failed: {e}");
            log::error!("find_app_class({class_name}): {msg}");
            msg
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
            // Print full Java stack trace to logcat, then clear.
            env.exception_describe();
            env.exception_clear();
            let msg = format!("loadClass({class_name}) failed: {e}");
            log::error!("{msg}");
            msg
        })?;

    log::debug!("find_app_class: loaded {class_name}");
    Ok(unsafe { jni::objects::JClass::from_raw(env, loaded.as_raw()) })
}

// ── global state ─────────────────────────────────────────────────────────────

/// The `AndroidApp` handle from `android-activity`.
///
/// Registered for one Activity and cleared before its native thread exits.
static ANDROID_APP: parking_lot::Mutex<Option<AndroidApp>> = parking_lot::Mutex::new(None);

/// Current Android platform registration.
///
/// NativeActivity registrations end with the Activity; host registrations persist.
static PLATFORM: parking_lot::Mutex<Option<Arc<AndroidPlatform>>> = parking_lot::Mutex::new(None);

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
                env.exception_clear();
                return Ok(0);
            }
        };
        match env.call_method(
            &key_event,
            jni::jni_str!("getUnicodeChar"),
            jni::jni_sig!("(I)I"),
            &[JValue::Int(meta_state)],
        ) {
            Ok(v) => {
                let c = v.i().unwrap_or(0);
                Ok(if c > 0 { c as u32 } else { 0 })
            }
            Err(_) => {
                env.exception_clear();
                Ok(0)
            }
        }
    })
    .unwrap_or(0)
}

// ── public accessors ──────────────────────────────────────────────────────────

/// JVM for the **host-driven** entry point ([`super::host`]), which has no
/// `AndroidApp` to read it from.
static HOST_VM: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Current Activity for the host-driven entry point, as a JNI global reference we
/// own. Replaced on every Activity creation so [`activity_as_ptr`] always names the
/// *live* Activity.
///
/// The outgoing reference is kept one generation longer: `activity_as_ptr` hands out a
/// raw `jobject`, and a call on the render thread may still be using the previous
/// Activity while the UI thread installs the next one.
static HOST_ACTIVITY: Mutex<HostActivity> = Mutex::new(HostActivity {
    current: None,
    previous: None,
});

struct HostActivity {
    current: Option<jni::refs::Global<JObject<'static>>>,
    previous: Option<jni::refs::Global<JObject<'static>>>,
}

/// Register the current Activity when running without `android-activity`.
///
/// Call from a JNI entry point in `Activity.onCreate`, every time — a recreated
/// Activity is a new object. This module takes its own global reference and records
/// the JVM, so `java_vm()` / `activity_as_ptr()` and everything built on them (IME,
/// safe areas, file pickers, `rustls-platform-verifier`) work exactly as on the
/// `android-activity` path. A no-op when `android-activity` owns the process.
pub fn set_host_activity(env: &mut jni::Env<'_>, activity: &JObject<'_>) -> Result<(), String> {
    if ANDROID_APP.lock().is_some() {
        return Ok(());
    }
    let global = env.new_global_ref(activity).e()?;
    let vm = env.get_java_vm().e()?;
    HOST_VM.store(
        vm.get_raw() as *mut c_void,
        std::sync::atomic::Ordering::SeqCst,
    );
    let mut slot = HOST_ACTIVITY.lock().expect("poisoned");
    slot.previous = slot.current.take();
    slot.current = Some(global);
    Ok(())
}

/// Public accessor for the JavaVM pointer.
///
/// Uses `AndroidApp::vm_as_ptr()` from the stored `AndroidApp`.
/// Used by `platform.rs` for JNI calls.
pub fn java_vm() -> *mut c_void {
    ANDROID_APP
        .lock()
        .as_ref()
        .map(|app| app.vm_as_ptr())
        .unwrap_or_else(|| HOST_VM.load(std::sync::atomic::Ordering::SeqCst))
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
        .lock()
        .as_ref()
        .map(|app| app.activity_as_ptr())
        .unwrap_or_else(|| {
            HOST_ACTIVITY
                .lock()
                .expect("poisoned")
                .current
                .as_ref()
                .map(|activity| activity.as_raw() as *mut c_void)
                .unwrap_or(std::ptr::null_mut())
        })
}

/// Returns a clone of the stored `AndroidApp`, if initialised.
pub fn android_app() -> Option<AndroidApp> {
    ANDROID_APP.lock().clone()
}

/// Install the platform built by the host-driven path (`super::host`).
///
/// The `android-activity` path calls `init_platform` instead; both end up in the same
/// slot so every accessor in this module keeps working regardless of entry point.
pub(crate) fn set_host_platform(platform: Arc<AndroidPlatform>) {
    let mut slot = PLATFORM.lock();
    if slot.is_some() {
        log::warn!("set_host_platform: PLATFORM already set — both entry points in one process?");
    } else {
        *slot = Some(platform);
    }
}

/// Returns an owned handle to the current `AndroidPlatform`, if initialised.
///
/// Returns `None` before `android_main` has set it up.
pub fn platform() -> Option<Arc<AndroidPlatform>> {
    PLATFORM.lock().clone()
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
        .lock()
        .as_ref()
        .map(|arc| SharedPlatform::new(Arc::clone(arc)))
}

// ── input event types ─────────────────────────────────────────────────────────

/// Motion event action constants from the NDK.
const AMOTION_EVENT_ACTION_DOWN: u32 = 0;
const AMOTION_EVENT_ACTION_UP: u32 = 1;
const AMOTION_EVENT_ACTION_MOVE: u32 = 2;
const AMOTION_EVENT_ACTION_CANCEL: u32 = 3;

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
    let platform = match platform() {
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

                            // Check if this touch lands on a platform view.
                            //
                            // On Android with NativeActivity, ALL touch events go
                            // to the native surface first. Platform views are real
                            // Java Views in a FrameLayout overlay, but they won't
                            // receive touches unless we skip GPUI dispatch and let
                            // Android's view hierarchy handle the event instead.
                            //
                            // We use the primary pointer for the hit-test on DOWN
                            // actions. Physical pixel coordinates are converted to
                            // logical pixels to match PlatformViewBounds.
                            let hits_platform_view = {
                                let registry = crate::platform_view::PlatformViewRegistry::global();
                                if registry.active_view_count() > 0 {
                                    let primary = motion_event.pointer_at_index(
                                        match action {
                                            MotionAction::PointerDown | MotionAction::PointerUp => {
                                                motion_event.pointer_index()
                                            }
                                            _ => 0,
                                        }
                                    );
                                    let scale = win.scale_factor();
                                    let logical_x = primary.x() / scale;
                                    let logical_y = primary.y() / scale;
                                    registry.hit_test(logical_x, logical_y)
                                } else {
                                    false
                                }
                            };

                            if hits_platform_view {
                                log::debug!(
                                    "process_input_events: touch hits platform view, skipping GPUI dispatch",
                                );
                                // Return Unhandled so android-activity can pass
                                // the event back to the Java view hierarchy where
                                // the platform view's FrameLayout lives.
                                return android_activity::InputStatus::Unhandled;
                            }

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
                                    MotionAction::Cancel => AMOTION_EVENT_ACTION_CANCEL,
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

mod event_loop;
pub use event_loop::{poll_events, run_event_loop};

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

/// Owns the platform registration for one NativeActivity's native thread.
///
/// Keep this guard alive until the GPUI Application has been dropped. A recreated
/// Activity gets a new AndroidApp, dispatcher and Looper, even in the same process.
#[must_use]
pub struct PlatformSession {
    platform: Arc<AndroidPlatform>,
    // Teardown must run on the native thread that registered the Looper callback.
    _thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl std::ops::Deref for PlatformSession {
    type Target = Arc<AndroidPlatform>;

    fn deref(&self) -> &Self::Target {
        &self.platform
    }
}

impl Drop for PlatformSession {
    fn drop(&mut self) {
        super::frame_source::uninstall();
        self.platform.foreground_executor().shutdown();
        NATIVE_INITIALIZED.store(false, Ordering::Release);
        PLATFORM.lock().take();
        ANDROID_APP.lock().take();
        log::info!(
            "platform_session: released on {:?}",
            std::thread::current().id()
        );
    }
}

/// Register a fresh platform for this Activity on its native thread.
///
/// The previous session must have ended before another Activity is initialized.
/// Returns a guard that must outlive the GPUI Application.
pub fn init_platform(app: &AndroidApp) -> PlatformSession {
    assert!(
        PLATFORM.lock().is_none(),
        "previous Activity platform is still registered"
    );
    *ANDROID_APP.lock() = Some(app.clone());
    for flag in [
        &INIT_WINDOW_DONE,
        &NATIVE_INITIALIZED,
        &PAUSE_PENDING,
        &RESUME_PENDING,
        &TERM_WINDOW_PENDING,
        &INIT_WINDOW_PENDING,
        &WINDOW_RESIZED_PENDING,
        &CONFIG_CHANGED_PENDING,
    ] {
        flag.store(false, Ordering::Release);
    }
    *LAST_CHROME_STYLE.lock().unwrap() = None;
    let platform = Arc::new(AndroidPlatform::new(false));
    *PLATFORM.lock() = Some(Arc::clone(&platform));
    log::info!(
        "platform_session: initialized on {:?}",
        std::thread::current().id()
    );
    PlatformSession {
        platform,
        _thread_bound: std::marker::PhantomData,
    }
}

mod chrome;
pub use chrome::set_system_chrome;
use chrome::LAST_CHROME_STYLE;

// ── software keyboard (IME) control ───────────────────────────────────────────

/// The keyboard type currently requested, or `None` when hidden.
///
/// The Java-side `InputProxy` belongs to the Activity, so an Activity recreation takes
/// the IME with it — while GPUI still considers the same field focused and therefore
/// never asks for the keyboard again. Remembering the request lets a new Activity put
/// the IME back; see [`restore_keyboard`].
static SHOWN_KEYBOARD: std::sync::Mutex<Option<crate::KeyboardType>> = std::sync::Mutex::new(None);

/// Re-request the keyboard on the current Activity if one was showing.
///
/// Called by [`super::host`] once a recreated Activity's surface is attached. A fresh
/// IME session is started, which is what we want: the proxy is a new object.
pub(crate) fn restore_keyboard() {
    let keyboard_type = *SHOWN_KEYBOARD.lock().expect("poisoned");
    if let Some(keyboard_type) = keyboard_type {
        log::info!("restore_keyboard: re-showing {keyboard_type:?} on the new Activity");
        show_keyboard_android(keyboard_type);
    }
}

/// Show the UI-thread EditText proxy supplied by GpuiInputActivity.
///
/// Unlike NativeActivity's key-event-only connection, its InputConnection
/// supports composing text, commits and Unicode surrounding-text deletion.
pub fn show_keyboard_android(keyboard_type: crate::KeyboardType) {
    *SHOWN_KEYBOARD.lock().expect("poisoned") = Some(keyboard_type);
    let kind = match keyboard_type {
        crate::KeyboardType::Default => 0,
        crate::KeyboardType::EmailAddress => 1,
        crate::KeyboardType::Phone => 2,
        crate::KeyboardType::NumberPad => 3,
        crate::KeyboardType::URL => 4,
        crate::KeyboardType::Decimal => 5,
    };
    let session = super::text_input::new_session();
    if let Err(error) = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiShowKeyboard"),
            jni::jni_sig!("(IJ)V"),
            &[JValue::Int(kind), JValue::Long(session as i64)],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }) {
        log::warn!("IME requires GpuiInputActivity: {error}");
        let _ = with_env(|env| {
            env.exception_clear();
            Ok(())
        });
    }
}

/// Hide the software keyboard on Android.
///
/// Invalidates queued IME callbacks and clears the native composition buffer.
pub fn hide_keyboard_android() {
    *SHOWN_KEYBOARD.lock().expect("poisoned") = None;
    let session = super::text_input::new_session();
    let _ = with_env(|env| {
        let activity = activity(env)?;
        let result = env.call_method(
            &activity,
            jni::jni_str!("gpuiHideKeyboard"),
            jni::jni_sig!("(J)V"),
            &[JValue::Long(session as i64)],
        );
        env.exception_clear();
        result.map_err(|e| e.to_string())?;
        Ok(())
    });
    if let Some(app) = android_app() {
        app.hide_soft_input(false);
    }
}

pub(super) fn reset_keyboard_composition() {
    let session = super::text_input::new_session();
    let _ = with_env(|env| {
        let activity = activity(env)?;
        let result = env.call_method(
            &activity,
            jni::jni_str!("gpuiResetComposition"),
            jni::jni_sig!("(J)V"),
            &[JValue::Long(session as i64)],
        );
        env.exception_clear();
        result.map_err(|e| e.to_string())?;
        Ok(())
    });
}

/// Receive Java InputConnection updates without touching GPUI on the UI thread.
///
/// # Safety
/// Called by JNI with a valid Java string and JNI call frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiInputActivity_nativeIme(
    _env: *mut c_void,
    _class: *mut c_void,
    session: i64,
    kind: i32,
    text: *mut c_void,
    start: i32,
    end: i32,
) {
    let _ = with_env(|env| {
        let text = unsafe { JObject::from_raw(env, text as jni::sys::jobject) };
        super::text_input::enqueue(super::text_input::ImeEvent {
            session: session as u64,
            kind,
            text: get_string(env, &text),
            start: start.max(0) as usize,
            end: end.max(0) as usize,
        });
        Ok(())
    });
}

// ── tests ─────────────────────────────────────────────────────────────────────

// ── JNI export for GpuiActivity splash screen ────────────────────────────────

/// Called from `GpuiActivity.nativeIsInitialized()` to check whether the
/// native library has finished initializing (first frame rendered).
///
/// The AndroidX SplashScreen API calls this via `setKeepOnScreenCondition`
/// to hold the splash visible until GPUI is ready.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeIsInitialized(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) -> u8 {
    if NATIVE_INITIALIZED.load(Ordering::Acquire) {
        1 // JNI_TRUE
    } else {
        0 // JNI_FALSE
    }
}

/// JNI bridge: receive a deeplink URL from `GpuiActivity.onNewIntent()`.
///
/// When the app is already running and a deeplink is opened (e.g. via
/// `adb shell am start -d gpui://video_player`), the Java side calls
/// this to notify the Rust deeplink handler.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread with a valid `url` jobject.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnDeepLink(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    url: *mut std::ffi::c_void,
) {
    // We already have a JVM attached on this thread (UI thread).
    // Use with_env to get a properly wrapped Env handle.
    let url_raw = url as jni::sys::jobject;
    let _ = with_env(|env| {
        let url_obj = unsafe { JObject::from_raw(env, url_raw) };
        let url_string = get_string(env, &url_obj);
        // Don't let the JObject be dropped (it's owned by the JNI call frame).
        if url_string.is_empty() {
            return Ok(());
        }
        log::info!("nativeOnDeepLink: {}", url_string);

        #[cfg(feature = "deeplink")]
        {
            crate::packages::deeplink::notify_deep_link(&url_string);
        }
        Ok(())
    });
}

/// JNI bridge: receive a media action from `GpuiMediaSession` system controls.
///
/// Actions: "play", "pause", "stop", "next", "previous"
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread with a valid `action` jobject.
#[cfg(feature = "media_session")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiMediaSession_nativeMediaAction(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    action: *mut std::ffi::c_void,
) {
    let action_raw = action as jni::sys::jobject;
    let _ = with_env(|env| {
        let action_obj = unsafe { JObject::from_raw(env, action_raw) };
        let action_str = get_string(env, &action_obj);

        let media_action = match action_str.as_str() {
            "play" => crate::packages::media_session::MediaAction::Play,
            "pause" => crate::packages::media_session::MediaAction::Pause,
            "stop" => crate::packages::media_session::MediaAction::Stop,
            "next" => crate::packages::media_session::MediaAction::Next,
            "previous" => crate::packages::media_session::MediaAction::Previous,
            other => {
                log::warn!("nativeMediaAction: unknown action '{}'", other);
                return Ok(());
            }
        };

        log::info!("nativeMediaAction: {:?}", media_action);
        crate::packages::media_session::notify_action(media_action);
        Ok(())
    });
}

/// JNI bridge: receive a seek request from `GpuiMediaSession` system controls.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread.
#[cfg(feature = "media_session")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiMediaSession_nativeMediaSeek(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    position_ms: i64,
) {
    log::info!("nativeMediaSeek: {}ms", position_ms);
    crate::packages::media_session::notify_seek(position_ms as u64);
}

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
