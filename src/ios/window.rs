//! iOS Window implementation using UIWindow and UIViewController.
//!
//! iOS windows are fundamentally different from desktop windows:
//! - Always fullscreen (or split-screen on iPad)
//! - No title bar or window chrome
//! - Touch-based input
//! - Safe area insets for notch / home indicator
//!
//! The window is backed by a `UIWindow` containing a `UIViewController`
//! whose view is a custom `UIView` subclass that uses `CAMetalLayer` as its
//! backing layer.  The Metal layer is then handed to the Blade renderer.
//!
//! # Touch dispatch
//!
//! UIKit calls `touchesBegan:withEvent:` / `touchesMoved:withEvent:` /
//! `touchesEnded:withEvent:` / `touchesCancelled:withEvent:` on the
//! `GPUIMetalView` Objective-C class registered below.  Each of those
//! handlers reads the `gpui_window_ptr` ivar set during construction and
//! forwards the touch to `IosWindow::handle_touch`.
//!
//! # Rendering
//!
//! `gpui_ios_request_frame` (in `ffi.rs`) calls the closure stored in
//! `request_frame_callback`.  The GPUI framework sets that closure via
//! `on_request_frame`; it in turn calls `draw` which invokes the Blade
//! renderer.

use super::{
    events::{
        touch_began_to_mouse_down, touch_ended_to_mouse_up, touch_location_in_view,
        touch_modifiers, touch_moved_to_mouse_move, touch_phase, touch_tap_count, Modifiers,
        MouseButton, PlatformInput, Point, UITouchPhase,
    },
    text_input::{key_code_to_key_down, key_code_to_key_up},
    IosDisplay,
};
use super::{DevicePixels, Pixels, Size};
use anyhow::{anyhow, Result};
use core_graphics::{
    base::CGFloat,
    geometry::{CGPoint, CGRect, CGSize},
};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel, BOOL, NO, YES},
    sel, sel_impl,
};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr::{self, NonNull},
    rc::Rc,
    sync::Once,
};

// ── Ivar name embedded in the GPUIMetalView ObjC class ───────────────────────

const GPUI_WINDOW_IVAR: &str = "gpui_window_ptr";

// ── One-time ObjC class registration ─────────────────────────────────────────

static METAL_VIEW_REGISTERED: Once = Once::new();

/// Register a custom `UIView` subclass whose backing layer is `CAMetalLayer`.
///
/// The class also installs `touchesBegan/Moved/Ended/Cancelled` handlers that
/// read the `gpui_window_ptr` ivar and forward touches to Rust.
fn register_metal_view_class() -> &'static Class {
    METAL_VIEW_REGISTERED.call_once(|| {
        let superclass = class!(UIView);
        let mut decl = ClassDecl::new("GPUIMetalView", superclass)
            .expect("GPUIMetalView class already registered");

        // Store a raw pointer back to the owning IosWindow.
        decl.add_ivar::<*mut c_void>(GPUI_WINDOW_IVAR);

        // ── Class method: layerClass → CAMetalLayer ──────────────────────────

        extern "C" fn layer_class(_self: &Class, _sel: Sel) -> *const Class {
            class!(CAMetalLayer) as *const Class
        }

        // ── Touch handler helpers ────────────────────────────────────────────

        fn forward_touches(this: &mut Object, touches: *mut Object, event: *mut Object) {
            unsafe {
                let window_ptr: *mut c_void = *this.get_ivar(GPUI_WINDOW_IVAR);
                if window_ptr.is_null() {
                    return;
                }
                let window = &*(window_ptr as *const IosWindow);
                let all: *mut Object = msg_send![touches, allObjects];
                let count: usize = msg_send![all, count];
                for i in 0..count {
                    let touch: *mut Object = msg_send![all, objectAtIndex: i];
                    window.handle_touch(touch, event);
                }
            }
        }

        extern "C" fn touches_began(
            this: &mut Object,
            _: Sel,
            touches: *mut Object,
            event: *mut Object,
        ) {
            forward_touches(this, touches, event);
        }

        extern "C" fn touches_moved(
            this: &mut Object,
            _: Sel,
            touches: *mut Object,
            event: *mut Object,
        ) {
            forward_touches(this, touches, event);
        }

        extern "C" fn touches_ended(
            this: &mut Object,
            _: Sel,
            touches: *mut Object,
            event: *mut Object,
        ) {
            forward_touches(this, touches, event);
        }

        extern "C" fn touches_cancelled(
            this: &mut Object,
            _: Sel,
            touches: *mut Object,
            event: *mut Object,
        ) {
            forward_touches(this, touches, event);
        }

        unsafe {
            decl.add_class_method(
                sel!(layerClass),
                layer_class as extern "C" fn(&Class, Sel) -> *const Class,
            );
            decl.add_method(
                sel!(touchesBegan:withEvent:),
                touches_began as extern "C" fn(&mut Object, Sel, *mut Object, *mut Object),
            );
            decl.add_method(
                sel!(touchesMoved:withEvent:),
                touches_moved as extern "C" fn(&mut Object, Sel, *mut Object, *mut Object),
            );
            decl.add_method(
                sel!(touchesEnded:withEvent:),
                touches_ended as extern "C" fn(&mut Object, Sel, *mut Object, *mut Object),
            );
            decl.add_method(
                sel!(touchesCancelled:withEvent:),
                touches_cancelled as extern "C" fn(&mut Object, Sel, *mut Object, *mut Object),
            );
        }

        decl.register();
    });

    class!(GPUIMetalView)
}

// ── IosWindow ─────────────────────────────────────────────────────────────────

/// A GPUI window on iOS, backed by `UIWindow + UIViewController + GPUIMetalView`.
///
/// All fields are accessed from the UIKit main thread only.
pub struct IosWindow {
    // ── ObjC objects ────────────────────────────────────────────────────────
    /// The `UIWindow` that hosts the whole view hierarchy.
    pub(crate) ui_window: *mut Object,
    /// The root `UIViewController`.
    view_controller: *mut Object,
    /// The `GPUIMetalView` (Metal-backed `UIView`).
    pub(crate) view: *mut Object,
    /// A tiny off-screen `UIView` that becomes first-responder to drive the
    /// soft keyboard.
    text_input_view: *mut Object,

    // ── Display geometry ─────────────────────────────────────────────────────
    /// Window bounds in logical pixels (points).
    bounds: Cell<Bounds>,
    /// Scale factor (e.g. `3.0` on an iPhone 15 Pro).
    scale_factor: Cell<f32>,

    // ── Input state ──────────────────────────────────────────────────────────
    /// Last known touch / pointer position in logical pixels.
    mouse_position: Cell<Point<Pixels>>,
    /// Current modifier state (relevant for external keyboards).
    modifiers: Cell<Modifiers>,
    /// Whether a touch is currently in the `Began` → `Ended` window.
    touch_pressed: Cell<bool>,

    // ── GPUI callbacks (set by the framework) ────────────────────────────────
    /// Called on each CADisplayLink tick to produce a new frame.
    /// `pub(super)` so `ffi.rs` can access it directly.
    pub(super) request_frame_callback: RefCell<Option<Box<dyn FnMut()>>>,
    /// Called for every `PlatformInput` event.
    input_callback: RefCell<Option<Box<dyn FnMut(PlatformInput)>>>,
    /// Called when the app transitions between foreground and background.
    active_status_callback: RefCell<Option<Box<dyn FnMut(bool)>>>,
    /// Called when the window is resized (rare on iOS but possible on iPad
    /// during Stage Manager / Split View transitions).
    resize_callback: RefCell<Option<Box<dyn FnMut(Size<Pixels>, f32)>>>,
    /// Called once when the window is closed / destroyed.
    close_callback: RefCell<Option<Box<dyn FnOnce()>>>,
    /// Called when the system appearance (light / dark mode) changes.
    appearance_changed_callback: RefCell<Option<Box<dyn FnMut()>>>,

    // ── Opaque window handle ─────────────────────────────────────────────────
    /// Application-level window ID (matches the GPUI `AnyWindowHandle` id).
    pub(crate) handle_id: u64,
}

/// Simple axis-aligned bounding rectangle in logical pixels.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Bounds {
    fn from_cgrect(r: CGRect) -> Self {
        Bounds {
            x: r.origin.x as f32,
            y: r.origin.y as f32,
            width: r.size.width as f32,
            height: r.size.height as f32,
        }
    }

    pub fn size(&self) -> Size<Pixels> {
        Size {
            width: Pixels(self.width),
            height: Pixels(self.height),
        }
    }
}

// SAFETY: `IosWindow` is only accessed from the UIKit main thread.
unsafe impl Send for IosWindow {}
unsafe impl Sync for IosWindow {}

impl IosWindow {
    // ── Construction ──────────────────────────────────────────────────────────

    /// Create a new `IosWindow` covering the full main screen.
    ///
    /// This:
    /// 1. Allocates and initialises a `UIWindow`
    /// 2. Creates a `UIViewController` with a `GPUIMetalView` root view
    /// 3. Configures the `CAMetalLayer` for Metal rendering
    /// 4. Makes the window key and visible
    /// 5. Creates a hidden text-input view for soft-keyboard routing
    pub fn new(handle_id: u64) -> Result<Self> {
        let display = IosDisplay::main();
        let (logical_w, logical_h) = display.logical_size();
        let scale = display.scale();

        unsafe {
            // ── UIScreen / bounds ────────────────────────────────────────────
            let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
            let screen_rect: CGRect = msg_send![screen, bounds];
            let screen_scale: CGFloat = msg_send![screen, scale];

            // ── UIWindow ─────────────────────────────────────────────────────
            let ui_window: *mut Object = {
                let alloc: *mut Object = msg_send![class!(UIWindow), alloc];
                msg_send![alloc, initWithFrame: screen_rect]
            };
            if ui_window.is_null() {
                return Err(anyhow!("Failed to create UIWindow"));
            }

            // ── UIViewController ─────────────────────────────────────────────
            let view_controller: *mut Object = {
                let alloc: *mut Object = msg_send![class!(UIViewController), alloc];
                msg_send![alloc, init]
            };
            if view_controller.is_null() {
                return Err(anyhow!("Failed to create UIViewController"));
            }

            // ── GPUIMetalView ─────────────────────────────────────────────────
            let metal_view_class = register_metal_view_class();
            let view: *mut Object = {
                let alloc: *mut Object = msg_send![metal_view_class, alloc];
                msg_send![alloc, initWithFrame: screen_rect]
            };
            if view.is_null() {
                return Err(anyhow!("Failed to create GPUIMetalView"));
            }

            // ── CAMetalLayer configuration ────────────────────────────────────
            let layer: *mut Object = msg_send![view, layer];

            // Obtain the default Metal device.
            #[link(name = "Metal", kind = "framework")]
            unsafe extern "C" {
                fn MTLCreateSystemDefaultDevice() -> *mut Object;
            }
            let device = MTLCreateSystemDefaultDevice();
            if !device.is_null() {
                let _: () = msg_send![layer, setDevice: device];
            }

            // MTLPixelFormatBGRA8Unorm = 80
            let _: () = msg_send![layer, setPixelFormat: 80_u64];
            // Allow the GPU to sample from the drawable texture.
            let _: () = msg_send![layer, setFramebufferOnly: NO];
            let _: () = msg_send![layer, setContentsScale: screen_scale];

            // Set drawable size in device pixels.
            let drawable_size = CGSize {
                width: screen_rect.size.width * screen_scale,
                height: screen_rect.size.height * screen_scale,
            };
            let _: () = msg_send![layer, setDrawableSize: drawable_size];

            // Enable multi-touch on the view.
            let _: () = msg_send![view, setUserInteractionEnabled: YES];
            let _: () = msg_send![view, setMultipleTouchEnabled: YES];

            // ── Hook up view hierarchy ────────────────────────────────────────
            let _: () = msg_send![view_controller, setView: view];
            let _: () = msg_send![ui_window, setRootViewController: view_controller];
            let _: () = msg_send![ui_window, makeKeyAndVisible];

            // ── Hidden text-input view for soft keyboard ──────────────────────
            let text_input_view: *mut Object = {
                let tiny = CGRect {
                    origin: CGPoint { x: 0.0, y: 0.0 },
                    size: CGSize {
                        width: 1.0,
                        height: 1.0,
                    },
                };
                let alloc: *mut Object = msg_send![class!(UIView), alloc];
                let v: *mut Object = msg_send![alloc, initWithFrame: tiny];
                // Nearly transparent — still receives first-responder.
                let _: () = msg_send![v, setAlpha: 0.01_f64];
                let _: () = msg_send![v, setUserInteractionEnabled: YES];
                let _: () = msg_send![view, addSubview: v];
                v
            };

            let bounds = Bounds {
                x: 0.0,
                y: 0.0,
                width: logical_w,
                height: logical_h,
            };

            Ok(IosWindow {
                ui_window,
                view_controller,
                view,
                text_input_view,
                bounds: Cell::new(bounds),
                scale_factor: Cell::new(scale),
                mouse_position: Cell::new(Point::new(Pixels(0.0), Pixels(0.0))),
                modifiers: Cell::new(Modifiers::default()),
                touch_pressed: Cell::new(false),
                request_frame_callback: RefCell::new(None),
                input_callback: RefCell::new(None),
                active_status_callback: RefCell::new(None),
                resize_callback: RefCell::new(None),
                close_callback: RefCell::new(None),
                appearance_changed_callback: RefCell::new(None),
                handle_id,
            })
        }
    }

    // ── FFI registration ──────────────────────────────────────────────────────

    /// Register this window with the FFI layer and set the back-pointer on the
    /// `GPUIMetalView` so touch events can find us.
    ///
    /// Must be called **after** the `IosWindow` has been placed at a stable
    /// address (i.e. inside a `Box`).
    pub(crate) fn register_with_ffi(&self) {
        super::ffi::register_window(self as *const Self);

        unsafe {
            let ptr = self as *const Self as *mut c_void;
            (*self.view).set_ivar(GPUI_WINDOW_IVAR, ptr);
            log::info!(
                "GPUI iOS Window: registered at {:p}, view {:p}",
                ptr,
                self.view
            );
        }
    }

    // ── Touch handling ────────────────────────────────────────────────────────

    /// Translate a raw `UITouch` / `UIEvent` pair into a `PlatformInput` event
    /// and deliver it to the registered input callback.
    ///
    /// # Safety
    /// `touch` must be a valid live `UITouch *`.
    /// `event` may be null (we only inspect the touch, not the event envelope).
    pub fn handle_touch(&self, touch: *mut Object, _event: *mut Object) {
        let position = touch_location_in_view(touch, self.view);
        let phase = touch_phase(touch);
        let tap_count = touch_tap_count(touch);
        let mods = self.modifiers.get();

        self.mouse_position.set(position);

        let platform_input = match phase {
            UITouchPhase::Began => {
                self.touch_pressed.set(true);
                touch_began_to_mouse_down(position, tap_count, mods)
            }
            UITouchPhase::Moved => {
                touch_moved_to_mouse_move(position, mods, Some(MouseButton::Left))
            }
            UITouchPhase::Ended | UITouchPhase::Cancelled => {
                self.touch_pressed.set(false);
                touch_ended_to_mouse_up(position, tap_count, mods)
            }
            UITouchPhase::Stationary => return,
        };

        if let Some(cb) = self.input_callback.borrow_mut().as_mut() {
            cb(platform_input);
        }
    }

    // ── Keyboard ──────────────────────────────────────────────────────────────

    /// Make the hidden text-input view the first responder, which causes UIKit
    /// to show the on-screen keyboard.
    pub fn show_keyboard(&self) {
        unsafe {
            let _: BOOL = msg_send![self.text_input_view, becomeFirstResponder];
        }
        log::info!("GPUI iOS Window: show_keyboard");
    }

    /// Resign first responder on the text-input view, hiding the keyboard.
    pub fn hide_keyboard(&self) {
        unsafe {
            let _: BOOL = msg_send![self.text_input_view, resignFirstResponder];
        }
        log::info!("GPUI iOS Window: hide_keyboard");
    }

    /// Handle a string of characters delivered by the soft keyboard.
    ///
    /// `text_ns_string` is an `NSString *` cast to `*mut Object`.
    pub fn handle_text_input(&self, text_ns_string: *mut Object) {
        if text_ns_string.is_null() {
            return;
        }

        let text: String = unsafe {
            let utf8: *const i8 = msg_send![text_ns_string, UTF8String];
            if utf8.is_null() {
                return;
            }
            std::ffi::CStr::from_ptr(utf8)
                .to_string_lossy()
                .into_owned()
        };

        log::debug!("GPUI iOS Window: text input {:?}", text);

        // Re-use the key-event path: emit one KeyDown per character.
        for c in text.chars() {
            use super::text_input::{character_to_key_down, PlatformKeyEvent};
            let ev = character_to_key_down(c);

            // Convert to a generic PlatformInput so the single input_callback
            // handles both touch and keyboard events.
            // (In a full GPUI integration you'd dispatch KeyDown / KeyUp
            //  through the proper gpui::PlatformInput variants.)
            if let PlatformKeyEvent::KeyDown(key_ev) = ev {
                log::trace!("  char KeyDown: {:?}", key_ev.keystroke.key);
            }
        }
    }

    /// Handle a hardware-keyboard key event from an external keyboard.
    ///
    /// - `key_code`   : USB HID keyboard usage code (`UIKeyboardHIDUsage`)
    /// - `modifier_flags` : `UIKeyModifierFlags` bitmask
    /// - `is_key_down`: `true` for key-down, `false` for key-up
    pub fn handle_key_event(&self, key_code: u32, modifier_flags: u32, is_key_down: bool) {
        use super::text_input::{
            key_code_to_string, modifier_flags_to_modifiers, PlatformKeyEvent,
        };

        let key = key_code_to_string(key_code);
        let mods = modifier_flags_to_modifiers(modifier_flags);

        log::debug!(
            "GPUI iOS Window: key_event key={:?} mods={:?} down={}",
            key,
            mods,
            is_key_down,
        );

        let ev = if is_key_down {
            key_code_to_key_down(key_code, modifier_flags)
        } else {
            key_code_to_key_up(key_code, modifier_flags)
        };

        // A full integration would dispatch through gpui::PlatformInput::KeyDown/Up.
        log::trace!("  platform key event: {:?}", ev);
    }

    // ── Active-status changes ─────────────────────────────────────────────────

    /// Notify the window that the application's active status has changed.
    ///
    /// Called by `ffi.rs` in response to `UIApplicationDelegate` lifecycle
    /// callbacks.
    pub fn notify_active_status_change(&self, is_active: bool) {
        log::info!(
            "GPUI iOS Window: active status → {}",
            if is_active { "active" } else { "inactive" }
        );
        if let Some(cb) = self.active_status_callback.borrow_mut().as_mut() {
            cb(is_active);
        }
    }

    // ── Geometry ──────────────────────────────────────────────────────────────

    /// Returns the current window bounds in logical pixels (points).
    pub fn bounds(&self) -> Bounds {
        self.bounds.get()
    }

    /// Returns the screen scale factor.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor.get()
    }

    /// Returns the content size (equals `bounds.size()` on iOS — always
    /// fullscreen).
    pub fn content_size(&self) -> Size<Pixels> {
        self.bounds.get().size()
    }

    /// Returns the safe-area insets `(top, left, bottom, right)` in points.
    ///
    /// Used to avoid drawing behind the notch and home indicator.
    pub fn safe_area_insets(&self) -> (f32, f32, f32, f32) {
        #[repr(C)]
        struct UIEdgeInsets {
            top: f64,
            left: f64,
            bottom: f64,
            right: f64,
        }
        unsafe {
            let insets: UIEdgeInsets = msg_send![self.view, safeAreaInsets];
            (
                insets.top as f32,
                insets.left as f32,
                insets.bottom as f32,
                insets.right as f32,
            )
        }
    }

    // ── Callback registration (called by the GPUI framework) ──────────────────

    /// Register the frame-request callback.  Called on every CADisplayLink tick
    /// via `gpui_ios_request_frame`.
    pub fn on_request_frame(&self, callback: Box<dyn FnMut()>) {
        *self.request_frame_callback.borrow_mut() = Some(callback);
    }

    /// Register the input-event callback.
    pub fn on_input(&self, callback: Box<dyn FnMut(PlatformInput)>) {
        *self.input_callback.borrow_mut() = Some(callback);
    }

    /// Register the active-status-change callback.
    pub fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        *self.active_status_callback.borrow_mut() = Some(callback);
    }

    /// Register the resize callback.
    pub fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        *self.resize_callback.borrow_mut() = Some(callback);
    }

    /// Register the close callback (called at most once).
    pub fn on_close(&self, callback: Box<dyn FnOnce()>) {
        *self.close_callback.borrow_mut() = Some(callback);
    }

    /// Register the appearance-changed callback.
    pub fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        *self.appearance_changed_callback.borrow_mut() = Some(callback);
    }

    // ── Miscellaneous UIKit helpers ───────────────────────────────────────────

    /// Returns whether this window is currently the key window.
    pub fn is_active(&self) -> bool {
        unsafe {
            let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
            let key: *mut Object = msg_send![app, keyWindow];
            self.ui_window == key
        }
    }

    /// Bring the window to the front and make it key.
    pub fn activate(&self) {
        unsafe {
            let _: () = msg_send![self.ui_window, makeKeyAndVisible];
        }
    }

    /// iOS windows are always fullscreen.
    pub fn is_fullscreen(&self) -> bool {
        true
    }

    /// Current light/dark appearance.
    pub fn appearance(&self) -> super::platform::WindowAppearance {
        unsafe {
            let tc: *mut Object = msg_send![self.view, traitCollection];
            let style: i64 = msg_send![tc, userInterfaceStyle];
            match style {
                2 => super::platform::WindowAppearance::Dark,
                1 => super::platform::WindowAppearance::Light,
                _ => super::platform::WindowAppearance::Unknown,
            }
        }
    }

    /// Last pointer position (from the most recent touch event).
    pub fn mouse_position(&self) -> Point<Pixels> {
        self.mouse_position.get()
    }

    /// Current modifier state.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers.get()
    }
}

impl Drop for IosWindow {
    fn drop(&mut self) {
        // Invoke the close callback, if any.
        if let Some(cb) = self.close_callback.borrow_mut().take() {
            cb();
        }
        log::info!("GPUI iOS Window: dropped (handle_id={})", self.handle_id);
    }
}
