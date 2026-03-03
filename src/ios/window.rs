//! iOS Window implementation using UIWindow and UIViewController.
//!
//! iOS windows are fundamentally different from desktop windows:
//! - Always fullscreen (or split-screen on iPad)
//! - No title bar or window chrome
//! - Touch-based input
//! - Safe area insets for notch/home indicator
//!
//! The window is backed by a UIWindow containing a UIViewController
//! whose view hosts a CAMetalLayer. Rendering is performed by
//! `gpui_wgpu::WgpuRenderer` which drives wgpu over the Metal backend.

use super::events::*;
use super::IosDisplay;
use crate::momentum::{MomentumScroller, VelocityTracker};
use gpui::{
    point, size, AnyWindowHandle, AtlasKey, AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds,
    Capslock, DevicePixels, DispatchEventResult, GpuSpecs, Modifiers, Pixels, PlatformAtlas,
    PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow, Point, PromptButton,
    PromptLevel, RequestFrameOptions, Scene, Size, TileId, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowParams,
};
use gpui_wgpu::{WgpuContext, WgpuRenderer, WgpuSurfaceConfig};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel, BOOL, NO, YES},
    sel, sel_impl,
};
use parking_lot::Mutex;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, UiKitDisplayHandle, UiKitWindowHandle};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::c_void,
    ptr::{self, NonNull},
    rc::Rc,
    sync::Arc,
};

const GPUI_WINDOW_IVAR: &str = "gpui_window_ptr";

static METAL_VIEW_CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();

/// Register a custom UIView subclass that uses CAMetalLayer as its backing layer.
/// This is required for Metal rendering on iOS.
fn register_metal_view_class() -> &'static Class {
    METAL_VIEW_CLASS_REGISTERED.call_once(|| {
        let superclass = class!(UIView);
        let mut decl = ClassDecl::new("GPUIMetalView", superclass).unwrap();

        // Add ivar to store window pointer for touch handling
        decl.add_ivar::<*mut std::ffi::c_void>(GPUI_WINDOW_IVAR);

        // Override layerClass to return CAMetalLayer
        extern "C" fn layer_class(_self: &Class, _sel: Sel) -> *const Class {
            class!(CAMetalLayer) as *const Class
        }

        // Touch handling methods
        extern "C" fn touches_began(
            this: &mut Object,
            _sel: Sel,
            touches: *mut Object,
            event: *mut Object,
        ) {
            handle_touches(this, touches, event);
        }

        extern "C" fn touches_moved(
            this: &mut Object,
            _sel: Sel,
            touches: *mut Object,
            event: *mut Object,
        ) {
            handle_touches(this, touches, event);
        }

        extern "C" fn touches_ended(
            this: &mut Object,
            _sel: Sel,
            touches: *mut Object,
            event: *mut Object,
        ) {
            handle_touches(this, touches, event);
        }

        extern "C" fn touches_cancelled(
            this: &mut Object,
            _sel: Sel,
            touches: *mut Object,
            event: *mut Object,
        ) {
            handle_touches(this, touches, event);
        }

        unsafe {
            // Add class method for layerClass
            decl.add_class_method(
                sel!(layerClass),
                layer_class as extern "C" fn(&Class, Sel) -> *const Class,
            );

            // Add touch handling instance methods
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

/// Handle touch events from the GPUIMetalView
fn handle_touches(view: &mut Object, touches: *mut Object, event: *mut Object) {
    unsafe {
        // Get the window pointer from the view's ivar
        let window_ptr: *mut std::ffi::c_void = *view.get_ivar(GPUI_WINDOW_IVAR);
        if window_ptr.is_null() {
            log::warn!("GPUI iOS: Touch event but no window pointer set");
            return;
        }

        let window = &*(window_ptr as *const IosWindow);

        // Get all touches from the set
        let all_touches: *mut Object = msg_send![touches, allObjects];
        let count: usize = msg_send![all_touches, count];

        for i in 0..count {
            let touch: *mut Object = msg_send![all_touches, objectAtIndex: i];
            window.handle_touch(touch, event);
        }
    }
}

/// iOS Window backed by UIWindow + UIViewController.
/// Distance (logical px) the finger must travel before a touch
/// is promoted from a potential tap to a scroll gesture.
const SCROLL_SLOP: f32 = 8.0;

/// Tracks the current touch gesture state machine.
///
/// This distinguishes taps (short, stationary touches) from scroll gestures
/// (finger drags). The same pattern is used on Android.
#[derive(Clone, Copy, Debug)]
enum TouchState {
    /// No active touch.
    Idle,
    /// Finger is down but hasn't moved beyond the slop threshold.
    Pending { start_x: f32, start_y: f32 },
    /// Finger has moved beyond the threshold — we are scrolling.
    Scrolling { prev_x: f32, prev_y: f32 },
}

pub(crate) struct IosWindow {
    /// Handle used by GPUI to identify this window
    handle: AnyWindowHandle,
    /// The UIWindow object
    window: *mut Object,
    /// The UIViewController
    view_controller: *mut Object,
    /// The Metal-backed UIView
    view: *mut Object,
    /// The hidden text input view for keyboard input
    text_input_view: *mut Object,
    /// Current bounds in pixels
    bounds: Cell<Bounds<Pixels>>,
    /// Scale factor
    scale_factor: Cell<f32>,
    /// Appearance (light/dark mode)
    appearance: Cell<WindowAppearance>,
    /// Input handler for text input
    input_handler: RefCell<Option<PlatformInputHandler>>,
    /// Callback for frame requests
    /// Note: pub(super) to allow ffi.rs to access this for the display link callback
    pub(super) request_frame_callback: RefCell<Option<Box<dyn FnMut(RequestFrameOptions)>>>,
    /// Callback for input events
    input_callback: RefCell<Option<Box<dyn FnMut(PlatformInput) -> DispatchEventResult>>>,
    /// Callback for active status changes
    active_status_callback: RefCell<Option<Box<dyn FnMut(bool)>>>,
    /// Callback for hover status changes (not really applicable on iOS)
    hover_status_callback: RefCell<Option<Box<dyn FnMut(bool)>>>,
    /// Callback for resize events
    resize_callback: RefCell<Option<Box<dyn FnMut(Size<Pixels>, f32)>>>,
    /// Callback for move events (not applicable on iOS)
    moved_callback: RefCell<Option<Box<dyn FnMut()>>>,
    /// Callback for should close
    should_close_callback: RefCell<Option<Box<dyn FnMut() -> bool>>>,
    /// Callback for hit test
    hit_test_callback: RefCell<Option<Box<dyn FnMut() -> Option<WindowControlArea>>>>,
    /// Callback for close
    close_callback: RefCell<Option<Box<dyn FnOnce()>>>,
    /// Callback for appearance changes
    appearance_changed_callback: RefCell<Option<Box<dyn FnMut()>>>,
    /// Current mouse position (from touch)
    mouse_position: Cell<Point<Pixels>>,
    /// Current modifiers
    modifiers: Cell<Modifiers>,
    /// Track if a touch is currently pressed
    touch_pressed: Cell<bool>,
    /// Touch gesture state machine — distinguishes taps from scroll drags.
    touch_state: Cell<TouchState>,
    /// Velocity tracker — records recent touch samples during drag gestures
    /// so we can compute the release velocity when the finger lifts.
    velocity_tracker: RefCell<VelocityTracker>,
    /// Momentum scroller — produces decelerating scroll deltas after a fling
    /// gesture, driven by the CADisplayLink frame callback.
    momentum_scroller: RefCell<MomentumScroller>,
    /// The wgpu renderer (Metal backend on iOS).
    /// Wrapped in a `Mutex<Option<…>>` so that `draw()` (called from the
    /// `request_frame` callback) can acquire a mutable reference without
    /// conflicting with the outer `&self` borrow.
    renderer: Mutex<Option<WgpuRenderer>>,
}

// Required for raw_window_handle
unsafe impl Send for IosWindow {}
unsafe impl Sync for IosWindow {}

impl IosWindow {
    pub fn new(handle: AnyWindowHandle, _params: WindowParams) -> anyhow::Result<Self> {
        // Create the window on the main screen
        let screen = IosDisplay::main();
        let screen_bounds = screen.bounds();
        let scale_factor = screen.scale();

        unsafe {
            // Create UIWindow
            let screen_obj: *mut Object = msg_send![class!(UIScreen), mainScreen];
            let screen_bounds_cg: core_graphics::geometry::CGRect = msg_send![screen_obj, bounds];
            let window: *mut Object = msg_send![class!(UIWindow), alloc];
            let window: *mut Object = msg_send![window, initWithFrame: screen_bounds_cg];

            // Create UIViewController
            let view_controller: *mut Object = msg_send![class!(UIViewController), alloc];
            let view_controller: *mut Object = msg_send![view_controller, init];

            // Create our custom Metal view using the registered class
            let metal_view_class = register_metal_view_class();
            let view: *mut Object = msg_send![metal_view_class, alloc];
            let view: *mut Object = msg_send![view, initWithFrame: screen_bounds_cg];

            // Configure the Metal layer — wgpu will use it for rendering but
            // we still need to set contentsScale so the drawable size is correct.
            let layer: *mut Object = msg_send![view, layer];
            let scale: core_graphics::base::CGFloat = msg_send![screen_obj, scale];
            let _: () = msg_send![layer, setContentsScale: scale];

            // Enable user interaction on the Metal view for touch handling
            let _: () = msg_send![view, setUserInteractionEnabled: YES];
            let _: () = msg_send![view, setMultipleTouchEnabled: YES];

            // Set the view as the view controller's view
            let _: () = msg_send![view_controller, setView: view];

            // Set the root view controller
            let _: () = msg_send![window, setRootViewController: view_controller];

            // Make the window visible
            let _: () = msg_send![window, makeKeyAndVisible];

            // Create a hidden text input view for keyboard handling
            let text_input_view: *mut Object = msg_send![class!(UIView), alloc];
            let text_input_frame = core_graphics::geometry::CGRect {
                origin: core_graphics::geometry::CGPoint { x: 0.0, y: 0.0 },
                size: core_graphics::geometry::CGSize {
                    width: 1.0,
                    height: 1.0,
                },
            };
            let text_input_view: *mut Object =
                msg_send![text_input_view, initWithFrame: text_input_frame];
            let _: () = msg_send![text_input_view, setAlpha: 0.01_f64];
            let _: () = msg_send![text_input_view, setUserInteractionEnabled: YES];
            let _: () = msg_send![view, addSubview: text_input_view];

            // --- Initialise the wgpu renderer (Metal backend) ---------------
            let pixel_w = (screen_bounds_cg.size.width * scale) as i32;
            let pixel_h = (screen_bounds_cg.size.height * scale) as i32;

            let ios_window = Self {
                handle,
                window,
                view_controller,
                view,
                text_input_view,
                bounds: Cell::new(screen_bounds),
                scale_factor: Cell::new(scale_factor),
                appearance: Cell::new(WindowAppearance::Light),
                input_handler: RefCell::new(None),
                request_frame_callback: RefCell::new(None),
                input_callback: RefCell::new(None),
                active_status_callback: RefCell::new(None),
                hover_status_callback: RefCell::new(None),
                resize_callback: RefCell::new(None),
                moved_callback: RefCell::new(None),
                should_close_callback: RefCell::new(None),
                hit_test_callback: RefCell::new(None),
                close_callback: RefCell::new(None),
                appearance_changed_callback: RefCell::new(None),
                mouse_position: Cell::new(Point::default()),
                modifiers: Cell::new(Modifiers::default()),
                touch_pressed: Cell::new(false),
                touch_state: Cell::new(TouchState::Idle),
                velocity_tracker: RefCell::new(VelocityTracker::new()),
                momentum_scroller: RefCell::new(MomentumScroller::new()),
                renderer: Mutex::new(None),
            };

            // Create the wgpu renderer using the Metal backend.
            //
            // `gpui_wgpu::WgpuContext::instance()` only enables Vulkan+GL,
            // so we create our own wgpu instance with Metal enabled, build
            // a surface from the UIView's raw window handle, construct the
            // WgpuContext with that instance, and finally create the renderer.
            let config = WgpuSurfaceConfig {
                size: size(DevicePixels(pixel_w), DevicePixels(pixel_h)),
                transparent: false,
            };

            let metal_instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::METAL,
                flags: wgpu::InstanceFlags::default(),
                ..Default::default()
            });

            // Build raw-window-handle types for the surface.
            let window_handle = ios_window
                .window_handle()
                .expect("iOS window handle unavailable");
            let display_handle = ios_window
                .display_handle()
                .expect("iOS display handle unavailable");

            let target = wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: display_handle.as_raw(),
                raw_window_handle: window_handle.as_raw(),
            };

            // Create a temporary surface just for WgpuContext initialisation
            // (adapter selection needs a surface to test compatibility).
            // Then use WgpuRenderer::new() which creates its own surface
            // from the window handles — it reuses the Metal instance from
            // the pre-populated WgpuContext.
            //
            // `new_with_surface` is private in upstream gpui_wgpu, so we
            // go through the public `WgpuRenderer::new()` path instead.
            let surface_result = metal_instance.create_surface_unsafe(target);
            match surface_result {
                Ok(surface) => match WgpuContext::new(metal_instance, &surface, None) {
                    Ok(context) => {
                        // Pre-populate gpu_context so WgpuRenderer::new()
                        // reuses our Metal-backed context (and its instance)
                        // instead of creating a Vulkan+GL one.
                        let mut gpu_context: Option<WgpuContext> = Some(context);
                        drop(surface); // no longer needed — new() creates its own

                        match WgpuRenderer::new(&mut gpu_context, &ios_window, config, None) {
                            Ok(renderer) => {
                                log::info!("iOS wgpu renderer created (Metal)");
                                *ios_window.renderer.lock() = Some(renderer);
                            }
                            Err(e) => {
                                log::error!("Failed to create iOS wgpu renderer: {e:#}");
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to create iOS WgpuContext: {e:#}");
                    }
                },
                Err(e) => {
                    log::error!("Failed to create iOS wgpu Metal surface: {e:#}");
                }
            }

            Ok(ios_window)
        }
    }

    /// Register this window with the FFI layer after it's been stored.
    /// This must be called after the window is placed at a stable address
    /// (e.g., in a Box or Arc).
    pub(crate) fn register_with_ffi(&self) {
        super::ffi::register_window(self as *const Self);

        // Set the window pointer on the view so touch events can find us
        unsafe {
            let window_ptr = self as *const Self as *mut std::ffi::c_void;
            (*self.view).set_ivar(GPUI_WINDOW_IVAR, window_ptr);
            log::info!(
                "GPUI iOS: Set window pointer {:p} on view {:p}",
                window_ptr,
                self.view
            );
        }
    }

    /// Handle a touch event from UIKit.
    ///
    /// Uses a state machine to distinguish **taps** from **drag gestures**:
    ///
    ///   DOWN  → record start position, enter "pending" (NO MouseDown yet)
    ///   MOVE  → if finger moved > threshold → switch to "scrolling",
    ///           emit `ScrollWheel` deltas (for scrollable containers) AND
    ///           `MouseMove` (for interactive canvas screens like Animations)
    ///   UP    → if still "pending" → emit `MouseDown` + `MouseUp` (tap)
    ///           if "scrolling"   → emit final `ScrollWheel` (Ended) +
    ///           `MouseUp` (so drag-to-throw works)
    ///
    /// MouseDown is **deferred** until finger-up so that starting a scroll
    /// near a button or tab doesn't accidentally trigger navigation.
    /// Interactive screens use `MouseMove` to track the finger during drags
    /// and `MouseUp` to detect the end of a throw/drag gesture.
    pub fn handle_touch(&self, touch: *mut Object, _event: *mut Object) {
        let position = touch_location_in_view(touch, self.view);
        let phase = touch_phase(touch);
        let tap_count = touch_tap_count(touch);
        let modifiers = self.modifiers.get();

        let logical_x: f32 = position.x.into();
        let logical_y: f32 = position.y.into();

        self.mouse_position.set(position);

        let mut ts = self.touch_state.get();

        let mut emit = |input: PlatformInput| {
            if let Some(callback) = self.input_callback.borrow_mut().as_mut() {
                callback(input);
            }
        };

        match phase {
            UITouchPhase::Began => {
                self.touch_pressed.set(true);
                // Cancel any active momentum fling — the user touched the
                // screen again, so inertia scrolling must stop immediately.
                self.momentum_scroller.borrow_mut().cancel();
                self.velocity_tracker.borrow_mut().reset();

                ts = TouchState::Pending {
                    start_x: logical_x,
                    start_y: logical_y,
                };
                // Do NOT emit MouseDown here — wait until we know whether
                // this is a tap or a scroll.  Emitting MouseDown immediately
                // causes accidental navigation when the user starts scrolling
                // near a button/tab.
                //
                // - Tap (finger lifts within slop) → emit MouseDown + MouseUp
                //   together in Ended phase.
                // - Scroll (finger exceeds slop) → emit only MouseMove +
                //   ScrollWheel, no MouseDown.
            }

            UITouchPhase::Moved => {
                // Record every move for velocity estimation.
                self.velocity_tracker
                    .borrow_mut()
                    .record(logical_x, logical_y);

                match ts {
                    TouchState::Pending { start_x, start_y } => {
                        let dx = logical_x - start_x;
                        let dy = logical_y - start_y;
                        let distance = (dx * dx + dy * dy).sqrt();

                        if distance > SCROLL_SLOP {
                            // Promote to scrolling — emit the first scroll
                            // delta from the start position.
                            ts = TouchState::Scrolling {
                                prev_x: logical_x,
                                prev_y: logical_y,
                            };
                            emit(PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                                position,
                                delta: gpui::ScrollDelta::Pixels(gpui::point(
                                    gpui::px(dx),
                                    gpui::px(dy),
                                )),
                                modifiers,
                                touch_phase: gpui::TouchPhase::Started,
                            }));
                        }
                        // Always emit MouseMove so interactive screens can
                        // track finger position (e.g. drag line in Animations,
                        // gradient control in Shaders).
                        emit(PlatformInput::MouseMove(gpui::MouseMoveEvent {
                            position,
                            modifiers,
                            pressed_button: Some(gpui::MouseButton::Left),
                        }));
                    }
                    TouchState::Scrolling { prev_x, prev_y } => {
                        let dx = logical_x - prev_x;
                        let dy = logical_y - prev_y;
                        ts = TouchState::Scrolling {
                            prev_x: logical_x,
                            prev_y: logical_y,
                        };
                        // Scroll event for scrollable containers.
                        emit(PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position,
                            delta: gpui::ScrollDelta::Pixels(gpui::point(
                                gpui::px(dx),
                                gpui::px(dy),
                            )),
                            modifiers,
                            touch_phase: gpui::TouchPhase::Moved,
                        }));
                        // MouseMove for interactive screens.
                        emit(PlatformInput::MouseMove(gpui::MouseMoveEvent {
                            position,
                            modifiers,
                            pressed_button: Some(gpui::MouseButton::Left),
                        }));
                    }
                    TouchState::Idle => {
                        // Spurious move without a preceding down — ignore.
                    }
                }
            }

            UITouchPhase::Ended | UITouchPhase::Cancelled => {
                self.touch_pressed.set(false);
                match ts {
                    TouchState::Pending { start_x, start_y } => {
                        // Finger lifted without exceeding slop → tap.
                        // Emit MouseDown + MouseUp together at the original
                        // down position so hit-testing matches the initial
                        // touch point.
                        self.velocity_tracker.borrow_mut().reset();
                        let tap_pos = gpui::point(gpui::px(start_x), gpui::px(start_y));
                        emit(PlatformInput::MouseDown(gpui::MouseDownEvent {
                            button: gpui::MouseButton::Left,
                            position: tap_pos,
                            modifiers,
                            click_count: tap_count as usize,
                            first_mouse: false,
                        }));
                        emit(PlatformInput::MouseUp(gpui::MouseUpEvent {
                            button: gpui::MouseButton::Left,
                            position: tap_pos,
                            modifiers,
                            click_count: tap_count as usize,
                        }));
                    }
                    TouchState::Scrolling { prev_x, prev_y } => {
                        // End the active touch-scroll gesture.
                        let dx = logical_x - prev_x;
                        let dy = logical_y - prev_y;
                        emit(PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                            position,
                            delta: gpui::ScrollDelta::Pixels(gpui::point(
                                gpui::px(dx),
                                gpui::px(dy),
                            )),
                            modifiers,
                            touch_phase: gpui::TouchPhase::Ended,
                        }));
                        // Also emit MouseUp so interactive screens can
                        // detect the end of a drag (e.g. fling a ball).
                        emit(PlatformInput::MouseUp(gpui::MouseUpEvent {
                            button: gpui::MouseButton::Left,
                            position,
                            modifiers,
                            click_count: 1,
                        }));

                        // ── Start momentum / inertia scrolling ───────────
                        // Compute release velocity from recent touch samples
                        // and kick off the momentum scroller.  Subsequent
                        // frames will pump synthetic ScrollWheel events via
                        // `pump_momentum()` until velocity decays below the
                        // threshold.
                        let (vx, vy) = self.velocity_tracker.borrow().velocity();
                        self.velocity_tracker.borrow_mut().reset();
                        self.momentum_scroller
                            .borrow_mut()
                            .fling(vx, vy, logical_x, logical_y);
                    }
                    TouchState::Idle => {}
                }
                ts = TouchState::Idle;
            }

            UITouchPhase::Stationary => {
                // No change — ignore.
                return;
            }
        }

        self.touch_state.set(ts);
    }

    /// Advance the momentum scroller by one frame and emit a synthetic
    /// `ScrollWheel` event if the fling is still active.
    ///
    /// Called from `gpui_ios_request_frame` on every CADisplayLink tick,
    /// **before** the GPUI render callback runs, so that the scroll delta
    /// is picked up during the current frame's layout/paint cycle.
    pub(crate) fn pump_momentum(&self) {
        let mut scroller = self.momentum_scroller.borrow_mut();
        if !scroller.is_active() {
            return;
        }

        if let Some(delta) = scroller.step() {
            let modifiers = self.modifiers.get();
            let position = gpui::point(gpui::px(delta.position_x), gpui::px(delta.position_y));

            if let Some(callback) = self.input_callback.borrow_mut().as_mut() {
                callback(PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                    position,
                    delta: gpui::ScrollDelta::Pixels(gpui::point(
                        gpui::px(delta.dx),
                        gpui::px(delta.dy),
                    )),
                    modifiers,
                    touch_phase: gpui::TouchPhase::Moved,
                }));
            }
        } else {
            // Fling finished — emit one final Ended event so GPUI knows
            // the scroll gesture is truly complete.
            let pos = self.mouse_position.get();
            let modifiers = self.modifiers.get();
            if let Some(callback) = self.input_callback.borrow_mut().as_mut() {
                callback(PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
                    position: pos,
                    delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(0.0))),
                    modifiers,
                    touch_phase: gpui::TouchPhase::Ended,
                }));
            }
        }
    }

    /// Get the safe area insets
    pub fn safe_area_insets(&self) -> (f32, f32, f32, f32) {
        unsafe {
            // UIEdgeInsets struct
            #[repr(C)]
            struct UIEdgeInsets {
                top: f64,
                left: f64,
                bottom: f64,
                right: f64,
            }

            let insets: UIEdgeInsets = msg_send![self.view, safeAreaInsets];
            (
                insets.top as f32,
                insets.left as f32,
                insets.bottom as f32,
                insets.right as f32,
            )
        }
    }

    /// Show the software keyboard
    pub fn show_keyboard(&self) {
        log::info!("GPUI iOS: Showing keyboard");
        unsafe {
            // Make the text input view become first responder to show keyboard
            let _: BOOL = msg_send![self.text_input_view, becomeFirstResponder];
        }
    }

    /// Hide the software keyboard
    pub fn hide_keyboard(&self) {
        log::info!("GPUI iOS: Hiding keyboard");
        unsafe {
            // Resign first responder to hide keyboard
            let _: BOOL = msg_send![self.text_input_view, resignFirstResponder];
        }
    }

    /// Handle text input from the software keyboard
    pub fn handle_text_input(&self, text: *mut Object) {
        if text.is_null() {
            return;
        }

        unsafe {
            // Convert NSString to Rust String
            let utf8: *const i8 = msg_send![text, UTF8String];
            if utf8.is_null() {
                return;
            }

            let text_str = std::ffi::CStr::from_ptr(utf8)
                .to_string_lossy()
                .into_owned();

            log::info!("GPUI iOS: Text input: {:?}", text_str);

            // First try the input handler (for text fields)
            if let Some(handler) = self.input_handler.borrow_mut().as_mut() {
                handler.replace_text_in_range(None, &text_str);
                return;
            }

            // Otherwise, send as key events
            for c in text_str.chars() {
                let keystroke = gpui::Keystroke {
                    modifiers: Modifiers::default(),
                    key: c.to_string(),
                    key_char: Some(c.to_string()),
                };

                let event = PlatformInput::KeyDown(gpui::KeyDownEvent {
                    keystroke,
                    is_held: false,
                    prefer_character_input: true,
                });

                if let Some(callback) = self.input_callback.borrow_mut().as_mut() {
                    callback(event);
                }
            }
        }
    }

    /// Handle a key event from an external keyboard
    pub fn handle_key_event(&self, key_code: u32, modifier_flags: u32, is_key_down: bool) {
        use super::text_input::{
            key_code_to_key_down, key_code_to_key_up, key_code_to_string,
            modifier_flags_to_modifiers,
        };

        let key = key_code_to_string(key_code);
        let modifiers = modifier_flags_to_modifiers(modifier_flags);

        log::info!(
            "GPUI iOS: Key event - key: {:?}, modifiers: {:?}, down: {}",
            key,
            modifiers,
            is_key_down
        );

        let event = if is_key_down {
            key_code_to_key_down(key_code, modifier_flags)
        } else {
            key_code_to_key_up(key_code, modifier_flags)
        };

        if let Some(callback) = self.input_callback.borrow_mut().as_mut() {
            callback(event);
        }
    }

    /// Notify the window of active status changes (foreground/background).
    ///
    /// This is called by the FFI layer when the app transitions between
    /// foreground and background states.
    pub fn notify_active_status_change(&self, is_active: bool) {
        log::info!("GPUI iOS: Window active status changed to: {}", is_active);

        if let Some(callback) = self.active_status_callback.borrow_mut().as_mut() {
            callback(is_active);
        }
    }
}

impl HasWindowHandle for IosWindow {
    fn window_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
    {
        let view = NonNull::new(self.view as *mut c_void)
            .ok_or(raw_window_handle::HandleError::Unavailable)?;
        let handle = UiKitWindowHandle::new(view);
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(handle.into()) })
    }
}

impl HasDisplayHandle for IosWindow {
    fn display_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
    {
        let handle = UiKitDisplayHandle::new();
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(handle.into()) })
    }
}

impl PlatformWindow for IosWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        self.bounds.get()
    }

    fn is_maximized(&self) -> bool {
        true // iOS windows are always "maximized"
    }

    fn window_bounds(&self) -> WindowBounds {
        WindowBounds::Fullscreen(self.bounds.get())
    }

    fn content_size(&self) -> Size<Pixels> {
        self.bounds.get().size
    }

    fn resize(&mut self, _size: Size<Pixels>) {
        // iOS windows cannot be resized programmatically
    }

    fn scale_factor(&self) -> f32 {
        self.scale_factor.get()
    }

    fn appearance(&self) -> WindowAppearance {
        unsafe {
            let trait_collection: *mut Object = msg_send![self.view, traitCollection];
            let style: i64 = msg_send![trait_collection, userInterfaceStyle];
            match style {
                2 => WindowAppearance::Dark,
                _ => WindowAppearance::Light,
            }
        }
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(Rc::new(IosDisplay::main()))
    }

    fn mouse_position(&self) -> Point<Pixels> {
        self.mouse_position.get()
    }

    fn modifiers(&self) -> Modifiers {
        self.modifiers.get()
    }

    fn capslock(&self) -> Capslock {
        // Would need to check UIKeyModifierFlags
        Capslock { on: false }
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        *self.input_handler.borrow_mut() = Some(input_handler);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.input_handler.borrow_mut().take()
    }

    fn prompt(
        &self,
        _level: PromptLevel,
        msg: &str,
        detail: Option<&str>,
        answers: &[PromptButton],
    ) -> Option<futures::channel::oneshot::Receiver<usize>> {
        // Would use UIAlertController
        let (_tx, rx) = futures::channel::oneshot::channel();

        unsafe {
            // Create UIAlertController
            let title = msg;
            let message = detail.unwrap_or("");

            let alert_style: i64 = 1; // UIAlertControllerStyleAlert

            let title_str: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: title.as_ptr()];
            let message_str: *mut Object =
                msg_send![class!(NSString), stringWithUTF8String: message.as_ptr()];

            let alert: *mut Object = msg_send![
                class!(UIAlertController),
                alertControllerWithTitle: title_str
                message: message_str
                preferredStyle: alert_style
            ];

            // Add buttons
            for (_index, button) in answers.iter().enumerate() {
                let button_title: *mut Object = msg_send![
                    class!(NSString),
                    stringWithUTF8String: button.label().as_str().as_ptr()
                ];

                let action_style: i64 = if button.is_cancel() { 1 } else { 0 }; // UIAlertActionStyleCancel or Default

                // Note: In production, this would need a block that calls tx.send(index)
                let action: *mut Object = msg_send![
                    class!(UIAlertAction),
                    actionWithTitle: button_title
                    style: action_style
                    handler: ptr::null::<Object>()
                ];

                let _: () = msg_send![alert, addAction: action];
            }

            // Present the alert
            let _: () = msg_send![
                self.view_controller,
                presentViewController: alert
                animated: YES
                completion: ptr::null::<Object>()
            ];
        }

        Some(rx)
    }

    fn activate(&self) {
        unsafe {
            let _: () = msg_send![self.window, makeKeyAndVisible];
        }
    }

    fn is_active(&self) -> bool {
        unsafe {
            let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
            let key_window: *mut Object = msg_send![app, keyWindow];
            self.window == key_window
        }
    }

    fn is_hovered(&self) -> bool {
        // Hover isn't really applicable on iOS
        false
    }

    fn set_title(&mut self, _title: &str) {
        // iOS apps don't have window titles
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        WindowBackgroundAppearance::Opaque
    }

    fn set_background_appearance(&self, _background_appearance: WindowBackgroundAppearance) {
        // Could adjust view background color
    }

    fn minimize(&self) {
        // iOS apps cannot be minimized
    }

    fn zoom(&self) {
        // iOS apps cannot be zoomed
    }

    fn toggle_fullscreen(&self) {
        // iOS apps are always fullscreen
    }

    fn is_fullscreen(&self) -> bool {
        true
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        *self.request_frame_callback.borrow_mut() = Some(callback);
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        *self.input_callback.borrow_mut() = Some(callback);
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        *self.active_status_callback.borrow_mut() = Some(callback);
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        *self.hover_status_callback.borrow_mut() = Some(callback);
    }

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        *self.resize_callback.borrow_mut() = Some(callback);
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        *self.moved_callback.borrow_mut() = Some(callback);
    }

    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        *self.should_close_callback.borrow_mut() = Some(callback);
    }

    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        *self.hit_test_callback.borrow_mut() = Some(callback);
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        *self.close_callback.borrow_mut() = Some(callback);
    }

    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        *self.appearance_changed_callback.borrow_mut() = Some(callback);
    }

    fn draw(&self, scene: &Scene) {
        let mut guard = self.renderer.lock();
        if let Some(renderer) = guard.as_mut() {
            renderer.draw(scene);
        } else {
            log::trace!("GPUI iOS: draw called but no renderer available");
        }
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        let guard = self.renderer.lock();
        if let Some(renderer) = guard.as_ref() {
            renderer.sprite_atlas().clone()
        } else {
            // Fallback: return a dummy atlas so GPUI doesn't panic before
            // the renderer is initialised.
            Arc::new(FallbackAtlas::new())
        }
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        let guard = self.renderer.lock();
        guard
            .as_ref()
            .map(|r| r.supports_dual_source_blending())
            .unwrap_or(false)
    }

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        let guard = self.renderer.lock();
        guard.as_ref().map(|r| r.gpu_specs())
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {
        // iOS handles IME positioning automatically
    }
}

// ── Fallback atlas ────────────────────────────────────────────────────────────

/// A minimal fallback `PlatformAtlas` used until a real Blade/Metal renderer is
/// wired up.  It records tiles in memory but does not upload texture data to the
/// GPU — just enough to satisfy GPUI's atlas queries without panicking.
struct FallbackAtlas {
    state: Mutex<FallbackAtlasState>,
}

struct FallbackAtlasState {
    next_id: u32,
    tiles: HashMap<AtlasKey, AtlasTile>,
}

impl FallbackAtlas {
    fn new() -> Self {
        Self {
            state: Mutex::new(FallbackAtlasState {
                next_id: 1,
                tiles: HashMap::new(),
            }),
        }
    }
}

impl PlatformAtlas for FallbackAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: &AtlasKey,
        build: &mut dyn FnMut() -> anyhow::Result<
            Option<(Size<DevicePixels>, std::borrow::Cow<'a, [u8]>)>,
        >,
    ) -> anyhow::Result<Option<AtlasTile>> {
        let mut state = self.state.lock();

        if let Some(tile) = state.tiles.get(key) {
            return Ok(Some(tile.clone()));
        }

        let data = build()?;
        if let Some((size, _pixels)) = data {
            let id = state.next_id;
            state.next_id += 1;

            let tile = AtlasTile {
                texture_id: AtlasTextureId {
                    index: 0,
                    kind: AtlasTextureKind::Monochrome,
                },
                tile_id: TileId(id),
                padding: 0,
                bounds: Bounds {
                    origin: point(DevicePixels(0), DevicePixels(0)),
                    size,
                },
            };

            state.tiles.insert(key.clone(), tile.clone());
            Ok(Some(tile))
        } else {
            Ok(None)
        }
    }

    fn remove(&self, key: &AtlasKey) {
        self.state.lock().tiles.remove(key);
    }
}
