//! Android multi-screen example.
//!
//! This example demonstrates a GPUI app with multiple screens and navigation
//! on Android.  The app uses a `Router` view as its root, which owns
//! navigation state and delegates rendering to the currently active screen.
//!
//! ## Screens
//!
//! - **Home** — welcome message, colour swatches, stats, and quick-nav cards.
//! - **Counter** — increment / decrement / reset a shared tap counter.
//! - **Settings** — toggle dark mode, reset counter, change user name.
//! - **About** — app info, technology stack, architecture, and credits.
//!
//! ## Entry point
//!
//! This crate defines `android_main` directly — the `android-activity` crate
//! calls it on a dedicated native thread after loading the `.so`.
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
//! To build for Android:
//! ```
//! rustup target add aarch64-linux-android
//! cargo ndk -t arm64-v8a build -p gpui-android-example
//! ```

// Link gpui-mobile so its symbols (jni helpers, platform, etc.) are available.
extern crate gpui_mobile;

mod screens;

use gpui::{prelude::*, App, Application, WindowOptions};
use gpui_mobile::android::jni_entry;
use screens::Router;

// ── Android entry point ──────────────────────────────────────────────────────
//
// Called by the `android-activity` crate on a dedicated native thread.
// Does NOT return until the app is ready to exit.

#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    // Logger first — so everything after this is visible in logcat.
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("gpui-android-example"),
    );

    // Panic hook — routes panics to logcat instead of silently aborting.
    jni_entry::install_panic_hook();

    log::info!("android_main: entered");

    // Initialise the global AndroidApp + AndroidPlatform.
    let _platform = jni_entry::init_platform(&app);
    log::info!("android_main: platform initialised");

    // Get a SharedPlatform (Rc-compatible wrapper around the global
    // Arc<AndroidPlatform>) so we can hand it to GPUI.
    let shared = match jni_entry::shared_platform() {
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

        match cx.open_window(
            WindowOptions {
                window_bounds: None,
                ..Default::default()
            },
            |_, cx| cx.new(|_| Router::new()),
        ) {
            Ok(_handle) => {
                log::info!("cx.open_window succeeded — Router is live");
            }
            Err(e) => {
                log::error!("cx.open_window failed: {e:#}");
            }
        }

        cx.activate(true);
    });

    // `Application::run` returns here only after the event loop exits
    // (i.e. the activity was destroyed or quit() was called).
    log::info!("android_main: Application.run returned — activity will finish");
}
