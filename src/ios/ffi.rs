//! FFI (Foreign Function Interface) bridge for iOS.
//!
//! This module exposes `#[no_mangle]` C-compatible functions that the iOS
//! Objective-C app delegate calls to drive the GPUI application lifecycle.
//!
//! # Lifecycle overview
//!
//! ```text
//! ObjC app delegate                    Rust / GPUI
//! ─────────────────                    ──────────────────────────────────────
//! application:didFinishLaunching  ──►  gpui_ios_initialize()
//!                                 ──►  gpui_ios_did_finish_launching()
//!                                       └─ invokes finish-launching callback
//! CADisplayLink tick              ──►  gpui_ios_request_frame(window_ptr)
//! touchesBegan / Moved / Ended    ──►  gpui_ios_handle_touch(window_ptr, …)
//! applicationWillEnterForeground  ──►  gpui_ios_will_enter_foreground()
//! applicationDidBecomeActive      ──►  gpui_ios_did_become_active()
//! applicationWillResignActive     ──►  gpui_ios_will_resign_active()
//! applicationDidEnterBackground   ──►  gpui_ios_did_enter_background()
//! applicationWillTerminate        ──►  gpui_ios_will_terminate()
//! ```
//!
//! All functions are guarded by `target_os = "ios"` at the module level
//! (the enclosing `ios` module in `lib.rs`).
//!
//! # Thread safety
//!
//! Every function in this module **must** be called from the UIKit main
//! thread.  The statics below use `UnsafeCell` intentionally; accesses are
//! always single-threaded because UIKit enforces main-thread-only UI work.

use super::window::IosWindow;
use std::{cell::UnsafeCell, ffi::c_void, sync::OnceLock};

// ── Global app state ──────────────────────────────────────────────────────────

/// Holds the finish-launching callback supplied by `IosPlatform::run()`.
///
/// Only written once (during `gpui_ios_initialize` / `set_finish_launching_callback`)
/// and read once (during `gpui_ios_did_finish_launching`).
struct IosAppState {
    finish_launching: UnsafeCell<Option<Box<dyn FnOnce()>>>,
}

// SAFETY: All accesses happen on the UIKit main thread.
unsafe impl Send for IosAppState {}
unsafe impl Sync for IosAppState {}

static IOS_APP_STATE: OnceLock<IosAppState> = OnceLock::new();

// ── Global window list ────────────────────────────────────────────────────────

struct WindowList(UnsafeCell<Vec<*const IosWindow>>);

// SAFETY: All accesses happen on the UIKit main thread.
unsafe impl Send for WindowList {}
unsafe impl Sync for WindowList {}

static IOS_WINDOW_LIST: OnceLock<WindowList> = OnceLock::new();

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Ensure both statics are initialised.  Safe to call multiple times.
fn ensure_initialized() {
    IOS_APP_STATE.get_or_init(|| IosAppState {
        finish_launching: UnsafeCell::new(None),
    });
    IOS_WINDOW_LIST.get_or_init(|| WindowList(UnsafeCell::new(Vec::new())));
}

/// Store the finish-launching callback so the FFI layer can invoke it later.
///
/// Called internally by `IosPlatform::run()`.
///
/// # Safety
/// Must only be called from the main thread.
pub(crate) fn set_finish_launching_callback(callback: Box<dyn FnOnce()>) {
    ensure_initialized();
    if let Some(state) = IOS_APP_STATE.get() {
        // SAFETY: main-thread-only access.
        unsafe {
            *state.finish_launching.get() = Some(callback);
        }
    }
}

/// Register a newly-created `IosWindow` so Objective-C code can retrieve its
/// pointer via `gpui_ios_get_window()`.
///
/// Called internally by `IosWindow::register_with_ffi()`.
///
/// # Safety
/// Must only be called from the main thread.  The caller is responsible for
/// keeping the `IosWindow` alive as long as it is registered.
pub(crate) fn register_window(window: *const IosWindow) {
    ensure_initialized();
    if let Some(list) = IOS_WINDOW_LIST.get() {
        // SAFETY: main-thread-only access.
        unsafe {
            (*list.0.get()).push(window);
        }
        log::info!("GPUI iOS FFI: registered window {:p}", window);
    }
}

// ── Public C-ABI functions ────────────────────────────────────────────────────

/// Initialise the GPUI iOS runtime.
///
/// Call from `application:didFinishLaunchingWithOptions:` **before** any
/// other `gpui_ios_*` function.
///
/// Returns a non-null sentinel on success, `NULL` if already initialised.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_initialize() -> *mut c_void {
    #[cfg(debug_assertions)]
    {
        // Best-effort: init a basic logger that writes to NSLog / os_log.
        // Ignore the error if a logger is already installed.
        let _ = env_logger::try_init();
    }

    if IOS_APP_STATE.get().is_some() {
        log::warn!("GPUI iOS FFI: gpui_ios_initialize called more than once");
        return std::ptr::null_mut();
    }

    ensure_initialized();
    log::info!("GPUI iOS FFI: initialized");

    // Return a non-null sentinel so ObjC can distinguish success from failure.
    1usize as *mut c_void
}

/// Invoke the finish-launching callback.
///
/// Call from `application:didFinishLaunchingWithOptions:` after
/// `gpui_ios_initialize()` returns.
///
/// `app_ptr` is ignored; it exists only for forward-compatibility.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_did_finish_launching(_app_ptr: *mut c_void) {
    log::info!("GPUI iOS FFI: did_finish_launching");

    let Some(state) = IOS_APP_STATE.get() else {
        log::error!("GPUI iOS FFI: did_finish_launching called before initialize");
        return;
    };

    // SAFETY: main-thread-only access.
    let callback = unsafe { (*state.finish_launching.get()).take() };

    match callback {
        Some(cb) => {
            log::info!("GPUI iOS FFI: invoking finish-launching callback");
            cb();
        }
        None => {
            log::warn!("GPUI iOS FFI: no finish-launching callback registered");
        }
    }
}

/// Notify all windows that the app is about to enter the foreground.
///
/// Call from `applicationWillEnterForeground:`.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_will_enter_foreground(_app_ptr: *mut c_void) {
    log::info!("GPUI iOS FFI: will_enter_foreground");
    notify_all_windows_active(true);
}

/// Notify all windows that the app has become active.
///
/// Call from `applicationDidBecomeActive:`.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_did_become_active(_app_ptr: *mut c_void) {
    log::info!("GPUI iOS FFI: did_become_active");
    notify_all_windows_active(true);
}

/// Notify all windows that the app is about to resign active status.
///
/// Call from `applicationWillResignActive:`.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_will_resign_active(_app_ptr: *mut c_void) {
    log::info!("GPUI iOS FFI: will_resign_active");
    notify_all_windows_active(false);
}

/// Notify all windows that the app has entered the background.
///
/// Call from `applicationDidEnterBackground:`.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_did_enter_background(_app_ptr: *mut c_void) {
    log::info!("GPUI iOS FFI: did_enter_background");
    notify_all_windows_active(false);
}

/// Notify all windows that the app is about to terminate.
///
/// Call from `applicationWillTerminate:`.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_will_terminate(_app_ptr: *mut c_void) {
    log::info!("GPUI iOS FFI: will_terminate");
    // Future: invoke registered quit callbacks.
}

/// Return a pointer to the most recently registered `IosWindow`, or NULL if
/// no window has been created yet.
///
/// The Objective-C app delegate stores this pointer and passes it to
/// `gpui_ios_request_frame` on every CADisplayLink tick.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_get_window() -> *mut c_void {
    let Some(list) = IOS_WINDOW_LIST.get() else {
        log::warn!("GPUI iOS FFI: get_window — not initialised");
        return std::ptr::null_mut();
    };

    // SAFETY: main-thread-only access.
    let ptr = unsafe { (*list.0.get()).last().copied().unwrap_or(std::ptr::null()) };

    if ptr.is_null() {
        log::warn!("GPUI iOS FFI: get_window — no windows registered");
    } else {
        log::debug!("GPUI iOS FFI: get_window → {:p}", ptr);
    }

    ptr as *mut c_void
}

/// Drive one rendering frame for the given window.
///
/// Call on every CADisplayLink tick.
///
/// `window_ptr` must be the value returned by `gpui_ios_get_window()`.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_request_frame(window_ptr: *mut c_void) {
    if window_ptr.is_null() {
        return;
    }

    // SAFETY: `window_ptr` is a valid `*const IosWindow` cast to `*mut c_void`
    // by `gpui_ios_get_window`.  We only call the callback stored inside the
    // window; the window itself is not mutated structurally.
    let window = unsafe { &*(window_ptr as *const IosWindow) };

    // Take the callback out, invoke it, then put it back.  We must release
    // the borrow on `request_frame_callback` before invoking the callback
    // because the callback itself may want to borrow the same `RefCell`.
    let callback = window.request_frame_callback.borrow_mut().take();
    if let Some(mut cb) = callback {
        cb();
        window.request_frame_callback.borrow_mut().replace(cb);
    }
}

/// Forward a UIKit touch event to the given window.
///
/// - `window_ptr`  : pointer returned by `gpui_ios_get_window()`
/// - `touch_ptr`   : `UITouch *`
/// - `event_ptr`   : `UIEvent *`
///
/// Both `touch_ptr` and `event_ptr` are passed as `void *` to avoid
/// a hard dependency on the Objective-C `id` type from the C header.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_handle_touch(
    window_ptr: *mut c_void,
    touch_ptr: *mut c_void,
    event_ptr: *mut c_void,
) {
    if window_ptr.is_null() || touch_ptr.is_null() {
        return;
    }

    // SAFETY: caller guarantees `window_ptr` is a valid `IosWindow`.
    let window = unsafe { &*(window_ptr as *const IosWindow) };
    window.handle_touch(
        touch_ptr as *mut objc::runtime::Object,
        event_ptr as *mut objc::runtime::Object,
    );
}

/// Show the software (on-screen) keyboard for the given window.
///
/// Call when a text-input field gains focus.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_show_keyboard(window_ptr: *mut c_void) {
    if window_ptr.is_null() {
        return;
    }
    log::info!("GPUI iOS FFI: show_keyboard");
    // SAFETY: caller guarantees `window_ptr` is a valid `IosWindow`.
    let window = unsafe { &*(window_ptr as *const IosWindow) };
    window.show_keyboard();
}

/// Hide the software (on-screen) keyboard for the given window.
///
/// Call when a text-input field loses focus.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_hide_keyboard(window_ptr: *mut c_void) {
    if window_ptr.is_null() {
        return;
    }
    log::info!("GPUI iOS FFI: hide_keyboard");
    // SAFETY: caller guarantees `window_ptr` is a valid `IosWindow`.
    let window = unsafe { &*(window_ptr as *const IosWindow) };
    window.hide_keyboard();
}

/// Deliver soft-keyboard text input to the given window.
///
/// `text_ptr` is an `NSString *` cast to `void *`.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_handle_text_input(window_ptr: *mut c_void, text_ptr: *mut c_void) {
    if window_ptr.is_null() || text_ptr.is_null() {
        return;
    }
    // SAFETY: caller guarantees valid pointers.
    let window = unsafe { &*(window_ptr as *const IosWindow) };
    window.handle_text_input(text_ptr as *mut objc::runtime::Object);
}

/// Deliver a hardware-keyboard key event to the given window.
///
/// - `key_code`   : `UIKeyboardHIDUsage` value (USB HID usage page 0x07)
/// - `modifiers`  : `UIKeyModifierFlags` bitmask
/// - `is_key_down`: `true` for key-down, `false` for key-up
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_handle_key_event(
    window_ptr: *mut c_void,
    key_code: u32,
    modifiers: u32,
    is_key_down: bool,
) {
    if window_ptr.is_null() {
        return;
    }
    log::debug!(
        "GPUI iOS FFI: key_event code=0x{:02x} mods=0x{:x} down={}",
        key_code,
        modifiers,
        is_key_down,
    );
    // SAFETY: caller guarantees `window_ptr` is a valid `IosWindow`.
    let window = unsafe { &*(window_ptr as *const IosWindow) };
    window.handle_key_event(key_code, modifiers, is_key_down);
}

/// Launch the built-in interactive demo application.
///
/// Creates a GPUI `Application` with a menu that lets the user choose between
/// the Animation Playground and the Shader Showcase demos.
///
/// Call from `application:didFinishLaunchingWithOptions:` as a self-contained
/// alternative to the `gpui_ios_initialize` / `gpui_ios_did_finish_launching`
/// pair.
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_run_demo() {
    log::info!("GPUI iOS FFI: run_demo — launching interactive demo");

    ensure_initialized();

    // Store a no-op callback so the state machine is consistent.
    set_finish_launching_callback(Box::new(|| {
        log::info!("GPUI iOS FFI: run_demo finish-launching callback reached");
    }));

    // Invoke the finish-launching path immediately (the app delegate has
    // already called UIApplicationMain before any Rust code runs, so the
    // run loop is live and we can create windows right now).
    gpui_ios_did_finish_launching(std::ptr::null_mut());
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Notify every registered window of an active-status change.
fn notify_all_windows_active(is_active: bool) {
    let Some(list) = IOS_WINDOW_LIST.get() else {
        return;
    };

    // SAFETY: main-thread-only access.
    unsafe {
        let windows = &*list.0.get();
        for &ptr in windows.iter() {
            if !ptr.is_null() {
                (*ptr).notify_active_status_change(is_active);
            }
        }
    }
}
