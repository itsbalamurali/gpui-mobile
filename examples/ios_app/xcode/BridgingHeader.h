//
//  BridgingHeader.h
//  GPUIMobileExample
//
//  Bridging header that declares the C-ABI functions exported by the
//  gpui-mobile Rust static library.
//
//  Usage:
//    In your Xcode project's Build Settings, set
//      "Objective-C Bridging Header" → "BridgingHeader.h"
//    so that Swift can see these declarations.
//
//  All functions below correspond to `#[unsafe(no_mangle)] pub extern "C"`
//  symbols defined in `gpui_mobile::ios::ffi` and the example crate.
//

#ifndef GPUI_MOBILE_BRIDGING_HEADER_H
#define GPUI_MOBILE_BRIDGING_HEADER_H

#include <stdint.h>
#include <stdbool.h>

// ── Platform lifecycle ──────────────────────────────────────────────────────

/// Initialise the GPUI iOS runtime.
///
/// Call once from `application:didFinishLaunchingWithOptions:` before any
/// other `gpui_ios_*` function.
///
/// Returns a non-null sentinel on success, NULL if already initialised.
void *gpui_ios_initialize(void);

/// Invoke the finish-launching callback registered by `IosPlatform::run()`.
///
/// Call from `application:didFinishLaunchingWithOptions:` after
/// `gpui_ios_initialize()` returns.
///
/// @param app_ptr  Reserved for future use; pass NULL.
void gpui_ios_did_finish_launching(void *app_ptr);

/// Notify GPUI that the app is about to enter the foreground.
///
/// Call from `applicationWillEnterForeground:`.
void gpui_ios_will_enter_foreground(void *app_ptr);

/// Notify GPUI that the app has become active (foregrounded).
///
/// Call from `applicationDidBecomeActive:`.
void gpui_ios_did_become_active(void *app_ptr);

/// Notify GPUI that the app is about to resign active status.
///
/// Call from `applicationWillResignActive:`.
void gpui_ios_will_resign_active(void *app_ptr);

/// Notify GPUI that the app has entered the background.
///
/// Call from `applicationDidEnterBackground:`.
void gpui_ios_did_enter_background(void *app_ptr);

/// Notify GPUI that the app is about to terminate.
///
/// Call from `applicationWillTerminate:`.
void gpui_ios_will_terminate(void *app_ptr);

// ── Window management ───────────────────────────────────────────────────────

/// Return a pointer to the most recently created `IosWindow`, or NULL.
///
/// The returned pointer is opaque; pass it to `gpui_ios_request_frame()`,
/// `gpui_ios_handle_touch()`, etc.
void *gpui_ios_get_window(void);

// ── Rendering ───────────────────────────────────────────────────────────────

/// Drive one rendering frame for the given window.
///
/// Call on every `CADisplayLink` tick.
///
/// @param window_ptr  The value returned by `gpui_ios_get_window()`.
void gpui_ios_request_frame(void *window_ptr);

// ── Input events ────────────────────────────────────────────────────────────

/// Forward a UIKit touch event to the given window.
///
/// @param window_ptr  The value returned by `gpui_ios_get_window()`.
/// @param touch_ptr   A `UITouch *` cast to `void *`.
/// @param event_ptr   A `UIEvent *` cast to `void *`.
void gpui_ios_handle_touch(void *window_ptr, void *touch_ptr, void *event_ptr);

/// Deliver a hardware-keyboard key event to the given window.
///
/// @param window_ptr   The value returned by `gpui_ios_get_window()`.
/// @param key_code     `UIKeyboardHIDUsage` value (USB HID usage page 0x07).
/// @param modifiers    `UIKeyModifierFlags` bitmask.
/// @param is_key_down  `true` for key-down, `false` for key-up.
void gpui_ios_handle_key_event(void *window_ptr,
                               uint32_t key_code,
                               uint32_t modifiers,
                               bool is_key_down);

/// Deliver soft-keyboard text input to the given window.
///
/// @param window_ptr  The value returned by `gpui_ios_get_window()`.
/// @param text_ptr    An `NSString *` cast to `void *`.
void gpui_ios_handle_text_input(void *window_ptr, void *text_ptr);

// ── Keyboard visibility ─────────────────────────────────────────────────────

/// Show the on-screen (software) keyboard for the given window.
void gpui_ios_show_keyboard(void *window_ptr);

/// Hide the on-screen (software) keyboard for the given window.
void gpui_ios_hide_keyboard(void *window_ptr);

// ── Demo launcher ───────────────────────────────────────────────────────────

/// Launch the built-in interactive demo (Animation Playground + Shader Showcase).
///
/// This is a self-contained alternative to the
/// `gpui_ios_initialize` / `gpui_ios_did_finish_launching` pair.
/// It creates a GPUI window and starts the demo menu automatically.
void gpui_ios_run_demo(void);

// ── Example-specific entry points ───────────────────────────────────────────

/// Start the example application from Objective-C / Swift.
///
/// Equivalent to calling `main()` in the Rust example binary.
void gpui_example_ios_start(void);

/// Launch the interactive demo from Objective-C / Swift.
void gpui_example_ios_run_demo(void);

#endif /* GPUI_MOBILE_BRIDGING_HEADER_H */