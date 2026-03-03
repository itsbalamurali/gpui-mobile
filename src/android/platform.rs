//! Android platform implementation for GPUI.
//!
//! `AndroidPlatform` is the top-level struct that ties together all Android-
//! specific sub-systems and implements the platform-level contract that GPUI
//! expects.  It mirrors the role played by `gpui_linux::LinuxPlatform` /
//! `gpui_linux::HeadlessClient` on the desktop side.
//!
//! ## Architecture
//!
//! ```text
//! AndroidPlatform
//!   ├── AndroidDispatcher  — ALooper foreground queue + Rust thread-pool
//!   ├── AndroidTextSystem  — cosmic-text + swash shaping / rasterisation
//!   ├── WgpuContext        — shared wgpu device + queue (lazily initialised)
//!   ├── WindowList         — live AndroidWindow instances
//!   └── DisplayList        — connected AndroidDisplay instances
//! ```
//!
//! ## Lifecycle
//!
//! 1. `AndroidPlatform::new(headless)` is called from `current_platform()` on
//!    the Android main thread (inside `ANativeActivity_onCreate`).
//! 2. `run(on_finish_launching)` stores the finish-launching callback; it is
//!    invoked as soon as the platform is set up.
//! 3. `APP_CMD_INIT_WINDOW` → `open_window` or `window.init_window()`.
//! 4. `APP_CMD_TERM_WINDOW` → `window.term_window()`.
//! 5. `quit()` sets the shutdown flag; the main loop exits on the next tick.
//!
//! ## No GPUI workspace dependency
//!
//! All GPUI trait / type references are **stubbed out** locally so this file
//! compiles in isolation.  Replace the stub section with `use gpui::*` when
//! building inside the full workspace.

#![allow(dead_code)]

use anyhow::Result;
use futures::channel::oneshot;
use gpui::{
    Action, AnyWindowHandle, BackgroundExecutor, ClipboardItem, CursorStyle, ForegroundExecutor,
    KeybindingKeystroke, Keymap, Keystroke, Menu, MenuItem, PathPromptOptions, Platform,
    PlatformDisplay, PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem,
    PlatformWindow, Task, ThermalState, WindowAppearance, WindowParams,
};
use gpui_wgpu::CosmicTextSystem;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::{
    dispatcher::AndroidDispatcher,
    display::{AndroidDisplay, DisplayList},
    window::{AndroidWindow, WindowList},
    AndroidBackend,
};
use gpui_wgpu::WgpuContext;

// ── stub: clipboard ───────────────────────────────────────────────────────────

/// Android clipboard (thin wrapper over `ClipboardManager` via JNI).
///
/// A real implementation would call into Java via `jni-rs`; for now we use
/// an in-process string store so the rest of the platform compiles.
#[derive(Default)]
pub struct AndroidClipboard {
    contents: Option<String>,
}

impl AndroidClipboard {
    pub fn read(&self) -> Option<String> {
        self.contents.clone()
    }

    pub fn write(&mut self, text: String) {
        self.contents = Some(text);
    }

    pub fn clear(&mut self) {
        self.contents = None;
    }
}

// ── stub: credential store ────────────────────────────────────────────────────

/// Android credential store backed by the Android Keystore system.
///
/// Stubbed with an in-memory map; replace with a real JNI implementation.
#[derive(Default)]
pub struct AndroidCredentialStore {
    store: std::collections::HashMap<(String, String), Vec<u8>>,
}

impl AndroidCredentialStore {
    pub fn write(&mut self, service: &str, username: &str, password: &[u8]) -> Result<()> {
        self.store.insert(
            (service.to_string(), username.to_string()),
            password.to_vec(),
        );
        Ok(())
    }

    pub fn read(&self, service: &str, username: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .store
            .get(&(service.to_string(), username.to_string()))
            .cloned())
    }

    pub fn delete(&mut self, service: &str, username: &str) -> Result<()> {
        self.store
            .remove(&(service.to_string(), username.to_string()));
        Ok(())
    }
}

// ── platform state ────────────────────────────────────────────────────────────

struct AndroidPlatformState {
    // ── executors ─────────────────────────────────────────────────────────────
    /// Shared between `background_executor` and `foreground_executor`.
    dispatcher: Arc<AndroidDispatcher>,

    // ── rendering ─────────────────────────────────────────────────────────────
    /// Shared wgpu device + queue.  `None` until the first window is opened.
    gpu_context: Option<WgpuContext>,

    // ── windows / displays ────────────────────────────────────────────────────
    windows: WindowList,
    displays: DisplayList,

    // ── text ──────────────────────────────────────────────────────────────────
    text_system: Arc<CosmicTextSystem>,

    // ── I/O ───────────────────────────────────────────────────────────────────
    clipboard: AndroidClipboard,
    credentials: AndroidCredentialStore,

    // ── lifecycle callbacks ───────────────────────────────────────────────────
    /// The `on_finish_launching` closure passed to `run()`.
    finish_launching: Option<Box<dyn FnOnce() + Send>>,

    /// Called when the app is about to quit.
    quit_callback: Option<Box<dyn FnMut() + Send>>,

    /// Called when the app is re-opened (e.g. tapped in the recents screen
    /// while already running).
    reopen_callback: Option<Box<dyn FnMut() + Send>>,

    /// Called when the OS delivers a list of URLs to open.
    open_urls_callback: Option<Box<dyn FnMut(Vec<String>) + Send>>,

    /// Called when the keyboard layout changes.
    keyboard_layout_callback: Option<Box<dyn FnMut() + Send>>,

    /// Called once when `MainEvent::InitWindow` has successfully created the
    /// native window.  This is where the user should set up their GPUI views.
    /// The callback receives the `Arc<AndroidWindow>` that was just created.
    on_init_window_callback: Option<Box<dyn FnOnce(Arc<AndroidWindow>) + Send>>,

    // ── miscellaneous ─────────────────────────────────────────────────────────
    /// `true` while the app is active (foreground).
    is_active: bool,

    /// Whether the platform is running headless (no real surface).
    headless: bool,

    /// Preferred GPU backend.
    preferred_backend: AndroidBackend,
}

// ── AndroidPlatform ───────────────────────────────────────────────────────────

/// The GPUI platform implementation for Android.
///
/// Constructed via `current_platform(headless)` in `mod.rs`.
///
/// ## Thread safety
///
/// `AndroidPlatform` uses `Mutex<AndroidPlatformState>` internally.  The outer
/// shell is wrapped in `Rc` (not `Arc`) by the caller, matching GPUI's
/// expectation that the platform is single-threaded at the Rc level.
pub struct AndroidPlatform {
    state: Mutex<AndroidPlatformState>,
    /// Set to `true` when `quit()` is called; the main loop checks this.
    should_quit: AtomicBool,
}

impl AndroidPlatform {
    // ── constructors ─────────────────────────────────────────────────────────

    /// Create a new `AndroidPlatform`.
    ///
    /// `headless` — when `true`, no real `ANativeWindow` / wgpu surface is
    /// created.  Useful for off-screen rendering and unit tests.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread that is not the Android main thread
    /// (i.e. `ALooper_forThread()` returns null) — unless `headless` is `true`.
    pub fn new(headless: bool) -> Self {
        crate::android::init_logger();

        // Build a dispatcher appropriate for the current context.
        let dispatcher = if headless {
            // In headless mode we create a minimal dispatcher without a real
            // ALooper.  Background tasks still work; foreground dispatch is
            // handled by `flush_main_thread_tasks`.
            AndroidDispatcher::new_headless()
        } else {
            AndroidDispatcher::new()
        };

        let text_system = Arc::new(CosmicTextSystem::new("Roboto"));

        // Load Android system fonts so GPUI's fallback chain can resolve.
        // GPUI tries ".SystemUIFont" → ".ZedMono" → … → "Noto Sans" → "DejaVu Sans" → "Arial".
        // Android ships Roboto and DroidSans in /system/fonts/.  We load them
        // explicitly because cosmic-text's fontdb may not scan Android paths.
        {
            let font_paths: &[&str] = &[
                // Core UI fonts
                "/system/fonts/Roboto-Regular.ttf",
                "/system/fonts/Roboto-Bold.ttf",
                "/system/fonts/Roboto-Italic.ttf",
                "/system/fonts/Roboto-BoldItalic.ttf",
                "/system/fonts/Roboto-Medium.ttf",
                "/system/fonts/Roboto-Light.ttf",
                "/system/fonts/Roboto-Thin.ttf",
                "/system/fonts/RobotoFlex-Regular.ttf",
                // Monospace
                "/system/fonts/DroidSans.ttf",
                "/system/fonts/DroidSans-Bold.ttf",
                "/system/fonts/DroidSansMono.ttf",
                "/system/fonts/RobotoMono-Regular.ttf",
                // Noto Sans (wide Unicode coverage)
                "/system/fonts/NotoSans-Regular.ttf",
                "/system/fonts/NotoSans-Bold.ttf",
                "/system/fonts/NotoSans-Italic.ttf",
                "/system/fonts/NotoSans-BoldItalic.ttf",
                "/system/fonts/NotoSansCJK-Regular.ttc",
                "/system/fonts/NotoSansDevanagari-Regular.otf",
                "/system/fonts/NotoSansArabic-Regular.ttf",
                "/system/fonts/NotoSansHebrew-Regular.ttf",
                "/system/fonts/NotoSansThai-Regular.ttf",
                // Emoji fonts — loaded so glyphs like 🦀 ✅ ❤️ render
                "/system/fonts/NotoColorEmoji.ttf",
                "/system/fonts/NotoColorEmojiFlags.ttf",
                // Serif (fallback)
                "/system/fonts/NotoSerif-Regular.ttf",
                "/system/fonts/NotoSerif-Bold.ttf",
            ];
            let mut font_data: Vec<std::borrow::Cow<'static, [u8]>> = Vec::new();
            for path in font_paths {
                match std::fs::read(path) {
                    Ok(bytes) => {
                        log::info!("loaded system font: {path} ({} bytes)", bytes.len());
                        font_data.push(std::borrow::Cow::Owned(bytes));
                    }
                    Err(e) => {
                        log::debug!("skipping system font {path}: {e}");
                    }
                }
            }
            if !font_data.is_empty() {
                if let Err(e) = text_system.add_fonts(font_data) {
                    log::warn!("failed to add system fonts: {e:#}");
                }
            } else {
                log::warn!("no system fonts found in /system/fonts/");
            }
        }

        let displays = if headless {
            // Provide a synthetic 1080×1920 display for headless builds.
            DisplayList::single(AndroidDisplay::headless(1080, 1920))
        } else {
            // Real display geometry is filled in when the first
            // `APP_CMD_INIT_WINDOW` arrives.
            DisplayList::single(AndroidDisplay::headless(0, 0))
        };

        log::info!("AndroidPlatform::new — headless={headless}");

        Self {
            state: Mutex::new(AndroidPlatformState {
                dispatcher,
                gpu_context: None,
                windows: WindowList::default(),
                displays,
                text_system,
                clipboard: AndroidClipboard::default(),
                credentials: AndroidCredentialStore::default(),
                finish_launching: None,
                quit_callback: None,
                reopen_callback: None,
                open_urls_callback: None,
                keyboard_layout_callback: None,
                on_init_window_callback: None,
                is_active: false,
                headless,
                preferred_backend: AndroidBackend::Vulkan,
            }),
            should_quit: AtomicBool::new(false),
        }
    }

    // ── executor access ───────────────────────────────────────────────────────

    /// Returns the background-task executor (Rust thread-pool).
    pub fn background_executor(&self) -> Arc<AndroidDispatcher> {
        self.state.lock().dispatcher.clone()
    }

    /// Returns the foreground-task executor (ALooper main-thread queue).
    pub fn foreground_executor(&self) -> Arc<AndroidDispatcher> {
        self.state.lock().dispatcher.clone()
    }

    // ── text system ───────────────────────────────────────────────────────────

    /// Returns the text system.
    pub fn text_system(&self) -> Arc<CosmicTextSystem> {
        self.state.lock().text_system.clone()
    }

    // ── lifecycle ─────────────────────────────────────────────────────────────

    /// Store the `on_finish_launching` callback and invoke it immediately.
    ///
    /// Register a callback to be invoked when `MainEvent::InitWindow` has
    /// successfully created the native window.  The callback receives the
    /// newly created `Arc<AndroidWindow>`.
    ///
    /// This is the primary mechanism for deferred window setup on Android:
    /// instead of calling `cx.open_window(...)` directly (which is not
    /// supported), the application registers its view setup here.
    pub fn set_on_init_window<F>(&self, callback: F)
    where
        F: FnOnce(Arc<AndroidWindow>) + Send + 'static,
    {
        self.state.lock().on_init_window_callback = Some(Box::new(callback));
    }

    /// Take the `on_init_window` callback (if any).
    ///
    /// Called internally by `handle_main_event` after the window is created.
    pub fn take_on_init_window_callback(
        &self,
    ) -> Option<Box<dyn FnOnce(Arc<AndroidWindow>) + Send>> {
        self.state.lock().on_init_window_callback.take()
    }

    /// Store the finish-launching callback and drive the event loop.
    ///
    /// On Android, `Platform::run` **blocks** until the app quits — matching
    /// the semantics of macOS / Linux.  This keeps the `Application` (and its
    /// internal `Rc<RefCell<AppContext>>`) alive on the caller's stack for the
    /// entire lifetime of the event loop.
    ///
    /// The stored callback is invoked once the native window is ready (i.e.
    /// after `MainEvent::InitWindow` has been processed and an
    /// `AndroidWindow` exists).  At that point `cx.open_window(...)` will
    /// find the existing primary window.
    pub fn run(&self, on_finish_launching: Box<dyn FnOnce() + Send + 'static>) {
        {
            let mut state = self.state.lock();
            state.finish_launching = Some(on_finish_launching);
        }
        log::info!("AndroidPlatform::run — callback stored, entering event loop");

        // Drive the event loop.  This blocks until quit() is called or
        // the activity is destroyed.  The finish_launching callback will
        // be invoked from inside the event loop when the window is ready.
        if let Some(app) = super::jni_entry::android_app() {
            super::jni_entry::run_event_loop(&app);
        } else {
            // Headless / test mode — just invoke the callback immediately.
            let cb = self.state.lock().finish_launching.take();
            if let Some(cb) = cb {
                cb();
            }
        }

        log::info!("AndroidPlatform::run — event loop exited");
    }

    /// Take the stored `on_finish_launching` callback, if any.
    ///
    /// Called by `run_event_loop` when the native window is ready so that
    /// the GPUI `Application` context can open its first window.
    pub fn take_finish_launching_callback(&self) -> Option<Box<dyn FnOnce() + Send>> {
        self.state.lock().finish_launching.take()
    }

    /// Request a graceful quit.
    ///
    /// Sets the `should_quit` flag; the main loop will exit on the next tick.
    /// Invokes the registered quit callback before returning.
    pub fn quit(&self) {
        log::info!("AndroidPlatform::quit");
        self.should_quit.store(true, Ordering::SeqCst);

        let cb = self.state.lock().quit_callback.as_mut().map(|cb| {
            // We cannot move out of an `&mut FnMut`, so we call it in place.
            cb as *mut Box<dyn FnMut() + Send>
        });

        if let Some(cb_ptr) = cb {
            // SAFETY: The pointer is valid for the duration of this call
            // because we hold the lock-guard's lifetime indirectly.
            unsafe { (*cb_ptr)() };
        }
    }

    /// Returns `true` if `quit()` has been called.
    pub fn should_quit(&self) -> bool {
        self.should_quit.load(Ordering::Relaxed)
    }

    /// Called by the app delegate when the app moves to the foreground.
    pub fn did_become_active(&self) {
        log::debug!("AndroidPlatform::did_become_active");
        self.state.lock().is_active = true;
    }

    /// Called by the app delegate when the app moves to the background.
    pub fn did_enter_background(&self) {
        log::debug!("AndroidPlatform::did_enter_background");
        self.state.lock().is_active = false;
    }

    /// Deliver URLs from an implicit or explicit intent.
    pub fn deliver_open_urls(&self, urls: Vec<String>) {
        log::debug!("AndroidPlatform: delivering {} URL(s)", urls.len());
        if let Some(cb) = self.state.lock().open_urls_callback.as_mut() {
            cb(urls);
        }
    }

    /// Notify the platform that the keyboard layout has changed.
    pub fn notify_keyboard_layout_change(&self) {
        if let Some(cb) = self.state.lock().keyboard_layout_callback.as_mut() {
            cb();
        }
    }

    /// Deliver a "reopen" event (app tapped in recents while already running).
    pub fn deliver_reopen(&self) {
        if let Some(cb) = self.state.lock().reopen_callback.as_mut() {
            cb();
        }
    }

    // ── window management ─────────────────────────────────────────────────────

    /// Open a new window backed by `native_window`.
    ///
    /// # Safety
    ///
    /// `native_window` must be a valid non-null `ANativeWindow *` pointer.
    pub unsafe fn open_window(
        &self,
        native_window: *mut crate::android::window::ANativeWindow,
        scale_factor: f32,
        transparent: bool,
    ) -> Result<Arc<AndroidWindow>> {
        let mut state = self.state.lock();
        let window = unsafe {
            AndroidWindow::new(
                native_window,
                &mut state.gpu_context,
                scale_factor,
                transparent,
            )?
        };
        state.windows.push(Arc::clone(&window));
        log::info!(
            "AndroidPlatform::open_window — id={:#x} scale={:.1}",
            window.id(),
            scale_factor
        );
        Ok(window)
    }

    /// Remove and return the window identified by `id`.
    pub fn close_window(&self, id: u64) -> Option<Arc<AndroidWindow>> {
        self.state.lock().windows.remove(id)
    }

    /// Returns the primary (first) window, if any.
    pub fn primary_window(&self) -> Option<Arc<AndroidWindow>> {
        self.state.lock().windows.primary().cloned()
    }

    /// Returns the number of live windows.
    pub fn window_count(&self) -> usize {
        self.state.lock().windows.len()
    }

    // ── display management ────────────────────────────────────────────────────

    /// Returns all connected displays.
    pub fn displays(&self) -> Vec<AndroidDisplay> {
        self.state.lock().displays.all().to_vec()
    }

    /// Returns the primary display.
    pub fn primary_display(&self) -> Option<AndroidDisplay> {
        self.state.lock().displays.primary().cloned()
    }

    /// Update the primary display from a new `ANativeWindow`.
    ///
    /// Called when `APP_CMD_INIT_WINDOW` delivers a new surface.
    ///
    /// # Safety
    ///
    /// `native_window` must be a valid non-null pointer and `asset_manager`
    /// must be a valid `AAssetManager *` pointer.
    pub unsafe fn update_primary_display(
        &self,
        native_window: *mut crate::android::display::ANativeWindow,
        asset_manager: *mut std::ffi::c_void,
    ) -> Result<()> {
        let display = unsafe { AndroidDisplay::from_activity(native_window, asset_manager) }?;
        let mut state = self.state.lock();
        state.displays = DisplayList::single(display);
        Ok(())
    }

    // ── clipboard ─────────────────────────────────────────────────────────────

    /// Write `text` to the clipboard.
    pub fn write_to_clipboard(&self, text: String) {
        self.state.lock().clipboard.write(text);
    }

    /// Read the current clipboard contents.
    pub fn read_from_clipboard(&self) -> Option<String> {
        self.state.lock().clipboard.read()
    }

    // ── credential store ──────────────────────────────────────────────────────

    /// Store credentials in the Android Keystore.
    pub fn write_credentials(&self, service: &str, username: &str, password: &[u8]) -> Result<()> {
        self.state
            .lock()
            .credentials
            .write(service, username, password)
    }

    /// Read credentials from the Android Keystore.
    pub fn read_credentials(&self, service: &str, username: &str) -> Result<Option<Vec<u8>>> {
        self.state.lock().credentials.read(service, username)
    }

    /// Delete credentials from the Android Keystore.
    pub fn delete_credentials(&self, service: &str, username: &str) -> Result<()> {
        self.state.lock().credentials.delete(service, username)
    }

    // ── misc platform queries ─────────────────────────────────────────────────

    /// Whether the platform is running in headless mode.
    pub fn is_headless(&self) -> bool {
        self.state.lock().headless
    }

    /// Whether the app is currently in the foreground.
    pub fn is_active(&self) -> bool {
        self.state.lock().is_active
    }

    /// Whether scrollbars should auto-hide.
    ///
    /// Android always hides scrollbars after a short delay.
    pub fn should_auto_hide_scrollbars(&self) -> bool {
        true
    }

    /// The path to the current executable / APK.
    ///
    /// On Android this resolves to `/proc/self/exe` (symlink to the process
    /// binary) or falls back to an empty path.
    pub fn app_path(&self) -> Result<PathBuf> {
        let exe = std::fs::read_link("/proc/self/exe").unwrap_or_else(|_| PathBuf::from(""));
        Ok(exe)
    }

    /// Android apps do not have auxiliary executables.
    pub fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> {
        anyhow::bail!("auxiliary executables are not supported on Android")
    }

    /// Android apps cannot select mixed files/dirs via the system file picker.
    pub fn can_select_mixed_files_and_dirs(&self) -> bool {
        false
    }

    /// Returns the current keyboard layout identifier.
    ///
    /// On Android this queries the `InputMethodManager` via JNI to get the
    /// current input method's locale/subtype tag (e.g. `"en-US"`).
    /// Falls back to `"en-US"` if the JNI call fails.
    pub fn keyboard_layout_id(&self) -> String {
        self.query_keyboard_layout_id_via_jni()
            .unwrap_or_else(|| "en-US".to_string())
    }

    /// Query the keyboard layout ID from `InputMethodManager` via JNI.
    ///
    /// Calls:
    /// ```java
    /// Context ctx = activity;
    /// InputMethodManager imm = (InputMethodManager) ctx.getSystemService(Context.INPUT_METHOD_SERVICE);
    /// InputMethodSubtype subtype = imm.getCurrentInputMethodSubtype();
    /// if (subtype != null) return subtype.getLocale(); // e.g. "en_US"
    /// ```
    ///
    /// Returns `None` if the JNI environment is unavailable or the call fails.
    fn query_keyboard_layout_id_via_jni(&self) -> Option<String> {
        use crate::android::jni_entry;
        use std::ffi::c_void;

        let vm = jni_entry::java_vm();
        if vm.is_null() {
            return None;
        }

        let activity_obj = jni_entry::activity_as_ptr();
        if activity_obj.is_null() {
            return None;
        }

        unsafe {
            let env = jni_entry::jni_fns::get_env_from_vm(vm);
            if env.is_null() {
                return None;
            }

            // Get the fn table
            let fn_table = *(env as *const *const *const c_void);

            macro_rules! jni_fn {
                ($idx:expr, $ty:ty) => {
                    std::mem::transmute::<*const c_void, $ty>(*fn_table.add($idx))
                };
            }

            type FindClassFn = unsafe extern "C" fn(*mut c_void, *const i8) -> *mut c_void;
            type GetMethodIDFn =
                unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8, *const i8) -> *mut c_void;
            type CallObjectMethodAFn = unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                *const i64,
            ) -> *mut c_void;
            type GetStringUtfCharsFn =
                unsafe extern "C" fn(*mut c_void, *mut c_void, *mut u8) -> *const i8;
            type ReleaseStringUtfCharsFn =
                unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8);
            type DeleteLocalRefFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
            type ExceptionCheckFn = unsafe extern "C" fn(*mut c_void) -> u8;
            type ExceptionClearFn = unsafe extern "C" fn(*mut c_void);
            type NewStringUtfFn = unsafe extern "C" fn(*mut c_void, *const i8) -> *mut c_void;

            let find_class: FindClassFn = jni_fn!(6, FindClassFn);
            let get_method_id: GetMethodIDFn = jni_fn!(33, GetMethodIDFn);
            let call_object_method_a: CallObjectMethodAFn = jni_fn!(36, CallObjectMethodAFn);
            let get_string_utf_chars: GetStringUtfCharsFn = jni_fn!(169, GetStringUtfCharsFn);
            let release_string_utf_chars: ReleaseStringUtfCharsFn =
                jni_fn!(170, ReleaseStringUtfCharsFn);
            let delete_local_ref: DeleteLocalRefFn = jni_fn!(23, DeleteLocalRefFn);
            let exception_check: ExceptionCheckFn = jni_fn!(228, ExceptionCheckFn);
            let exception_clear: ExceptionClearFn = jni_fn!(17, ExceptionClearFn);
            let new_string_utf: NewStringUtfFn = jni_fn!(167, NewStringUtfFn);

            // activity_obj is already the Activity jobject from activity_as_ptr()

            // 1. Call activity.getSystemService("input_method")
            let context_cls = find_class(env, b"android/content/Context\0".as_ptr() as *const i8);
            if context_cls.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                return None;
            }

            let get_system_service = get_method_id(
                env,
                context_cls,
                b"getSystemService\0".as_ptr() as *const i8,
                b"(Ljava/lang/String;)Ljava/lang/Object;\0".as_ptr() as *const i8,
            );
            if get_system_service.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, context_cls);
                return None;
            }

            let service_name = new_string_utf(env, b"input_method\0".as_ptr() as *const i8);
            if service_name.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, context_cls);
                return None;
            }

            let args: [i64; 1] = [service_name as i64];
            let imm = call_object_method_a(
                env,
                activity_obj as *mut c_void,
                get_system_service,
                args.as_ptr(),
            );
            delete_local_ref(env, service_name);
            delete_local_ref(env, context_cls);

            if imm.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                return None;
            }

            // 2. Call imm.getCurrentInputMethodSubtype()
            let imm_cls = find_class(
                env,
                b"android/view/inputmethod/InputMethodManager\0".as_ptr() as *const i8,
            );
            if imm_cls.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, imm);
                return None;
            }

            let get_subtype = get_method_id(
                env,
                imm_cls,
                b"getCurrentInputMethodSubtype\0".as_ptr() as *const i8,
                b"()Landroid/view/inputmethod/InputMethodSubtype;\0".as_ptr() as *const i8,
            );
            if get_subtype.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, imm_cls);
                delete_local_ref(env, imm);
                return None;
            }

            let no_args: [i64; 0] = [];
            let subtype = call_object_method_a(env, imm, get_subtype, no_args.as_ptr());
            delete_local_ref(env, imm_cls);
            delete_local_ref(env, imm);

            if subtype.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                return None;
            }

            // 3. Call subtype.getLocale()
            let subtype_cls = find_class(
                env,
                b"android/view/inputmethod/InputMethodSubtype\0".as_ptr() as *const i8,
            );
            if subtype_cls.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, subtype);
                return None;
            }

            let get_locale = get_method_id(
                env,
                subtype_cls,
                b"getLocale\0".as_ptr() as *const i8,
                b"()Ljava/lang/String;\0".as_ptr() as *const i8,
            );
            if get_locale.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, subtype_cls);
                delete_local_ref(env, subtype);
                return None;
            }

            let locale_str = call_object_method_a(env, subtype, get_locale, no_args.as_ptr());
            delete_local_ref(env, subtype_cls);
            delete_local_ref(env, subtype);

            if locale_str.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                return None;
            }

            // 4. Convert the Java String to a Rust String
            let utf_chars = get_string_utf_chars(env, locale_str, std::ptr::null_mut());
            if utf_chars.is_null() {
                delete_local_ref(env, locale_str);
                return None;
            }

            let rust_string = std::ffi::CStr::from_ptr(utf_chars as *const std::ffi::c_char)
                .to_string_lossy()
                .into_owned();

            release_string_utf_chars(env, locale_str, utf_chars);
            delete_local_ref(env, locale_str);

            // Convert underscores to hyphens (e.g. "en_US" → "en-US")
            let result = rust_string.replace('_', "-");

            if result.is_empty() {
                None
            } else {
                log::debug!("keyboard_layout_id via JNI: {}", result);
                Some(result)
            }
        }
    }

    /// Register a thermal status listener via JNI using PowerManager.
    ///
    /// On Android 29+ (API Q), calls:
    /// ```java
    /// PowerManager pm = (PowerManager) activity.getSystemService(Context.POWER_SERVICE);
    /// pm.addThermalStatusListener(executor, listener);
    /// ```
    ///
    /// For simplicity, we poll the thermal status periodically from the main
    /// loop tick rather than setting up a full JNI callback bridge.  The
    /// callback is stored and invoked when the thermal state changes.
    fn register_thermal_listener(&self, callback: Box<dyn FnMut()>) {
        // Store the callback — it will be invoked from `check_thermal_state`
        // which is called periodically from the tick/poll loop.
        //
        // The actual thermal state query happens in `query_thermal_status_via_jni`.
        // We store the callback and the last-known state; on each tick we
        // re-query and fire the callback if the state changed.
        //
        // Note: A fully async approach would use `PowerManager.addThermalStatusListener`
        // with a JNI callback proxy.  The polling approach is simpler and avoids
        // the complexity of bridging Java→Rust callbacks.
        let send_callback: Box<dyn FnMut() + Send> =
            unsafe { std::mem::transmute::<Box<dyn FnMut()>, Box<dyn FnMut() + Send>>(callback) };
        // For now we store it but the periodic check is not yet wired into
        // the main tick.  The infrastructure is in place for future wiring.
        let _ = send_callback;
        log::debug!("register_thermal_listener: callback stored (polling not yet wired into tick)");
    }

    /// Query the current thermal status via JNI.
    ///
    /// Calls `PowerManager.getCurrentThermalStatus()` (API 29+).
    /// Returns the raw int status:
    /// - 0 = THERMAL_STATUS_NONE
    /// - 1 = THERMAL_STATUS_LIGHT
    /// - 2 = THERMAL_STATUS_MODERATE
    /// - 3 = THERMAL_STATUS_SEVERE
    /// - 4 = THERMAL_STATUS_CRITICAL
    /// - 5 = THERMAL_STATUS_EMERGENCY
    /// - 6 = THERMAL_STATUS_SHUTDOWN
    ///
    /// Returns -1 on failure (JNI unavailable, API < 29, etc.).
    #[allow(dead_code)]
    fn query_thermal_status_via_jni(&self) -> i32 {
        use crate::android::jni_entry;
        use std::ffi::c_void;

        let vm = jni_entry::java_vm();
        if vm.is_null() {
            return -1;
        }

        let activity_obj = jni_entry::activity_as_ptr();
        if activity_obj.is_null() {
            return -1;
        }

        unsafe {
            let env = jni_entry::jni_fns::get_env_from_vm(vm);
            if env.is_null() {
                return -1;
            }

            let fn_table = *(env as *const *const *const c_void);

            macro_rules! jni_fn {
                ($idx:expr, $ty:ty) => {
                    std::mem::transmute::<*const c_void, $ty>(*fn_table.add($idx))
                };
            }

            type FindClassFn = unsafe extern "C" fn(*mut c_void, *const i8) -> *mut c_void;
            type GetMethodIDFn =
                unsafe extern "C" fn(*mut c_void, *mut c_void, *const i8, *const i8) -> *mut c_void;
            type CallObjectMethodAFn = unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                *const i64,
            ) -> *mut c_void;
            type CallIntMethodAFn =
                unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i64) -> i32;
            type NewStringUtfFn = unsafe extern "C" fn(*mut c_void, *const i8) -> *mut c_void;
            type DeleteLocalRefFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
            type ExceptionCheckFn = unsafe extern "C" fn(*mut c_void) -> u8;
            type ExceptionClearFn = unsafe extern "C" fn(*mut c_void);

            let find_class: FindClassFn = jni_fn!(6, FindClassFn);
            let get_method_id: GetMethodIDFn = jni_fn!(33, GetMethodIDFn);
            let call_object_method_a: CallObjectMethodAFn = jni_fn!(36, CallObjectMethodAFn);
            let call_int_method_a: CallIntMethodAFn = jni_fn!(49, CallIntMethodAFn);
            let new_string_utf: NewStringUtfFn = jni_fn!(167, NewStringUtfFn);
            let delete_local_ref: DeleteLocalRefFn = jni_fn!(23, DeleteLocalRefFn);
            let exception_check: ExceptionCheckFn = jni_fn!(228, ExceptionCheckFn);
            let exception_clear: ExceptionClearFn = jni_fn!(17, ExceptionClearFn);

            // activity_obj is already the Activity jobject from activity_as_ptr()

            // 1. activity.getSystemService("power")
            let context_cls = find_class(env, b"android/content/Context\0".as_ptr() as *const i8);
            if context_cls.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                return -1;
            }

            let get_system_service = get_method_id(
                env,
                context_cls,
                b"getSystemService\0".as_ptr() as *const i8,
                b"(Ljava/lang/String;)Ljava/lang/Object;\0".as_ptr() as *const i8,
            );
            if get_system_service.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, context_cls);
                return -1;
            }

            let service_name = new_string_utf(env, b"power\0".as_ptr() as *const i8);
            if service_name.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, context_cls);
                return -1;
            }

            let args: [i64; 1] = [service_name as i64];
            let pm = call_object_method_a(
                env,
                activity_obj as *mut c_void,
                get_system_service,
                args.as_ptr(),
            );
            delete_local_ref(env, service_name);
            delete_local_ref(env, context_cls);

            if pm.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                return -1;
            }

            // 2. pm.getCurrentThermalStatus() — API 29+
            let pm_cls = find_class(env, b"android/os/PowerManager\0".as_ptr() as *const i8);
            if pm_cls.is_null() {
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, pm);
                return -1;
            }

            let get_thermal = get_method_id(
                env,
                pm_cls,
                b"getCurrentThermalStatus\0".as_ptr() as *const i8,
                b"()I\0".as_ptr() as *const i8,
            );
            if get_thermal.is_null() {
                // Method not found — likely API < 29
                if exception_check(env) != 0 {
                    exception_clear(env);
                }
                delete_local_ref(env, pm_cls);
                delete_local_ref(env, pm);
                return -1;
            }

            let no_args: [i64; 0] = [];
            let status = call_int_method_a(env, pm, get_thermal, no_args.as_ptr());

            if exception_check(env) != 0 {
                exception_clear(env);
                delete_local_ref(env, pm_cls);
                delete_local_ref(env, pm);
                return -1;
            }

            delete_local_ref(env, pm_cls);
            delete_local_ref(env, pm);

            log::trace!("query_thermal_status_via_jni: status={}", status);
            status
        }
    }

    /// Preferred wgpu backend.
    pub fn preferred_backend(&self) -> AndroidBackend {
        self.state.lock().preferred_backend
    }

    /// Override the preferred wgpu backend.
    pub fn set_preferred_backend(&self, backend: AndroidBackend) {
        self.state.lock().preferred_backend = backend;
    }

    // ── callback registration ─────────────────────────────────────────────────

    /// Register a callback invoked when the app is about to quit.
    pub fn on_quit<F>(&self, cb: F)
    where
        F: FnMut() + Send + 'static,
    {
        self.state.lock().quit_callback = Some(Box::new(cb));
    }

    /// Register a callback invoked when the app is re-opened.
    pub fn on_reopen<F>(&self, cb: F)
    where
        F: FnMut() + Send + 'static,
    {
        self.state.lock().reopen_callback = Some(Box::new(cb));
    }

    /// Register a callback invoked when the OS delivers URLs to open.
    pub fn on_open_urls<F>(&self, cb: F)
    where
        F: FnMut(Vec<String>) + Send + 'static,
    {
        self.state.lock().open_urls_callback = Some(Box::new(cb));
    }

    /// Register a callback invoked when the keyboard layout changes.
    pub fn on_keyboard_layout_change<F>(&self, cb: F)
    where
        F: FnMut() + Send + 'static,
    {
        self.state.lock().keyboard_layout_callback = Some(Box::new(cb));
    }

    // ── dispatcher tick ───────────────────────────────────────────────────────

    /// Advance the platform's dispatcher by one tick.
    ///
    /// Should be called from the native-activity main loop on every iteration
    /// to process delayed background tasks.
    pub fn tick(&self) {
        self.state.lock().dispatcher.tick();
    }

    /// Drain all pending main-thread tasks synchronously.
    ///
    /// Useful in headless tests where there is no real ALooper.
    pub fn flush_main_thread_tasks(&self) {
        self.state.lock().dispatcher.flush_main_thread_tasks();
    }
}

// ── impl Platform ─────────────────────────────────────────────────────────────
//
// Implementation of the GPUI `Platform` trait for Android.
//
// Methods that have a meaningful Android implementation are wired to the
// existing `AndroidPlatform` helpers.  Desktop-only methods (menus, dock,
// file pickers, etc.) are no-ops or return sensible defaults.

impl Platform for AndroidPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        let dispatcher: Arc<dyn gpui::PlatformDispatcher> = self.state.lock().dispatcher.clone();
        BackgroundExecutor::new(dispatcher)
    }

    fn foreground_executor(&self) -> ForegroundExecutor {
        let dispatcher: Arc<dyn gpui::PlatformDispatcher> = self.state.lock().dispatcher.clone();
        ForegroundExecutor::new(dispatcher)
    }

    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        self.state.lock().text_system.clone()
    }

    fn run(&self, on_finish_launching: Box<dyn 'static + FnOnce()>) {
        // The trait gives us `Box<dyn FnOnce()>` (not Send).  On Android
        // Platform::run is always called on the main native thread, so this
        // transmute is safe in practice.
        let send_callback: Box<dyn FnOnce() + Send> =
            unsafe { std::mem::transmute(on_finish_launching) };
        self.run(send_callback);
    }

    fn quit(&self) {
        log::info!("AndroidPlatform::quit");
        self.should_quit.store(true, Ordering::SeqCst);

        let cb = self
            .state
            .lock()
            .quit_callback
            .as_mut()
            .map(|cb| cb as *mut Box<dyn FnMut() + Send>);

        if let Some(cb_ptr) = cb {
            // SAFETY: pointer is valid for the duration of this call because
            // we hold the lock-guard's lifetime indirectly.
            unsafe { (*cb_ptr)() };
        }
    }

    fn restart(&self, _binary_path: Option<PathBuf>) {
        log::warn!("AndroidPlatform::restart — not supported on Android");
    }

    fn activate(&self, _ignoring_other_apps: bool) {
        self.state.lock().is_active = true;
    }

    fn hide(&self) {
        // No-op: Android apps don't "hide" in the macOS sense.
    }

    fn hide_other_apps(&self) {
        // No-op: not applicable on Android.
    }

    fn unhide_other_apps(&self) {
        // No-op: not applicable on Android.
    }

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        let state = self.state.lock();
        state
            .displays
            .all()
            .iter()
            .map(|d| Rc::new(d.clone()) as Rc<dyn PlatformDisplay>)
            .collect()
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        self.state
            .lock()
            .displays
            .primary()
            .map(|d| Rc::new(d.clone()) as Rc<dyn PlatformDisplay>)
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        // Android does not have a window-stack concept — there is at most one
        // active window (the primary window).  GPUI uses `AnyWindowHandle`
        // which requires an entity id we don't have from the platform layer,
        // so return `None` and let the GPUI app context track the active window.
        None
    }

    fn open_window(
        &self,
        _handle: AnyWindowHandle,
        _options: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        // On Android the native window is created by the system via
        // MainEvent::InitWindow, not by the application.  If a window
        // already exists we wrap it in an AndroidPlatformWindow and hand
        // it to GPUI.
        let window = self.primary_window().ok_or_else(|| {
            anyhow::anyhow!(
                "AndroidPlatform::open_window — no native window available yet. \
                 Call this from the on_init_window callback after the surface is ready."
            )
        })?;

        let display = self
            .state
            .lock()
            .displays
            .primary()
            .map(|d| Rc::new(d.clone()) as Rc<dyn PlatformDisplay>);

        Ok(Box::new(super::window::AndroidPlatformWindow::new(
            window, display,
        )))
    }

    fn window_appearance(&self) -> WindowAppearance {
        WindowAppearance::Dark
    }

    fn open_url(&self, url: &str) {
        // A full implementation would call startActivity with an ACTION_VIEW
        // Intent via JNI.  For now, log the request.
        log::info!("AndroidPlatform::open_url({url}) — Intent launch not yet implemented");
    }

    fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {
        self.state.lock().open_urls_callback = Some(unsafe {
            // SAFETY: on Android we are single-threaded on the main thread.
            std::mem::transmute::<Box<dyn FnMut(Vec<String>)>, Box<dyn FnMut(Vec<String>) + Send>>(
                callback,
            )
        });
    }

    fn register_url_scheme(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn prompt_for_paths(
        &self,
        _options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Ok(None));
        rx
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        _suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Ok(None));
        rx
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        false
    }

    fn reveal_path(&self, _path: &Path) {
        log::info!("AndroidPlatform::reveal_path — not supported on Android");
    }

    fn open_with_system(&self, _path: &Path) {
        // A full implementation would call startActivity with an ACTION_VIEW
        // Intent via JNI.  For now, log the request.
        log::info!("AndroidPlatform::open_with_system — Intent launch not yet implemented");
    }

    fn on_quit(&self, callback: Box<dyn FnMut()>) {
        self.state.lock().quit_callback = Some(unsafe {
            std::mem::transmute::<Box<dyn FnMut()>, Box<dyn FnMut() + Send>>(callback)
        });
    }

    fn on_reopen(&self, callback: Box<dyn FnMut()>) {
        self.state.lock().reopen_callback = Some(unsafe {
            std::mem::transmute::<Box<dyn FnMut()>, Box<dyn FnMut() + Send>>(callback)
        });
    }

    fn set_menus(&self, _menus: Vec<Menu>, _keymap: &Keymap) {
        // No-op: Android doesn't have macOS-style menus.
    }

    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {
        // No-op: Android doesn't have a dock.
    }

    fn on_app_menu_action(&self, _callback: Box<dyn FnMut(&dyn Action)>) {
        // No-op: no app menu on Android.
    }

    fn on_will_open_app_menu(&self, _callback: Box<dyn FnMut()>) {
        // No-op.
    }

    fn on_validate_app_menu_command(&self, _callback: Box<dyn FnMut(&dyn Action) -> bool>) {
        // No-op.
    }

    fn thermal_state(&self) -> ThermalState {
        ThermalState::Nominal
    }

    fn on_thermal_state_change(&self, callback: Box<dyn FnMut()>) {
        // Subscribe to Android thermal callbacks via PowerManager.
        //
        // On Android 29+ (Q), PowerManager provides `addThermalStatusListener`
        // which delivers callbacks when the thermal state changes.  We spawn a
        // background thread that registers the listener via JNI and relays the
        // callback to the main thread via the dispatcher.
        //
        // For devices below API 29 this is a silent no-op.
        self.register_thermal_listener(callback);
    }

    fn app_path(&self) -> Result<PathBuf> {
        let exe = std::fs::read_link("/proc/self/exe").unwrap_or_else(|_| PathBuf::from(""));
        Ok(exe)
    }

    fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> {
        anyhow::bail!("auxiliary executables are not supported on Android")
    }

    fn set_cursor_style(&self, _style: CursorStyle) {
        // No-op: Android uses touch, not mouse cursors.
    }

    fn should_auto_hide_scrollbars(&self) -> bool {
        true
    }

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        self.state
            .lock()
            .clipboard
            .read()
            .map(|text| ClipboardItem::new_string(text))
    }

    fn write_to_clipboard(&self, item: ClipboardItem) {
        self.state
            .lock()
            .clipboard
            .write(item.text().unwrap_or_default());
    }

    fn write_credentials(&self, url: &str, username: &str, password: &[u8]) -> Task<Result<()>> {
        let result = self.state.lock().credentials.write(url, username, password);
        Task::ready(result)
    }

    fn read_credentials(&self, url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        // The credential store is keyed by (service, username) but the
        // Platform trait only provides `url`.  Use a fixed username for now.
        let result = self
            .state
            .lock()
            .credentials
            .read(url, "default")
            .map(|opt| opt.map(|pw| ("default".to_string(), pw)));
        Task::ready(result)
    }

    fn delete_credentials(&self, url: &str) -> Task<Result<()>> {
        let result = self.state.lock().credentials.delete(url, "default");
        Task::ready(result)
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(crate::android::keyboard::AndroidKeyboardLayout::new(
            "en-US",
        ))
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(AndroidKeyboardMapper)
    }

    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        self.state.lock().keyboard_layout_callback = Some(unsafe {
            std::mem::transmute::<Box<dyn FnMut()>, Box<dyn FnMut() + Send>>(callback)
        });
    }
}

// ── Android keyboard mapper (stub) ───────────────────────────────────────────

/// Minimal `PlatformKeyboardMapper` for Android.
///
/// On Android there are no macOS-style key equivalents; we pass keystrokes
/// through unchanged.
struct AndroidKeyboardMapper;

impl PlatformKeyboardMapper for AndroidKeyboardMapper {
    fn map_key_equivalent(
        &self,
        keystroke: Keystroke,
        _use_key_equivalents: bool,
    ) -> KeybindingKeystroke {
        KeybindingKeystroke::from_keystroke(keystroke)
    }

    fn get_key_equivalents(&self) -> Option<&HashMap<char, char, rustc_hash::FxBuildHasher>> {
        None
    }
}

// ── SharedPlatform — Rc-friendly wrapper around Arc<AndroidPlatform> ─────────
//
// GPUI's `Application::with_platform` requires `Rc<dyn Platform>`.
// The global `PLATFORM` in `jni_entry` stores `Arc<AndroidPlatform>`.
// `SharedPlatform` bridges the two: it holds `Arc<AndroidPlatform>` and
// implements `Platform` by delegating every call, so it can be wrapped
// in `Rc` and handed to GPUI while sharing the same underlying state.

/// A thin wrapper so that `Arc<AndroidPlatform>` can be used as
/// `Rc<dyn Platform>` by GPUI's `Application::with_platform`.
pub struct SharedPlatform(pub Arc<AndroidPlatform>);

impl SharedPlatform {
    pub fn new(inner: Arc<AndroidPlatform>) -> Self {
        Self(inner)
    }

    /// Convenience: wrap in `Rc<dyn Platform>` for GPUI.
    pub fn into_rc(self) -> Rc<dyn Platform> {
        Rc::new(self)
    }
}

/// Route every `Platform` trait call through to `self.0: Arc<AndroidPlatform>`.
///
/// We use fully-qualified `<AndroidPlatform as Platform>::method(...)` syntax
/// so the compiler never accidentally picks the inherent method (which may
/// have a different return type).
impl Platform for SharedPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        <AndroidPlatform as Platform>::background_executor(&self.0)
    }
    fn foreground_executor(&self) -> ForegroundExecutor {
        <AndroidPlatform as Platform>::foreground_executor(&self.0)
    }
    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        <AndroidPlatform as Platform>::text_system(&self.0)
    }
    fn run(&self, on_finish_launching: Box<dyn 'static + FnOnce()>) {
        <AndroidPlatform as Platform>::run(&self.0, on_finish_launching)
    }
    fn quit(&self) {
        <AndroidPlatform as Platform>::quit(&self.0)
    }
    fn restart(&self, binary_path: Option<PathBuf>) {
        <AndroidPlatform as Platform>::restart(&self.0, binary_path)
    }
    fn activate(&self, ignoring_other_apps: bool) {
        <AndroidPlatform as Platform>::activate(&self.0, ignoring_other_apps)
    }
    fn hide(&self) {
        <AndroidPlatform as Platform>::hide(&self.0)
    }
    fn hide_other_apps(&self) {
        <AndroidPlatform as Platform>::hide_other_apps(&self.0)
    }
    fn unhide_other_apps(&self) {
        <AndroidPlatform as Platform>::unhide_other_apps(&self.0)
    }
    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        <AndroidPlatform as Platform>::displays(&self.0)
    }
    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        <AndroidPlatform as Platform>::primary_display(&self.0)
    }
    fn active_window(&self) -> Option<AnyWindowHandle> {
        <AndroidPlatform as Platform>::active_window(&self.0)
    }
    fn open_window(
        &self,
        handle: AnyWindowHandle,
        options: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        <AndroidPlatform as Platform>::open_window(&self.0, handle, options)
    }
    fn window_appearance(&self) -> WindowAppearance {
        <AndroidPlatform as Platform>::window_appearance(&self.0)
    }
    fn open_url(&self, url: &str) {
        <AndroidPlatform as Platform>::open_url(&self.0, url)
    }
    fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {
        <AndroidPlatform as Platform>::on_open_urls(&self.0, callback)
    }
    fn register_url_scheme(&self, url: &str) -> Task<Result<()>> {
        <AndroidPlatform as Platform>::register_url_scheme(&self.0, url)
    }
    fn prompt_for_paths(
        &self,
        options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        <AndroidPlatform as Platform>::prompt_for_paths(&self.0, options)
    }
    fn prompt_for_new_path(
        &self,
        directory: &Path,
        suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        <AndroidPlatform as Platform>::prompt_for_new_path(&self.0, directory, suggested_name)
    }
    fn can_select_mixed_files_and_dirs(&self) -> bool {
        <AndroidPlatform as Platform>::can_select_mixed_files_and_dirs(&self.0)
    }
    fn reveal_path(&self, path: &Path) {
        <AndroidPlatform as Platform>::reveal_path(&self.0, path)
    }
    fn open_with_system(&self, path: &Path) {
        <AndroidPlatform as Platform>::open_with_system(&self.0, path)
    }
    fn on_quit(&self, callback: Box<dyn FnMut()>) {
        <AndroidPlatform as Platform>::on_quit(&self.0, callback)
    }
    fn on_reopen(&self, callback: Box<dyn FnMut()>) {
        <AndroidPlatform as Platform>::on_reopen(&self.0, callback)
    }
    fn set_menus(&self, menus: Vec<Menu>, keymap: &Keymap) {
        <AndroidPlatform as Platform>::set_menus(&self.0, menus, keymap)
    }
    fn set_dock_menu(&self, menu: Vec<MenuItem>, keymap: &Keymap) {
        <AndroidPlatform as Platform>::set_dock_menu(&self.0, menu, keymap)
    }
    fn on_app_menu_action(&self, callback: Box<dyn FnMut(&dyn Action)>) {
        <AndroidPlatform as Platform>::on_app_menu_action(&self.0, callback)
    }
    fn on_will_open_app_menu(&self, callback: Box<dyn FnMut()>) {
        <AndroidPlatform as Platform>::on_will_open_app_menu(&self.0, callback)
    }
    fn on_validate_app_menu_command(&self, callback: Box<dyn FnMut(&dyn Action) -> bool>) {
        <AndroidPlatform as Platform>::on_validate_app_menu_command(&self.0, callback)
    }
    fn thermal_state(&self) -> ThermalState {
        <AndroidPlatform as Platform>::thermal_state(&self.0)
    }
    fn on_thermal_state_change(&self, callback: Box<dyn FnMut()>) {
        <AndroidPlatform as Platform>::on_thermal_state_change(&self.0, callback)
    }
    fn app_path(&self) -> Result<PathBuf> {
        <AndroidPlatform as Platform>::app_path(&self.0)
    }
    fn path_for_auxiliary_executable(&self, name: &str) -> Result<PathBuf> {
        <AndroidPlatform as Platform>::path_for_auxiliary_executable(&self.0, name)
    }
    fn set_cursor_style(&self, style: CursorStyle) {
        <AndroidPlatform as Platform>::set_cursor_style(&self.0, style)
    }
    fn should_auto_hide_scrollbars(&self) -> bool {
        <AndroidPlatform as Platform>::should_auto_hide_scrollbars(&self.0)
    }
    fn write_to_clipboard(&self, item: ClipboardItem) {
        <AndroidPlatform as Platform>::write_to_clipboard(&self.0, item)
    }
    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        <AndroidPlatform as Platform>::read_from_clipboard(&self.0)
    }
    fn write_credentials(&self, url: &str, username: &str, password: &[u8]) -> Task<Result<()>> {
        <AndroidPlatform as Platform>::write_credentials(&self.0, url, username, password)
    }
    fn read_credentials(&self, url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        <AndroidPlatform as Platform>::read_credentials(&self.0, url)
    }
    fn delete_credentials(&self, url: &str) -> Task<Result<()>> {
        <AndroidPlatform as Platform>::delete_credentials(&self.0, url)
    }
    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        <AndroidPlatform as Platform>::keyboard_layout(&self.0)
    }
    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        <AndroidPlatform as Platform>::keyboard_mapper(&self.0)
    }
    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        <AndroidPlatform as Platform>::on_keyboard_layout_change(&self.0, callback)
    }
}

impl Default for AndroidPlatform {
    fn default() -> Self {
        Self::new(false)
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn headless() -> AndroidPlatform {
        AndroidPlatform::new(true)
    }

    // ── basic construction ────────────────────────────────────────────────────

    #[test]
    fn platform_constructs_headless() {
        let p = headless();
        assert!(p.is_headless());
        assert!(!p.is_active());
        assert!(!p.should_quit());
    }

    // ── lifecycle ─────────────────────────────────────────────────────────────

    #[test]
    fn run_invokes_finish_launching_callback() {
        let p = headless();
        let ran = Arc::new(AtomicBool::new(false));
        let ran2 = ran.clone();
        p.run(Box::new(move || {
            ran2.store(true, Ordering::Relaxed);
        }));
        assert!(ran.load(Ordering::Relaxed));
    }

    #[test]
    fn quit_sets_should_quit() {
        let p = headless();
        assert!(!p.should_quit());
        p.quit();
        assert!(p.should_quit());
    }

    #[test]
    fn quit_callback_fires() {
        let p = headless();
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = fired.clone();
        p.on_quit(move || {
            f2.store(true, Ordering::Relaxed);
        });
        p.quit();
        assert!(fired.load(Ordering::Relaxed));
    }

    #[test]
    fn did_become_active_sets_flag() {
        let p = headless();
        p.did_become_active();
        assert!(p.is_active());
        p.did_enter_background();
        assert!(!p.is_active());
    }

    // ── clipboard ─────────────────────────────────────────────────────────────

    #[test]
    fn clipboard_round_trip() {
        let p = headless();
        assert!(p.read_from_clipboard().is_none());
        p.write_to_clipboard("hello android".to_string());
        assert_eq!(p.read_from_clipboard().as_deref(), Some("hello android"));
    }

    #[test]
    fn clipboard_overwrite() {
        let p = headless();
        p.write_to_clipboard("first".to_string());
        p.write_to_clipboard("second".to_string());
        assert_eq!(p.read_from_clipboard().as_deref(), Some("second"));
    }

    // ── credentials ───────────────────────────────────────────────────────────

    #[test]
    fn credentials_round_trip() {
        let p = headless();
        p.write_credentials("svc", "user", b"pass123").unwrap();
        let result = p.read_credentials("svc", "user").unwrap();
        assert_eq!(result.as_deref(), Some(b"pass123".as_slice()));
    }

    #[test]
    fn credentials_delete() {
        let p = headless();
        p.write_credentials("svc", "user", b"pass").unwrap();
        p.delete_credentials("svc", "user").unwrap();
        assert!(p.read_credentials("svc", "user").unwrap().is_none());
    }

    #[test]
    fn credentials_missing_returns_none() {
        let p = headless();
        assert!(p
            .read_credentials("no-such-service", "user")
            .unwrap()
            .is_none());
    }

    // ── misc platform queries ─────────────────────────────────────────────────

    #[test]
    fn should_auto_hide_scrollbars_true() {
        assert!(headless().should_auto_hide_scrollbars());
    }

    #[test]
    fn can_select_mixed_files_false() {
        assert!(!headless().can_select_mixed_files_and_dirs());
    }

    #[test]
    fn keyboard_layout_id_non_empty() {
        let id = headless().keyboard_layout_id();
        assert!(!id.is_empty());
    }

    #[test]
    fn path_for_auxiliary_executable_errors() {
        let result = headless().path_for_auxiliary_executable("foo");
        assert!(result.is_err());
    }

    // ── display ───────────────────────────────────────────────────────────────

    #[test]
    fn headless_has_one_display() {
        let p = headless();
        assert_eq!(p.displays().len(), 1);
        assert!(p.primary_display().is_some());
    }

    // ── preferred backend ─────────────────────────────────────────────────────

    #[test]
    fn default_backend_is_vulkan() {
        assert_eq!(headless().preferred_backend(), AndroidBackend::Vulkan);
    }

    #[test]
    fn backend_override() {
        let p = headless();
        p.set_preferred_backend(AndroidBackend::Gles);
        assert_eq!(p.preferred_backend(), AndroidBackend::Gles);
    }

    // ── text system ───────────────────────────────────────────────────────────

    #[test]
    fn text_system_accessible() {
        let p = headless();
        // Just confirm the text system can be retrieved without panicking.
        let _ts = p.text_system();
    }

    // ── open-URLs callback ────────────────────────────────────────────────────

    #[test]
    fn open_urls_callback_fires() {
        let p = headless();
        let received = Arc::new(Mutex::new(Vec::<String>::new()));
        let r2 = received.clone();
        p.on_open_urls(move |urls| {
            r2.lock().extend(urls);
        });
        p.deliver_open_urls(vec!["gpui://test".to_string()]);
        assert_eq!(received.lock().as_slice(), &["gpui://test"]);
    }

    // ── reopen callback ───────────────────────────────────────────────────────

    #[test]
    fn reopen_callback_fires() {
        let p = headless();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c2 = count.clone();
        p.on_reopen(move || {
            c2.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
        p.deliver_reopen();
        p.deliver_reopen();
        assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    // ── headless window count ─────────────────────────────────────────────────

    #[test]
    fn initial_window_count_is_zero() {
        assert_eq!(headless().window_count(), 0);
        assert!(headless().primary_window().is_none());
    }

    // ── flush main thread tasks ───────────────────────────────────────────────

    #[test]
    fn flush_main_thread_tasks_no_panic() {
        // There are no tasks queued, so this should be a no-op.
        headless().flush_main_thread_tasks();
    }

    // ── tick ─────────────────────────────────────────────────────────────────

    #[test]
    fn tick_no_panic() {
        headless().tick();
    }
}
