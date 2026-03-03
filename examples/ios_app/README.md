# iOS Example App — GPUI Mobile

A complete example showing how to build an iOS application using the `gpui-mobile` crate with Metal rendering, UIKit integration, and touch input handling.

## Architecture

```
Swift (Xcode)                          Rust (gpui-mobile)
─────────────                          ──────────────────────────────
AppDelegate                            gpui_mobile::ios
  │                                      │
  ├─ didFinishLaunching ──────────────►  gpui_ios_initialize()
  │                                      gpui_ios_did_finish_launching()
  │                                        └─ creates IosPlatform + IosWindow
  │                                           (UIWindow + CAMetalLayer view)
  │
  ├─ CADisplayLink tick ──────────────►  gpui_ios_request_frame(window_ptr)
  │                                        └─ invokes request_frame_callback
  │                                           └─ Metal draw call
  │
  ├─ touchesBegan/Moved/Ended ────────►  gpui_ios_handle_touch(window, touch, event)
  │                                        └─ converts UITouch → GPUI MouseDown/Move/Up
  │
  ├─ pressesBegan/Ended ──────────────►  gpui_ios_handle_key_event(window, code, mods, down)
  │                                        └─ converts HID key code → GPUI Keystroke
  │
  ├─ willEnterForeground ─────────────►  gpui_ios_will_enter_foreground()
  ├─ didBecomeActive ─────────────────►  gpui_ios_did_become_active()
  ├─ willResignActive ────────────────►  gpui_ios_will_resign_active()
  ├─ didEnterBackground ──────────────►  gpui_ios_did_enter_background()
  └─ willTerminate ───────────────────►  gpui_ios_will_terminate()
```

## Prerequisites

- macOS with Xcode 15+ installed
- Rust toolchain (stable)
- iOS targets:
  ```bash
  rustup target add aarch64-apple-ios           # Device
  rustup target add aarch64-apple-ios-sim       # Simulator (Apple Silicon)
  ```

## Building the Rust Library

```bash
# From the repository root (gpui/)

# Build for device (ARM64)
cargo build --example ios_app --target aarch64-apple-ios --release

# Build for simulator (Apple Silicon Mac)
cargo build --example ios_app --target aarch64-apple-ios-sim --release

# The output is a static library at:
#   target/aarch64-apple-ios/release/examples/libios_app.a
#   target/aarch64-apple-ios-sim/release/examples/libios_app.a
```

## Xcode Project Setup

### Option A: Use the provided scaffolding

The `xcode/` directory contains the Swift and configuration files you need:

| File | Purpose |
|------|---------|
| `AppDelegate.swift` | UIKit app delegate that drives the Rust library |
| `BridgingHeader.h` | C-ABI function declarations for Swift interop |
| `Info.plist` | iOS app metadata and capabilities |

1. Create a new Xcode project (iOS → App, Swift, Storyboard)
2. Replace the generated `AppDelegate.swift` with `xcode/AppDelegate.swift`
3. Add `xcode/BridgingHeader.h` to the project
4. Set **Build Settings → Objective-C Bridging Header** to the header path
5. Add the compiled `libios_app.a` to **Build Phases → Link Binary With Libraries**
6. Also link these system frameworks:
   - `Metal.framework`
   - `MetalKit.framework`
   - `QuartzCore.framework`
   - `UIKit.framework`
   - `CoreGraphics.framework`
   - `CoreText.framework`
   - `CoreFoundation.framework`
7. Build and run on device or simulator

### Option B: Use the demo launcher

For the quickest path to seeing something on screen, skip the full project setup and use the built-in demo launcher:

```swift
// In your AppDelegate.swift:
func application(_ app: UIApplication, didFinishLaunchingWithOptions opts: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
    gpui_ios_run_demo()  // Launches the Animation Playground + Shader Showcase
    return true
}
```

## Project Structure

```
examples/ios_app/
├── main.rs                 # Rust entry point (example binary)
├── README.md               # This file
└── xcode/
    ├── AppDelegate.swift   # Swift UIKit app delegate
    ├── BridgingHeader.h    # C function declarations for Swift
    └── Info.plist          # iOS app configuration
```

## C-ABI Functions

The following `extern "C"` functions are exported by `gpui-mobile` and are available to call from Swift or Objective-C:

### Lifecycle

| Function | When to call |
|----------|-------------|
| `gpui_ios_initialize()` | Once, in `didFinishLaunchingWithOptions:`, before everything else |
| `gpui_ios_did_finish_launching(app_ptr)` | Immediately after `initialize()` |
| `gpui_ios_will_enter_foreground(app_ptr)` | `applicationWillEnterForeground:` |
| `gpui_ios_did_become_active(app_ptr)` | `applicationDidBecomeActive:` |
| `gpui_ios_will_resign_active(app_ptr)` | `applicationWillResignActive:` |
| `gpui_ios_did_enter_background(app_ptr)` | `applicationDidEnterBackground:` |
| `gpui_ios_will_terminate(app_ptr)` | `applicationWillTerminate:` |

### Window & Rendering

| Function | Purpose |
|----------|---------|
| `gpui_ios_get_window()` | Returns the current `IosWindow` pointer (or NULL) |
| `gpui_ios_request_frame(window_ptr)` | Drive one render frame (call on every CADisplayLink tick) |

### Input

| Function | Purpose |
|----------|---------|
| `gpui_ios_handle_touch(window, touch, event)` | Forward a `UITouch` event |
| `gpui_ios_handle_key_event(window, code, mods, down)` | Forward a hardware keyboard event |
| `gpui_ios_handle_text_input(window, nsstring)` | Forward soft-keyboard text input |
| `gpui_ios_show_keyboard(window)` | Show the on-screen keyboard |
| `gpui_ios_hide_keyboard(window)` | Hide the on-screen keyboard |

### Demo

| Function | Purpose |
|----------|---------|
| `gpui_ios_run_demo()` | Launch the built-in demo (Animation Playground + Shader Showcase) |

## Built-in Demos

The `gpui_mobile::ios::demos` module includes two interactive demos:

### Animation Playground
- Tap to spawn particle bursts
- Drag and release to launch bouncing balls with physics (gravity, friction, wall bounce)
- Balls leave color trails; oldest balls are evicted when the cap is reached

### Shader Showcase
- Dynamic rotating gradient background whose hue cycles over time
- Eight translucent orbs with parallax offset driven by touch position
- Tap to spawn expanding ripple rings that fade out

## Troubleshooting

### `ld: library not found for -lSystem`
Install Xcode Command Line Tools:
```bash
xcode-select --install
```

### `Undefined symbols for architecture arm64`
Make sure you've:
1. Built the Rust library for the correct target (`aarch64-apple-ios` for device, `aarch64-apple-ios-sim` for simulator)
2. Added the `.a` file to **Link Binary With Libraries** in Xcode
3. Linked all required system frameworks (Metal, UIKit, etc.)

### `gpui_ios_get_window()` returns NULL
The window is created inside the finish-launching callback. Make sure you call `gpui_ios_did_finish_launching()` after `gpui_ios_initialize()` and before calling `gpui_ios_get_window()`.

### No rendering output
- Verify the `CADisplayLink` is running and calling `gpui_ios_request_frame()` every tick
- Check Xcode console for log output prefixed with `GPUI iOS FFI:`
- Ensure the `UIWindow` is made key and visible

## License

MIT OR Apache-2.0