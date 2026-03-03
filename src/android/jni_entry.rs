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
#![allow(dead_code)]

use std::{
    ffi::c_void,
    sync::{Arc, OnceLock},
    time::Duration,
};

use android_activity::{AndroidApp, MainEvent, PollEvent};

use super::platform::{AndroidPlatform, SharedPlatform};

// ── JNI types for unicode character extraction ────────────────────────────────

/// Raw JNI environment pointer (`JNIEnv *`).
type JNIEnvPtr = *mut c_void;
/// Raw JNI JavaVM pointer.
type JavaVMPtr = *mut c_void;

// ── global state ─────────────────────────────────────────────────────────────

/// The `AndroidApp` handle from `android-activity`.
///
/// Set once in `android_main`; read-only thereafter.
static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

/// Process-global `AndroidPlatform` instance.
///
/// Initialised once during `android_main`; read-only thereafter.
static PLATFORM: OnceLock<Arc<AndroidPlatform>> = OnceLock::new();

// ── JNI function table offsets (from `JNINativeInterface`) ────────────────────

/// Raw JNI helper functions used by `platform.rs` and `window.rs` for
/// JNI calls that aren't covered by the `jni` crate.
pub mod jni_fns {
    use std::ffi::c_void;

    /// Attach the current thread to the JVM and return a `JNIEnv *`.
    ///
    /// JNIEnv is a pointer to a pointer to a function table.
    pub unsafe fn get_env_from_vm(vm: *mut c_void) -> *mut c_void {
        // JavaVM is `*const *const JNIInvokeInterface`
        #[repr(C)]
        struct JNIInvokeInterface {
            reserved0: *mut c_void,
            reserved1: *mut c_void,
            reserved2: *mut c_void,
            destroy_java_vm: *mut c_void,
            attach_current_thread:
                unsafe extern "C" fn(*mut c_void, *mut *mut c_void, *mut c_void) -> i32,
            detach_current_thread: *mut c_void,
            get_env: unsafe extern "C" fn(*mut c_void, *mut *mut c_void, i32) -> i32,
        }

        let vm_table = *(vm as *const *const JNIInvokeInterface);

        // First try GetEnv (JNI_VERSION_1_6 = 0x00010006)
        let mut env: *mut c_void = std::ptr::null_mut();
        let result = unsafe { ((*vm_table).get_env)(vm, &mut env, 0x00010006) };
        if result == 0 && !env.is_null() {
            return env;
        }

        // Thread not attached — attach it
        let result =
            unsafe { ((*vm_table).attach_current_thread)(vm, &mut env, std::ptr::null_mut()) };
        if result == 0 && !env.is_null() {
            env
        } else {
            std::ptr::null_mut()
        }
    }

    /// Get the unicode character from an Android KeyEvent via JNI.
    ///
    /// Calls `android.view.KeyEvent(action, code).getUnicodeChar(metaState)`.
    ///
    /// Returns 0 if the JNI call fails or the key produces no character.
    pub unsafe fn get_unicode_char(
        env: *mut c_void,
        key_code: i32,
        action: i32,
        meta_state: i32,
    ) -> u32 {
        // The env pointer is `JNIEnv **` → dereference once to get the fn table.
        let fn_table = *(env as *const *const *const c_void);

        macro_rules! jni_fn {
            ($idx:expr, $ty:ty) => {
                std::mem::transmute::<*const c_void, $ty>(*fn_table.add($idx))
            };
        }

        type FindClassFn = unsafe extern "C" fn(*mut c_void, *const i8) -> *mut c_void;
        type GetMethodIDFn =
            unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8, *const i8) -> *mut c_void;
        type NewObjectAFn =
            unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i64) -> *mut c_void;
        type CallIntMethodAFn =
            unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i64) -> i32;
        type DeleteLocalRefFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
        type ExceptionCheckFn = unsafe extern "C" fn(*mut c_void) -> u8;
        type ExceptionClearFn = unsafe extern "C" fn(*mut c_void);

        let find_class: FindClassFn = jni_fn!(6, FindClassFn);
        let get_method_id: GetMethodIDFn = jni_fn!(33, GetMethodIDFn);
        let new_object_a: NewObjectAFn = jni_fn!(30, NewObjectAFn);
        let call_int_method_a: CallIntMethodAFn = jni_fn!(49, CallIntMethodAFn);
        let delete_local_ref: DeleteLocalRefFn = jni_fn!(23, DeleteLocalRefFn);
        let exception_check: ExceptionCheckFn = jni_fn!(228, ExceptionCheckFn);
        let exception_clear: ExceptionClearFn = jni_fn!(17, ExceptionClearFn);

        // Step 1: FindClass
        let class_name = b"android/view/KeyEvent\0";
        let cls = find_class(env, class_name.as_ptr() as *const i8);
        if cls.is_null() {
            if exception_check(env) != 0 {
                exception_clear(env);
            }
            return 0;
        }

        // Step 2: Get constructor <init>(int action, int code)
        let init_name = b"<init>\0";
        let init_sig = b"(II)V\0";
        let ctor = get_method_id(
            env,
            cls,
            init_name.as_ptr() as *const i8,
            init_sig.as_ptr() as *const i8,
        );
        if ctor.is_null() {
            if exception_check(env) != 0 {
                exception_clear(env);
            }
            delete_local_ref(env, cls);
            return 0;
        }

        // Step 3: NewObject — jvalue args are 8 bytes each
        let ctor_args: [i64; 2] = [action as i64, key_code as i64];
        let key_event_obj = new_object_a(env, cls, ctor, ctor_args.as_ptr());
        if key_event_obj.is_null() {
            if exception_check(env) != 0 {
                exception_clear(env);
            }
            delete_local_ref(env, cls);
            return 0;
        }

        // Step 4: Get getUnicodeChar(int metaState) method
        let method_name = b"getUnicodeChar\0";
        let method_sig = b"(I)I\0";
        let get_unicode_method = get_method_id(
            env,
            cls,
            method_name.as_ptr() as *const i8,
            method_sig.as_ptr() as *const i8,
        );
        if get_unicode_method.is_null() {
            if exception_check(env) != 0 {
                exception_clear(env);
            }
            delete_local_ref(env, key_event_obj);
            delete_local_ref(env, cls);
            return 0;
        }

        // Step 5: Call getUnicodeChar(metaState)
        let call_args: [i64; 1] = [meta_state as i64];
        let unicode = call_int_method_a(env, key_event_obj, get_unicode_method, call_args.as_ptr());

        if exception_check(env) != 0 {
            exception_clear(env);
            delete_local_ref(env, key_event_obj);
            delete_local_ref(env, cls);
            return 0;
        }

        // Cleanup local references
        delete_local_ref(env, key_event_obj);
        delete_local_ref(env, cls);

        if unicode > 0 {
            unicode as u32
        } else {
            0
        }
    }
}

// ── JNI helper accessors ──────────────────────────────────────────────────────

/// Obtain a JNI environment for the current thread.
///
/// Attaches the current thread to the JVM if not already attached.
/// Returns null if no `AndroidApp` has been stored yet.
fn get_jni_env() -> JNIEnvPtr {
    let vm = java_vm();
    if vm.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { jni_fns::get_env_from_vm(vm) }
}

/// Get the unicode character produced by an Android key event via JNI.
///
/// This creates a `android.view.KeyEvent` Java object and calls
/// `getUnicodeChar(metaState)` on it.  Returns 0 on failure.
pub fn unicode_char_for_key_event(key_code: i32, action: i32, meta_state: i32) -> u32 {
    let env = get_jni_env();
    if env.is_null() {
        return 0;
    }
    unsafe { jni_fns::get_unicode_char(env, key_code, action, meta_state) }
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
/// let platform = jni_entry::shared_platform().unwrap();
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

// ── night mode query via JNI ──────────────────────────────────────────────────

/// Query the current night mode from the activity's Configuration via JNI.
///
/// Returns `true` if the system is in dark mode.
pub fn query_night_mode_via_jni() -> bool {
    let vm = java_vm();
    if vm.is_null() {
        return false;
    }

    let env = unsafe { jni_fns::get_env_from_vm(vm) };
    if env.is_null() {
        return false;
    }

    let activity_obj = activity_as_ptr();
    if activity_obj.is_null() {
        return false;
    }

    unsafe { query_night_mode_impl(env, activity_obj) }
}

/// Internal implementation of night mode query.
///
/// # Safety
///
/// `env` must be a valid `JNIEnv *` and `activity_obj` must be a valid
/// Activity jobject.
unsafe fn query_night_mode_impl(env: *mut c_void, activity_obj: *mut c_void) -> bool {
    let fn_table = *(env as *const *const *const c_void);

    macro_rules! jni_fn {
        ($idx:expr, $ty:ty) => {
            std::mem::transmute::<*const c_void, $ty>(*fn_table.add($idx))
        };
    }

    type FindClassFn = unsafe extern "C" fn(*mut c_void, *const i8) -> *mut c_void;
    type GetMethodIDFn =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8, *const i8) -> *mut c_void;
    type CallObjectMethodAFn =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i64) -> *mut c_void;
    type GetFieldIDFn =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8, *const i8) -> *mut c_void;
    type GetIntFieldFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32;
    type DeleteLocalRefFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
    type ExceptionCheckFn = unsafe extern "C" fn(*mut c_void) -> u8;
    type ExceptionClearFn = unsafe extern "C" fn(*mut c_void);

    let find_class: FindClassFn = jni_fn!(6, FindClassFn);
    let get_method_id: GetMethodIDFn = jni_fn!(33, GetMethodIDFn);
    let call_object_method_a: CallObjectMethodAFn = jni_fn!(36, CallObjectMethodAFn);
    let get_field_id: GetFieldIDFn = jni_fn!(94, GetFieldIDFn);
    let get_int_field: GetIntFieldFn = jni_fn!(100, GetIntFieldFn);
    let delete_local_ref: DeleteLocalRefFn = jni_fn!(23, DeleteLocalRefFn);
    let exception_check: ExceptionCheckFn = jni_fn!(228, ExceptionCheckFn);
    let exception_clear: ExceptionClearFn = jni_fn!(17, ExceptionClearFn);

    let no_args: [i64; 0] = [];

    // 1. activity.getResources()
    let activity_cls = find_class(env, b"android/app/Activity\0".as_ptr() as *const i8);
    if activity_cls.is_null() {
        if exception_check(env) != 0 {
            exception_clear(env);
        }
        return false;
    }

    let get_resources = get_method_id(
        env,
        activity_cls,
        b"getResources\0".as_ptr() as *const i8,
        b"()Landroid/content/res/Resources;\0".as_ptr() as *const i8,
    );
    if get_resources.is_null() {
        if exception_check(env) != 0 {
            exception_clear(env);
        }
        delete_local_ref(env, activity_cls);
        return false;
    }

    let resources = call_object_method_a(env, activity_obj, get_resources, no_args.as_ptr());
    delete_local_ref(env, activity_cls);

    if resources.is_null() {
        if exception_check(env) != 0 {
            exception_clear(env);
        }
        return false;
    }

    // 2. resources.getConfiguration()
    let resources_cls = find_class(
        env,
        b"android/content/res/Resources\0".as_ptr() as *const i8,
    );
    if resources_cls.is_null() {
        if exception_check(env) != 0 {
            exception_clear(env);
        }
        delete_local_ref(env, resources);
        return false;
    }

    let get_config = get_method_id(
        env,
        resources_cls,
        b"getConfiguration\0".as_ptr() as *const i8,
        b"()Landroid/content/res/Configuration;\0".as_ptr() as *const i8,
    );
    if get_config.is_null() {
        if exception_check(env) != 0 {
            exception_clear(env);
        }
        delete_local_ref(env, resources_cls);
        delete_local_ref(env, resources);
        return false;
    }

    let config = call_object_method_a(env, resources, get_config, no_args.as_ptr());
    delete_local_ref(env, resources_cls);
    delete_local_ref(env, resources);

    if config.is_null() {
        if exception_check(env) != 0 {
            exception_clear(env);
        }
        return false;
    }

    // 3. config.uiMode & UI_MODE_NIGHT_MASK
    let config_cls = find_class(
        env,
        b"android/content/res/Configuration\0".as_ptr() as *const i8,
    );
    if config_cls.is_null() {
        if exception_check(env) != 0 {
            exception_clear(env);
        }
        delete_local_ref(env, config);
        return false;
    }

    let ui_mode_field = get_field_id(
        env,
        config_cls,
        b"uiMode\0".as_ptr() as *const i8,
        b"I\0".as_ptr() as *const i8,
    );
    if ui_mode_field.is_null() {
        if exception_check(env) != 0 {
            exception_clear(env);
        }
        delete_local_ref(env, config_cls);
        delete_local_ref(env, config);
        return false;
    }

    let ui_mode = get_int_field(env, config, ui_mode_field);
    delete_local_ref(env, config_cls);
    delete_local_ref(env, config);

    if exception_check(env) != 0 {
        exception_clear(env);
        return false;
    }

    const UI_MODE_NIGHT_MASK: i32 = 0x30;
    const UI_MODE_NIGHT_YES: i32 = 0x20;

    let is_dark = (ui_mode & UI_MODE_NIGHT_MASK) == UI_MODE_NIGHT_YES;
    log::debug!(
        "query_night_mode_via_jni: uiMode={:#x} is_dark={}",
        ui_mode,
        is_dark
    );
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
        if let Some(platform) = PLATFORM.get() {
            // Flush pending main-thread tasks first so that any
            // state changes (e.g. model updates, layout
            // invalidations) are applied before we paint.
            platform.flush_main_thread_tasks();

            // Only drive rendering when the app is active / in the foreground.
            // When backgrounded, the surface may be invalid and request_frame
            // can block or panic.  We still flush main-thread tasks above so
            // that pending work doesn't pile up.
            if app_is_active {
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
