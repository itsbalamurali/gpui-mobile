//! iOS event handling — converting UIKit touch events to GPUI's input model.
//!
//! iOS is a touch-first platform, so we map UIKit gestures onto the subset of
//! GPUI `PlatformInput` variants that the rest of the framework already knows
//! how to handle:
//!
//! | UIKit gesture          | GPUI event                          |
//! |------------------------|-------------------------------------|
//! | `touchesBegan`         | `MouseDown` (left button)           |
//! | `touchesMoved`         | `MouseMove` (button = Left)         |
//! | `touchesEnded`         | `MouseUp`   (left button)           |
//! | `touchesCancelled`     | `MouseUp`   (left button)           |
//! | Force-touch / long-press | `MouseDown` (right button)        |
//! | Pan gesture delta      | `ScrollWheel`                       |
//!
//! External (hardware) keyboard events are translated in `text_input.rs`.

use core_graphics::geometry::CGPoint;
use objc::{msg_send, runtime::Object, sel, sel_impl};

// ---------------------------------------------------------------------------
// Re-exported pixel / point type
// ---------------------------------------------------------------------------

use super::Pixels;

/// A 2-D point with typed coordinates.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

impl<T> Point<T> {
    #[inline]
    pub fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

// ---------------------------------------------------------------------------
// Modifiers
// ---------------------------------------------------------------------------

/// Keyboard-modifier state.
///
/// Most of these are irrelevant for finger touches; they become meaningful
/// when an external (Bluetooth / USB-C) keyboard is connected.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    /// The ⌘ Command / Windows key (called "platform" for cross-platform parity).
    pub platform: bool,
    pub function: bool,
}

// ---------------------------------------------------------------------------
// Mouse button
// ---------------------------------------------------------------------------

/// The logical mouse button associated with a touch or pointer event.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Navigate(NavigateDirection),
}

/// Forward / backward navigation buttons (thumb buttons on a mouse).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum NavigateDirection {
    Back,
    Forward,
}

// ---------------------------------------------------------------------------
// Touch phase
// ---------------------------------------------------------------------------

/// Phase of a `UITouch`, matching `UITouchPhase` integer values from UIKit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum UITouchPhase {
    Began = 0,
    Moved = 1,
    Stationary = 2,
    Ended = 3,
    Cancelled = 4,
}

impl From<i64> for UITouchPhase {
    fn from(raw: i64) -> Self {
        match raw {
            0 => UITouchPhase::Began,
            1 => UITouchPhase::Moved,
            2 => UITouchPhase::Stationary,
            3 => UITouchPhase::Ended,
            _ => UITouchPhase::Cancelled,
        }
    }
}

/// Phase of a continuous gesture (scroll, pinch …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
}

impl From<UITouchPhase> for TouchPhase {
    fn from(p: UITouchPhase) -> Self {
        match p {
            UITouchPhase::Began => TouchPhase::Started,
            UITouchPhase::Moved | UITouchPhase::Stationary => TouchPhase::Moved,
            UITouchPhase::Ended | UITouchPhase::Cancelled => TouchPhase::Ended,
        }
    }
}

// ---------------------------------------------------------------------------
// GPUI-style input event types
// ---------------------------------------------------------------------------

/// A mouse / touch *down* event.
#[derive(Clone, Debug)]
pub struct MouseDownEvent {
    pub button: MouseButton,
    pub position: Point<Pixels>,
    pub modifiers: Modifiers,
    /// 1 = single tap, 2 = double tap, …
    pub click_count: usize,
    /// `true` when this is the first event after the app became active.
    pub first_mouse: bool,
}

/// A mouse / touch *up* event.
#[derive(Clone, Debug)]
pub struct MouseUpEvent {
    pub button: MouseButton,
    pub position: Point<Pixels>,
    pub modifiers: Modifiers,
    pub click_count: usize,
}

/// A pointer (finger) *move* event.
#[derive(Clone, Debug)]
pub struct MouseMoveEvent {
    pub position: Point<Pixels>,
    pub modifiers: Modifiers,
    /// Which button is held while moving (`None` if no button is pressed).
    pub pressed_button: Option<MouseButton>,
}

/// A scroll-wheel (or pan-gesture) event.
#[derive(Clone, Debug)]
pub struct ScrollWheelEvent {
    pub position: Point<Pixels>,
    pub delta: ScrollDelta,
    pub modifiers: Modifiers,
    pub touch_phase: TouchPhase,
}

/// The scroll distance, either in logical pixels or in discrete lines.
#[derive(Clone, Debug)]
pub enum ScrollDelta {
    Pixels(Point<Pixels>),
    Lines(Point<f32>),
}

/// The top-level discriminated union of all platform input events.
#[derive(Clone, Debug)]
pub enum PlatformInput {
    MouseDown(MouseDownEvent),
    MouseUp(MouseUpEvent),
    MouseMove(MouseMoveEvent),
    ScrollWheel(ScrollWheelEvent),
}

// ---------------------------------------------------------------------------
// UITouch helpers
// ---------------------------------------------------------------------------

/// Returns the location of `touch` inside `view` as a `Point<Pixels>`.
///
/// # Safety
/// Both `touch` and `view` must be valid, live Objective-C object pointers.
pub fn touch_location_in_view(touch: *mut Object, view: *mut Object) -> Point<Pixels> {
    let location: CGPoint = unsafe { msg_send![touch, locationInView: view] };
    Point::new(Pixels(location.x as f32), Pixels(location.y as f32))
}

/// Returns the `UITouchPhase` of `touch`.
///
/// # Safety
/// `touch` must be a valid, live `UITouch` pointer.
pub fn touch_phase(touch: *mut Object) -> UITouchPhase {
    let raw: i64 = unsafe { msg_send![touch, phase] };
    UITouchPhase::from(raw)
}

/// Returns the tap count of `touch` (1 for a single tap, 2 for double-tap, …).
///
/// # Safety
/// `touch` must be a valid, live `UITouch` pointer.
pub fn touch_tap_count(touch: *mut Object) -> u32 {
    let count: i64 = unsafe { msg_send![touch, tapCount] };
    count as u32
}

/// Returns `true` when a 3D-Touch / Haptic-Touch force threshold is exceeded,
/// which is used to synthesise a right-click (context-menu) event.
///
/// Falls back to `false` on devices that do not support force input.
///
/// # Safety
/// `touch` must be a valid, live `UITouch` pointer.
pub fn is_force_touch(touch: *mut Object) -> bool {
    unsafe {
        let force: f64 = msg_send![touch, force];
        let max_force: f64 = msg_send![touch, maximumPossibleForce];
        max_force > 0.0 && (force / max_force) > 0.5
    }
}

// ---------------------------------------------------------------------------
// PlatformInput constructors
// ---------------------------------------------------------------------------

/// Builds a `MouseDown(Left)` from a `touchesBegan` event.
pub fn touch_began_to_mouse_down(
    position: Point<Pixels>,
    tap_count: u32,
    modifiers: Modifiers,
) -> PlatformInput {
    PlatformInput::MouseDown(MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers,
        click_count: tap_count as usize,
        first_mouse: false,
    })
}

/// Builds a `MouseUp(Left)` from a `touchesEnded` / `touchesCancelled` event.
pub fn touch_ended_to_mouse_up(
    position: Point<Pixels>,
    tap_count: u32,
    modifiers: Modifiers,
) -> PlatformInput {
    PlatformInput::MouseUp(MouseUpEvent {
        button: MouseButton::Left,
        position,
        modifiers,
        click_count: tap_count as usize,
    })
}

/// Builds a `MouseMove` from a `touchesMoved` event.
pub fn touch_moved_to_mouse_move(
    position: Point<Pixels>,
    modifiers: Modifiers,
    pressed_button: Option<MouseButton>,
) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent {
        position,
        modifiers,
        pressed_button,
    })
}

/// Builds a `MouseDown(Right)` from a force-touch / long-press event.
///
/// This is the iOS equivalent of a right-click for context menus.
pub fn force_touch_to_right_click(position: Point<Pixels>, modifiers: Modifiers) -> PlatformInput {
    PlatformInput::MouseDown(MouseDownEvent {
        button: MouseButton::Right,
        position,
        modifiers,
        click_count: 1,
        first_mouse: false,
    })
}

/// Builds a `ScrollWheel` from a pan-gesture delta.
///
/// `delta_x` and `delta_y` are the translation in **logical pixels** since the
/// last event; positive Y means scrolling down (content moves up).
pub fn pan_gesture_to_scroll(
    position: Point<Pixels>,
    delta_x: f32,
    delta_y: f32,
    modifiers: Modifiers,
    touch_phase: TouchPhase,
) -> PlatformInput {
    PlatformInput::ScrollWheel(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(Point::new(Pixels(delta_x), Pixels(delta_y))),
        modifiers,
        touch_phase,
    })
}

// ---------------------------------------------------------------------------
// Modifier helper
// ---------------------------------------------------------------------------

/// Returns the current (empty) modifiers for a pure touch event.
///
/// iOS 13.4+ can supply modifier flags from an external keyboard via
/// `UIKeyModifierFlags`; that mapping lives in `text_input.rs`.
/// For finger-only touch events we always return the default (all false).
#[inline]
pub fn touch_modifiers() -> Modifiers {
    Modifiers::default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_phase_from_raw() {
        assert_eq!(UITouchPhase::from(0), UITouchPhase::Began);
        assert_eq!(UITouchPhase::from(1), UITouchPhase::Moved);
        assert_eq!(UITouchPhase::from(2), UITouchPhase::Stationary);
        assert_eq!(UITouchPhase::from(3), UITouchPhase::Ended);
        assert_eq!(UITouchPhase::from(4), UITouchPhase::Cancelled);
        // Unknown values map to Cancelled
        assert_eq!(UITouchPhase::from(99), UITouchPhase::Cancelled);
    }

    #[test]
    fn touch_phase_to_gpui_phase() {
        assert_eq!(TouchPhase::from(UITouchPhase::Began), TouchPhase::Started);
        assert_eq!(TouchPhase::from(UITouchPhase::Moved), TouchPhase::Moved);
        assert_eq!(
            TouchPhase::from(UITouchPhase::Stationary),
            TouchPhase::Moved
        );
        assert_eq!(TouchPhase::from(UITouchPhase::Ended), TouchPhase::Ended);
        assert_eq!(TouchPhase::from(UITouchPhase::Cancelled), TouchPhase::Ended);
    }

    #[test]
    fn mouse_down_event_fields() {
        let pos = Point::new(Pixels(10.0), Pixels(20.0));
        let ev = touch_began_to_mouse_down(pos, 2, Modifiers::default());
        match ev {
            PlatformInput::MouseDown(e) => {
                assert_eq!(e.button, MouseButton::Left);
                assert_eq!(e.click_count, 2);
                assert_eq!(e.position.x, Pixels(10.0));
            }
            _ => panic!("expected MouseDown"),
        }
    }

    #[test]
    fn mouse_up_event_fields() {
        let pos = Point::new(Pixels(5.0), Pixels(15.0));
        let ev = touch_ended_to_mouse_up(pos, 1, Modifiers::default());
        match ev {
            PlatformInput::MouseUp(e) => {
                assert_eq!(e.button, MouseButton::Left);
                assert_eq!(e.click_count, 1);
            }
            _ => panic!("expected MouseUp"),
        }
    }

    #[test]
    fn scroll_wheel_pixels_delta() {
        let pos = Point::new(Pixels(0.0), Pixels(0.0));
        let ev = pan_gesture_to_scroll(pos, -30.0, 15.0, Modifiers::default(), TouchPhase::Moved);
        match ev {
            PlatformInput::ScrollWheel(e) => match e.delta {
                ScrollDelta::Pixels(d) => {
                    assert_eq!(d.x, Pixels(-30.0));
                    assert_eq!(d.y, Pixels(15.0));
                }
                _ => panic!("expected pixel delta"),
            },
            _ => panic!("expected ScrollWheel"),
        }
    }

    #[test]
    fn force_touch_is_right_button() {
        let pos = Point::new(Pixels(1.0), Pixels(2.0));
        let ev = force_touch_to_right_click(pos, Modifiers::default());
        match ev {
            PlatformInput::MouseDown(e) => assert_eq!(e.button, MouseButton::Right),
            _ => panic!("expected MouseDown(Right)"),
        }
    }
}
