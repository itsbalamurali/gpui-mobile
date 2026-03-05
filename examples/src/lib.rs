//! Cross-platform GPUI example app.
//!
//! This crate provides a multi-screen GPUI demo app that runs on both
//! **Android** and **iOS**.  The UI code (screens, router, navigation) is
//! fully shared; only the platform initialisation differs.
//!
//! ## Screens
//!
//! - **Home** — welcome message, colour swatches, stats, and quick-nav cards.
//! - **Counter** — increment / decrement / reset a shared tap counter.
//! - **Settings** — toggle dark mode, reset counter, change user name.
//! - **About** — app info, technology stack, architecture, and credits.
//!
//! ## Entry points
//!
//! ### Android
//!
//! This crate defines `android_main` directly — the `android-activity` crate
//! calls it on a dedicated native thread after loading the `.so`.
//!
//! ### iOS
//!
//! The companion `main.rs` binary calls [`ios_main`] which creates an
//! `IosPlatform`, opens a fullscreen window with the `Router`, and hands
//! control to the GPUI run loop.
//!
//! ## Window lifecycle
//!
//! On Android, windows are NOT created by calling `cx.open_window(...)` at
//! startup.  Instead, the system delivers a `MainEvent::InitWindow` lifecycle
//! event when the native surface is ready.
//!
//! The GPUI `Application::run` callback (the `|cx| { ... }` closure) is
//! **deferred** until the native window exists.  `Platform::run` blocks on the
//! Android event loop, and the finish-launching callback is invoked from
//! inside `run_event_loop` once `InitWindow` has been processed.  This keeps
//! the `Application` (and its internal `Rc<RefCell<AppContext>>`) alive on the
//! call stack for the entire lifetime of the event loop, so that weak
//! references held by GPUI's `on_request_frame` / `on_input` callbacks remain
//! valid.
//!
//! On iOS, `Application::run` similarly blocks until the app exits.
//! `IosPlatform` hooks into the UIKit lifecycle via Objective-C FFI callbacks.
//!
//! ## Building
//!
//! ```text
//! # Android
//! rustup target add aarch64-linux-android
//! cargo ndk -t arm64-v8a build -p gpui-mobile-example
//!
//! # iOS simulator
//! cargo build --target aarch64-apple-ios-sim -p gpui-mobile-example --features font-kit
//!
//! # iOS device
//! cargo build --target aarch64-apple-ios -p gpui-mobile-example --features font-kit
//! ```

// Link gpui-mobile so its symbols (jni helpers, platform, etc.) are available.
extern crate gpui_mobile;

pub mod demos;
pub mod screens;

#[cfg(any(target_os = "ios", target_os = "android"))]
use gpui::{prelude::*, App, WindowOptions};

#[cfg(target_os = "android")]
use gpui::Application;

#[cfg(any(target_os = "ios", target_os = "android"))]
use screens::Router;

// ═══════════════════════════════════════════════════════════════════════════
// Android entry point
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "android")]
use gpui_mobile::android::jni;

/// Called by the `android-activity` crate on a dedicated native thread.
/// Does NOT return until the app is ready to exit.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    // Logger first — so everything after this is visible in logcat.
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("gpui-mobile-example"),
    );

    // Panic hook — routes panics to logcat instead of silently aborting.
    jni::install_panic_hook();

    log::info!("android_main: entered");

    // Initialise the global AndroidApp + AndroidPlatform.
    let _platform = jni::init_platform(&app);
    log::info!("android_main: platform initialised");

    // Get a SharedPlatform (Rc-compatible wrapper around the global
    // Arc<AndroidPlatform>) so we can hand it to GPUI.
    let shared = match jni::shared_platform() {
        Some(s) => s,
        None => {
            log::error!("android_main: shared_platform() returned None — aborting");
            return;
        }
    };

    log::info!("android_main: creating GPUI Application");

    // `Application::with_platform(...).run(...)` calls `Platform::run` which,
    // on Android, **blocks** by driving the native event loop.  The user's
    // `|cx| { ... }` closure is deferred: it runs inside `run_event_loop`
    // once `MainEvent::InitWindow` has delivered a native surface and an
    // `AndroidWindow` exists.
    //
    // Because `Platform::run` blocks, the `Application` stays alive on this
    // stack frame for the entire duration of the event loop.  This means the
    // `Rc<RefCell<AppContext>>` (which GPUI callbacks hold via `Weak`) remains
    // valid — solving the lifetime mismatch that previously caused
    // `default_prevented=false` on touch events and potential crashes.
    Application::with_platform(shared.into_rc()).run(|cx: &mut App| {
        log::info!("Application::run callback — opening window with Router");
        open_main_window(cx);
    });

    // `Application::run` returns here only after the event loop exits
    // (i.e. the activity was destroyed or quit() was called).
    log::info!("android_main: Application.run returned — activity will finish");
}

// ═══════════════════════════════════════════════════════════════════════════
// iOS entry point
// ═══════════════════════════════════════════════════════════════════════════

/// Register the example app's root view with the GPUI iOS platform.
///
/// This is called from `main.m` **before** `gpui_ios_run_demo()` so that
/// when the GPUI run loop starts it knows which view to create.
///
/// The symbol lives in the example crate's static lib which is force-loaded
/// alongside `libgpui_mobile.a` by the Xcode linker.
/// Minimal logger that routes Rust `log` crate messages through NSLog.
#[cfg(target_os = "ios")]
struct NsLogLogger;

#[cfg(target_os = "ios")]
impl log::Log for NsLogLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool { true }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let msg = format!("[{}] {}: {}", record.level(), record.target(), record.args());
            nslog(&msg);
        }
    }
    fn flush(&self) {}
}

/// Call NSLog from Rust via raw FFI.
#[cfg(target_os = "ios")]
fn nslog(msg: &str) {
    use objc::{class, msg_send, runtime::Object, sel, sel_impl};
    unsafe {
        extern "C" { fn NSLog(fmt: *mut Object, ...); }
        let c_msg = std::ffi::CString::new(msg).unwrap_or_default();
        let ns_msg: *mut Object = msg_send![class!(NSString), alloc];
        let ns_msg: *mut Object = msg_send![ns_msg, initWithUTF8String: c_msg.as_ptr()];
        let c_fmt = std::ffi::CString::new("%@").unwrap_or_default();
        let ns_fmt: *mut Object = msg_send![class!(NSString), alloc];
        let ns_fmt: *mut Object = msg_send![ns_fmt, initWithUTF8String: c_fmt.as_ptr()];
        NSLog(ns_fmt, ns_msg);
    }
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn gpui_ios_register_app() {
    // Set up Rust logging → NSLog so log::info! etc. appear in devicectl --console.
    let _ = log::set_logger(&NsLogLogger).map(|()| log::set_max_level(log::LevelFilter::Info));

    // Panic hook → NSLog so panics are visible.
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("GPUI PANIC: {info}");
        nslog(&msg);
    }));

    gpui_mobile::ios::ffi::set_app_callback(Box::new(|cx: &mut App| {
        open_main_window(cx);
    }));
}

/// Convenience entry point for the binary target (`main.rs`).
#[cfg(target_os = "ios")]
pub fn ios_main() {
    gpui_ios_register_app();
    gpui_mobile::ios::ffi::run_app();
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared window creation
// ═══════════════════════════════════════════════════════════════════════════

/// Open the main application window with the shared `Router` view.
///
/// This is called from both the Android and iOS entry points.  On both
/// platforms, windows are fullscreen so `window_bounds` is `None`.
#[cfg(any(target_os = "ios", target_os = "android"))]
fn open_main_window(cx: &mut App) {
    match cx.open_window(
        WindowOptions {
            window_bounds: None,
            ..Default::default()
        },
        |_, cx| cx.new(|_| Router::new()),
    ) {
        Ok(_handle) => {
            #[cfg(target_os = "android")]
            log::info!("cx.open_window succeeded — Router is live");
        }
        Err(_e) => {
            #[cfg(target_os = "android")]
            log::error!("cx.open_window failed: {_e:#}");

            #[cfg(target_os = "ios")]
            eprintln!("cx.open_window failed: {_e:#}");
        }
    }

    cx.activate(true);
}
