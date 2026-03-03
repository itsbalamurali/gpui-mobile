//! iOS Platform implementation for GPUI.
//!
//! Implements the `Platform`-like interface for iOS using UIKit.
//!
//! Key differences from macOS:
//! - Uses `UIApplication` instead of `NSApplication`
//! - No menu bar (iOS apps don't have traditional menus)
//! - No windowed mode (iOS apps are always fullscreen on their display)
//! - Touch-based input instead of mouse
//! - System keyboard handling differs significantly
//! - Apps cannot programmatically quit or restart
//!
//! The platform lifecycle is driven externally by the iOS app delegate
//! (Objective-C / Swift code) calling into the C FFI layer defined in
//! `ffi.rs`.  `IosPlatform::run()` simply stores the finish-launching
//! callback and returns; the callback is invoked later by
//! `gpui_ios_did_finish_launching()`.

use super::{IosDispatcher, IosDisplay, IosWindow};
use anyhow::{anyhow, Result};
use core_graphics::geometry::CGRect;
use objc::{class, msg_send, runtime::Object, sel, sel_impl};
use parking_lot::Mutex;
use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

// ---------------------------------------------------------------------------
// Window appearance
// ---------------------------------------------------------------------------

/// The light / dark appearance of a window or the whole application.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WindowAppearance {
    Light,
    Dark,
    /// The system appearance could not be determined.
    Unknown,
}

// ---------------------------------------------------------------------------
// Platform state
// ---------------------------------------------------------------------------

struct IosPlatformState {
    background_executor: Arc<IosDispatcher>,
    foreground_executor: Arc<IosDispatcher>,

    /// The callback supplied to `run()`.  It is stored here and forwarded to
    /// the FFI layer so Objective-C can invoke it at the right moment in the
    /// iOS app-launch sequence.
    finish_launching: Option<Box<dyn FnOnce()>>,

    /// Optional callback invoked when the app is about to quit / terminate.
    quit_callback: Option<Box<dyn FnMut()>>,

    /// Optional callback invoked when the app is asked to open URLs
    /// (e.g. via a custom URL scheme registered in Info.plist).
    open_urls_callback: Option<Box<dyn FnMut(Vec<String>)>>,
}

// ---------------------------------------------------------------------------
// IosPlatform
// ---------------------------------------------------------------------------

/// The GPUI platform implementation for iOS.
///
/// Wrap in `Rc` (never `Arc`) — GPUI platforms are single-threaded.
pub struct IosPlatform(Mutex<IosPlatformState>);

impl Default for IosPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl IosPlatform {
    /// Construct a new `IosPlatform`.
    ///
    /// Both the background and foreground executors share the same
    /// `IosDispatcher` instance; the dispatcher itself decides which GCD
    /// queue to use based on whether the caller requests main-thread or
    /// background scheduling.
    pub fn new() -> Self {
        let dispatcher = Arc::new(IosDispatcher);

        IosPlatform(Mutex::new(IosPlatformState {
            background_executor: dispatcher.clone(),
            foreground_executor: dispatcher,
            finish_launching: None,
            quit_callback: None,
            open_urls_callback: None,
        }))
    }

    // -----------------------------------------------------------------------
    // Platform trait methods
    // -----------------------------------------------------------------------

    /// Returns the background-task dispatcher (GCD global queue).
    pub fn background_executor(&self) -> Arc<IosDispatcher> {
        self.0.lock().background_executor.clone()
    }

    /// Returns the foreground-task dispatcher (GCD main queue).
    pub fn foreground_executor(&self) -> Arc<IosDispatcher> {
        self.0.lock().foreground_executor.clone()
    }

    /// Store the finish-launching callback and forward it to the FFI layer.
    ///
    /// On iOS the UIKit run loop is already running when GPUI code executes,
    /// so unlike macOS we do **not** call `UIApplicationMain` here.  Instead
    /// we store the callback; it will be invoked by `gpui_ios_did_finish_launching`
    /// which is called from the Objective-C app delegate.
    pub fn run(&self, on_finish_launching: Box<dyn FnOnce() + 'static>) {
        self.0.lock().finish_launching = Some(on_finish_launching);

        // Forward to the FFI layer immediately so Objective-C can pick it up.
        if let Some(cb) = self.0.lock().finish_launching.take() {
            super::ffi::set_finish_launching_callback(cb);
        }

        log::info!("GPUI iOS: IosPlatform::run() stored finish-launching callback");
    }

    /// iOS apps cannot quit programmatically — only the system / user can
    /// terminate them.
    pub fn quit(&self) {
        log::warn!("GPUI iOS: quit() called — iOS apps cannot terminate themselves");
    }

    /// iOS apps cannot restart themselves.
    pub fn restart(&self, _binary_path: Option<PathBuf>) {
        log::warn!("GPUI iOS: restart() called — not supported on iOS");
    }

    /// App activation is managed by UIKit automatically.
    pub fn activate(&self, _ignoring_other_apps: bool) {}

    /// iOS apps cannot hide themselves.
    pub fn hide(&self) {}

    /// Not applicable on iOS.
    pub fn hide_other_apps(&self) {}

    /// Not applicable on iOS.
    pub fn unhide_other_apps(&self) {}

    /// Returns all currently connected displays.
    pub fn displays(&self) -> Vec<Rc<IosDisplay>> {
        IosDisplay::all().map(Rc::new).collect()
    }

    /// Returns the primary (built-in) display.
    pub fn primary_display(&self) -> Option<Rc<IosDisplay>> {
        Some(Rc::new(IosDisplay::main()))
    }

    /// Opens a new `IosWindow` and registers it with the FFI layer.
    pub fn open_window(
        &self,
        handle: u64, // opaque window handle id
        _title: Option<&str>,
    ) -> Result<Box<IosWindow>> {
        let window = Box::new(IosWindow::new(handle)?);
        // Register with the FFI layer so Objective-C can retrieve the pointer.
        window.register_with_ffi();
        Ok(window)
    }

    /// Returns the current window appearance (light / dark mode).
    pub fn window_appearance(&self) -> WindowAppearance {
        unsafe {
            let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
            let key_window: *mut Object = msg_send![app, keyWindow];
            if key_window.is_null() {
                return WindowAppearance::Light;
            }
            let trait_collection: *mut Object = msg_send![key_window, traitCollection];
            // UIUserInterfaceStyle: 0 = unspecified, 1 = light, 2 = dark
            let style: i64 = msg_send![trait_collection, userInterfaceStyle];
            match style {
                2 => WindowAppearance::Dark,
                1 => WindowAppearance::Light,
                _ => WindowAppearance::Unknown,
            }
        }
    }

    /// Open a URL using `UIApplication.openURL`.
    pub fn open_url(&self, url: &str) {
        unsafe {
            // Build NSString from the URL string.
            let url_cstr = std::ffi::CString::new(url).unwrap_or_default();
            let url_nsstring: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: url_cstr.as_ptr()];
            let nsurl: *mut Object = msg_send![class!(NSURL), URLWithString: url_nsstring];
            if nsurl.is_null() {
                log::error!("GPUI iOS: open_url — invalid URL: {}", url);
                return;
            }
            let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
            let nil: *mut Object = std::ptr::null_mut();
            let _: () = msg_send![app, openURL: nsurl options: nil completionHandler: nil];
        }
    }

    /// Register a callback invoked when the app is asked to open URLs.
    pub fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {
        self.0.lock().open_urls_callback = Some(callback);
    }

    /// URL schemes on iOS are declared in `Info.plist`, not registered at
    /// runtime, so this is a no-op.
    pub fn register_url_scheme(&self, _scheme: &str) -> Result<()> {
        Ok(())
    }

    // ── File-system prompts ─────────────────────────────────────────────────

    /// iOS uses `UIDocumentPickerViewController` for file selection.
    /// Full UIKit integration is left as future work; this returns an error.
    pub fn prompt_for_paths(&self, _options: ()) -> Result<Option<Vec<PathBuf>>> {
        Err(anyhow!("File picker not yet implemented for iOS"))
    }

    /// Save dialogs are not yet implemented.
    pub fn prompt_for_new_path(&self, _directory: &Path) -> Result<Option<PathBuf>> {
        Err(anyhow!("Save dialog not yet implemented for iOS"))
    }

    /// iOS does not support mixed file + directory selection.
    pub fn can_select_mixed_files_and_dirs(&self) -> bool {
        false
    }

    /// iOS does not have a Finder-equivalent "reveal in shell" action.
    pub fn reveal_path(&self, _path: &Path) {}

    /// Would use `UIActivityViewController` to open a file with another app.
    pub fn open_with_system(&self, _path: &Path) {
        log::warn!("GPUI iOS: open_with_system not yet implemented");
    }

    // ── App lifecycle callbacks ─────────────────────────────────────────────

    /// Register a callback to be invoked when the app is about to terminate.
    pub fn on_quit(&self, callback: Box<dyn FnMut()>) {
        self.0.lock().quit_callback = Some(callback);
    }

    /// iOS handles app "reopen" through scene lifecycle; no-op here.
    pub fn on_reopen(&self, _callback: Box<dyn FnMut()>) {}

    // ── Menus ───────────────────────────────────────────────────────────────

    /// iOS does not have a menu bar.
    pub fn set_menus(&self, _menus: Vec<()>) {}

    /// iOS does not have a dock menu.
    pub fn set_dock_menu(&self, _menu: Vec<()>) {}

    // ── App bundle ─────────────────────────────────────────────────────────

    /// Returns the path to the app bundle (`.app` directory).
    pub fn app_path(&self) -> Result<PathBuf> {
        unsafe {
            let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
            let path: *mut Object = msg_send![bundle, bundlePath];
            let utf8: *const i8 = msg_send![path, UTF8String];
            if utf8.is_null() {
                return Err(anyhow!("Failed to get bundle path"));
            }
            let path_str = std::ffi::CStr::from_ptr(utf8).to_str()?;
            Ok(PathBuf::from(path_str))
        }
    }

    /// Returns the path to an auxiliary executable inside the app bundle.
    pub fn path_for_auxiliary_executable(&self, name: &str) -> Result<PathBuf> {
        let app = self.app_path()?;
        Ok(app.join(name))
    }

    // ── Cursor / scrollbar ──────────────────────────────────────────────────

    /// iOS does not have a visible cursor (except Apple Pencil hover on iPad).
    pub fn set_cursor_style(&self, _style: ()) {}

    /// iOS always auto-hides scrollbars.
    pub fn should_auto_hide_scrollbars(&self) -> bool {
        true
    }

    // ── Clipboard ──────────────────────────────────────────────────────────

    /// Write plain text to `UIPasteboard.generalPasteboard`.
    pub fn write_to_clipboard(&self, text: &str) {
        unsafe {
            let pasteboard: *mut Object = msg_send![class!(UIPasteboard), generalPasteboard];
            let cstr = std::ffi::CString::new(text).unwrap_or_default();
            let ns_string: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: cstr.as_ptr()];
            let _: () = msg_send![pasteboard, setString: ns_string];
        }
    }

    /// Read plain text from `UIPasteboard.generalPasteboard`.
    pub fn read_from_clipboard(&self) -> Option<String> {
        unsafe {
            let pasteboard: *mut Object = msg_send![class!(UIPasteboard), generalPasteboard];
            let string: *mut Object = msg_send![pasteboard, string];
            if string.is_null() {
                return None;
            }
            let utf8: *const i8 = msg_send![string, UTF8String];
            if utf8.is_null() {
                return None;
            }
            std::ffi::CStr::from_ptr(utf8)
                .to_str()
                .ok()
                .map(|s| s.to_string())
        }
    }

    // ── Keychain ────────────────────────────────────────────────────────────

    /// Keychain access is not yet implemented.
    pub fn write_credentials(&self, _url: &str, _username: &str, _password: &[u8]) -> Result<()> {
        Err(anyhow!("Keychain not yet implemented for iOS"))
    }

    /// Keychain access is not yet implemented.
    pub fn read_credentials(&self, _url: &str) -> Result<Option<(String, Vec<u8>)>> {
        Err(anyhow!("Keychain not yet implemented for iOS"))
    }

    /// Keychain access is not yet implemented.
    pub fn delete_credentials(&self, _url: &str) -> Result<()> {
        Err(anyhow!("Keychain not yet implemented for iOS"))
    }

    // ── Keyboard layout ─────────────────────────────────────────────────────

    /// Returns a placeholder keyboard layout identifier.
    ///
    /// iOS does not expose keyboard layout APIs equivalent to macOS
    /// `TISCopyCurrentKeyboardLayoutInputSource`.
    pub fn keyboard_layout_id(&self) -> &'static str {
        "ios"
    }

    /// Called when the keyboard layout changes (no-op on iOS).
    pub fn on_keyboard_layout_change(&self, _callback: Box<dyn FnMut()>) {}

    // ── Internal helpers ────────────────────────────────────────────────────

    /// Invoke the stored quit callback, if any.
    pub(crate) fn invoke_quit_callback(&self) {
        if let Some(cb) = self.0.lock().quit_callback.as_mut() {
            cb();
        }
    }

    /// Deliver a list of opened URLs to the registered callback.
    pub(crate) fn deliver_open_urls(&self, urls: Vec<String>) {
        if let Some(cb) = self.0.lock().open_urls_callback.as_mut() {
            cb(urls);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_constructs_without_panic() {
        let _p = IosPlatform::new();
    }

    #[test]
    fn clipboard_round_trip() {
        // Only meaningful on a real device / simulator; this verifies the
        // call path compiles and doesn't panic on host builds.
        let p = IosPlatform::new();
        p.write_to_clipboard("hello gpui-mobile");
        // On a host build UIKit is absent, so read_from_clipboard returns None.
        // That is still a valid (non-panicking) result.
        let _ = p.read_from_clipboard();
    }

    #[test]
    fn auto_hide_scrollbars_is_true() {
        let p = IosPlatform::new();
        assert!(p.should_auto_hide_scrollbars());
    }

    #[test]
    fn keyboard_layout_id_is_non_empty() {
        let p = IosPlatform::new();
        assert!(!p.keyboard_layout_id().is_empty());
    }

    #[test]
    fn can_select_mixed_files_false() {
        let p = IosPlatform::new();
        assert!(!p.can_select_mixed_files_and_dirs());
    }
}
