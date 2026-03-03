# GPUI Mobile — Cross-Platform Mobile UI Framework

A high-performance mobile platform layer for [GPUI](https://github.com/zed-industries/zed), enabling Rust-based UI applications to run natively on iOS and Android.

## Overview

`gpui-mobile` implements the **real `gpui::Platform` trait** from the [Zed](https://github.com/zed-industries/zed) repository for mobile targets. It depends on:

- **[`gpui`](https://github.com/zed-industries/zed/tree/main/crates/gpui)** — Core types (`Platform`, `PlatformWindow`, `PlatformDisplay`, `Pixels`, `Keystroke`, `Modifiers`, event types, text system traits, etc.)
- **[`gpui_wgpu`](https://github.com/zed-industries/zed/tree/main/crates/gpui_wgpu)** (Android) — wgpu-based renderer + cosmic-text system (same crate used by `gpui_linux`)

This follows the same architecture as [`gpui_linux`](https://github.com/zed-industries/zed/tree/main/crates/gpui_linux) — a separate crate that implements `gpui::Platform` for a specific OS.

| Platform | Renderer | Windowing | Text | Dispatcher |
|----------|----------|-----------|------|------------|
| **iOS** | Metal via [Blade](https://github.com/kvark/blade) | UIKit (`UIWindow` + `CAMetalLayer`) | CoreText (shared with macOS) | Grand Central Dispatch |
| **Android** | Vulkan / GL via [wgpu](https://wgpu.rs/) | NDK (`ANativeWindow`) | [cosmic-text](https://github.com/pop-os/cosmic-text) + [swash](https://github.com/dfrg/swash) via `gpui_wgpu` | `ALooper` + thread pool |

## Project Structure

```
gpui/
├── Cargo.toml                  # Single-crate manifest (gpui-mobile)
│                               #   depends on gpui + gpui_wgpu from Zed repo
├── src/
│   ├── lib.rs                  # Crate root — re-exports gpui, platform dispatch
│   ├── ios/                    # iOS platform (mirrors zed PR #43655)
│   │   ├── mod.rs              # Module root + CG↔GPUI geometry helpers
│   │   ├── platform.rs         # IosPlatform (impl gpui::Platform)
│   │   ├── window.rs           # IosWindow (impl gpui::PlatformWindow)
│   │   ├── display.rs          # IosDisplay (impl gpui::PlatformDisplay)
│   │   ├── dispatcher.rs       # IosDispatcher (impl gpui::PlatformDispatcher)
│   │   ├── events.rs           # UITouch → gpui::PlatformInput translation
│   │   ├── ffi.rs              # C-ABI bridge for ObjC app delegates
│   │   ├── text_input.rs       # HID key-code → gpui::Keystroke mapping
│   │   └── demos/              # Interactive demos (Animation, Shaders)
│   └── android/                # Android platform (mirrors gpui_linux)
│       ├── mod.rs              # Module root — uses real gpui types
│       ├── platform.rs         # AndroidPlatform (impl gpui::Platform)
│       ├── window.rs           # AndroidWindow (ANativeWindow + wgpu)
│       ├── renderer.rs         # WgpuContext + WgpuRenderer (via gpui_wgpu)
│       ├── display.rs          # AndroidDisplay
│       ├── dispatcher.rs       # AndroidDispatcher (ALooper)
│       ├── keyboard.rs         # Android NDK keycodes → gpui::Keystroke
│       │                       #   + AndroidKeyboardLayout (impl PlatformKeyboardLayout)
│       ├── atlas.rs            # GPU texture atlas (etagere)
│       ├── text.rs             # AndroidTextSystem (cosmic-text via gpui_wgpu)
│       ├── jni_entry.rs        # JNI_OnLoad + ANativeActivity_onCreate
│       └── shaders/            # WGSL shader sources
│           ├── shaders.wgsl
│           └── shaders_subpixel.wgsl
└── examples/
    ├── ios_app/                # Complete iOS example application
    │   ├── main.rs             # Rust entry point
    │   ├── README.md           # iOS build & integration guide
    │   └── xcode/              # Xcode project scaffolding
    │       ├── AppDelegate.swift
    │       ├── BridgingHeader.h
    │       └── Info.plist
    └── android_app/            # Complete Android example application
        ├── main.rs             # Rust entry point (gpui_android_main)
        ├── README.md           # Android build & integration guide
        └── gradle/             # Gradle project scaffolding
            ├── build.gradle.kts
            ├── settings.gradle.kts
            └── app/
                ├── build.gradle.kts
                ├── proguard-rules.pro
                └── src/main/
                    ├── AndroidManifest.xml
                    └── res/
```

## Quick Start

### iOS

```bash
# Add the iOS target
rustup target add aarch64-apple-ios

# Build the example
cargo build --example ios_app --target aarch64-apple-ios --release

# Link the output static library into an Xcode project
# See examples/ios_app/README.md for full instructions
```

### Android

```bash
# Add the Android target
rustup target add aarch64-linux-android

# Install cargo-ndk
cargo install cargo-ndk

# Build the example
cargo ndk -t arm64-v8a build --example android_app --release

# Package into an APK using the provided Gradle project
cd examples/android_app/gradle
./gradlew assembleDebug
adb install app/build/outputs/apk/debug/app-debug.apk
```

### Host (documentation / CI)

```bash
# Compiles the non-mobile fallback — pulls in gpui + gpui_wgpu from Zed repo
cargo check

# Run tests (keyboard module tests etc. are target-gated)
cargo test
```

## Usage

### Dependency

```toml
[dependencies]
gpui-mobile = { git = "https://github.com/pandranki/gpui" }

# The crate transitively brings in:
#   gpui      — from zed-industries/zed (Platform trait, event types, geometry)
#   gpui_wgpu — from zed-industries/zed (wgpu renderer, cosmic-text, on Android)
```

### iOS

The iOS platform implements `gpui::Platform` using UIKit + Metal/Blade.
It is driven by a Swift/ObjC app delegate that calls into C-ABI functions exported by the crate:

```swift
// In your AppDelegate.swift:
func application(_ app: UIApplication, didFinishLaunchingWithOptions opts: ...) -> Bool {
    gpui_ios_initialize()
    gpui_ios_did_finish_launching(nil)

    let window = gpui_ios_get_window()
    // Set up CADisplayLink to call gpui_ios_request_frame(window) every tick
    // Forward touches via gpui_ios_handle_touch(window, touch, event)
    // Keyboard events use gpui::Keystroke via gpui_ios_handle_key_event()
    return true
}
```

Or use the built-in demo launcher for a quick start:

```swift
func application(_ app: UIApplication, didFinishLaunchingWithOptions opts: ...) -> Bool {
    gpui_ios_run_demo()  // Animation Playground + Shader Showcase
    return true
}
```

From Rust — note that `current_platform()` returns `Rc<dyn gpui::Platform>`:

```rust
use gpui_mobile::ios::current_platform;

// Returns Rc<dyn gpui::Platform> backed by IosPlatform
let platform = current_platform(false);
platform.run(Box::new(|| {
    println!("App launched on iOS!");
}));
```

### Android

The Android platform implements `gpui::Platform` using NDK + wgpu/Vulkan.
It is driven by `NativeActivity`. Define the `gpui_android_main` entry point:

```rust
#[no_mangle]
pub extern "C" fn gpui_android_main(activity: *mut std::ffi::c_void) {
    use gpui_mobile::android::{self, current_platform};
    use gpui_mobile::android::keyboard::*;

    android::init_logger();

    // Returns Rc<dyn gpui::Platform> backed by AndroidPlatform
    let platform = current_platform(false);

    // Keyboard events from hardware/Bluetooth keyboards are converted
    // through the keyboard module: Android NDK keycodes → gpui::Keystroke
    // with proper gpui::Modifiers (Ctrl, Alt, Shift, Meta, Function).

    // Main event loop
    loop {
        if android::jni_entry::poll_events(16) {
            break; // quit requested
        }
    }
}
```

Then declare the `NativeActivity` in `AndroidManifest.xml`:

```xml
<activity android:name="android.app.NativeActivity">
    <meta-data android:name="android.app.lib_name" android:value="your_lib_name" />
    <intent-filter>
        <action android:name="android.intent.action.MAIN" />
        <category android:name="android.intent.category.LAUNCHER" />
    </intent-filter>
</activity>
```

### Keyboard Handling (Android)

The `android::keyboard` module provides full key mapping from Android NDK to GPUI,
mirroring the approach used by `gpui_linux::keyboard`:

```rust
use gpui_mobile::android::keyboard::*;

// Convert Android NDK key event → gpui::Keystroke
let keystroke = android_key_to_keystroke(
    AKEYCODE_A,       // key code
    AMETA_CTRL_ON,    // meta state (modifiers)
    0,                // unicode char (0 = derive from key)
);
// → Keystroke { key: "a", modifiers: { control: true, ... }, key_char: Some("a") }

// Software keyboard text → gpui::Keystroke
let keystroke = character_to_keystroke('é');
// → Keystroke { key: "é", modifiers: default, key_char: Some("é") }

// Keyboard layout (implements gpui::PlatformKeyboardLayout)
let layout = AndroidKeyboardLayout::new("en-US");
assert_eq!(layout.id(), "en-US");
```

## Platform Support

| Platform | Status | Min Version | GPU Backend |
|----------|--------|-------------|-------------|
| iOS (device) | ✅ | iOS 13.0+ | Metal |
| iOS (simulator) | ✅ | iOS 13.0+ | Metal (simulated) |
| Android (arm64) | ✅ | API 26+ | Vulkan (preferred), GL ES 3.0 (fallback) |
| Android (armv7) | ⚠️ Untested | API 26+ | Vulkan / GL ES |
| Android (x86_64) | ⚠️ Emulator | API 26+ | Vulkan / GL ES |
| Host (macOS/Linux) | 🔧 Check only | — | — |

## Features

| Feature | Description |
|---------|-------------|
| `font-kit` | Enables `font-kit` based font matching on iOS (CoreText text system) |
| `ios` | (marker) iOS-specific code |
| `android` | (marker) Android-specific code |

## Key Dependencies

| Crate | Source | Used For |
|-------|--------|----------|
| `gpui` | [zed-industries/zed](https://github.com/zed-industries/zed) | `Platform` trait, event types, geometry types, text system traits |
| `gpui_wgpu` | [zed-industries/zed](https://github.com/zed-industries/zed) | wgpu renderer, `CosmicTextSystem`, `WgpuAtlas` (Android) |
| `wgpu` 28.x | crates.io | Vulkan/GL backend (Android) |
| `blade-graphics` | [kvark/blade](https://github.com/kvark/blade) | Metal renderer (iOS) |
| `cosmic-text` 0.17 | crates.io | Text shaping (Android, via gpui_wgpu) |
| `core-text` 21 | crates.io | Text shaping (iOS, CoreText framework) |

## Architecture

### iOS (mirrors [Zed PR #43655](https://github.com/zed-industries/zed/pull/43655))

```
IosPlatform (impl gpui::Platform)
  ├── IosDispatcher         — GCD main queue + global background queue
  │                           (impl gpui::PlatformDispatcher)
  ├── IosWindow             — UIWindow + CAMetalLayer + Blade/Metal renderer
  │     │                     (impl gpui::PlatformWindow)
  │     └── render loop     — driven by CADisplayLink via FFI callbacks
  ├── IosDisplay            — UIScreen wrapper (impl gpui::PlatformDisplay)
  ├── events                — UITouch → gpui::PlatformInput translation
  ├── text_input            — HID key codes → gpui::Keystroke mapping
  ├── text_system           — CoreText (impl gpui::PlatformTextSystem, shared with macOS)
  └── demos                 — Animation Playground + Shader Showcase
```

### Android (mirrors [gpui_linux](https://github.com/zed-industries/zed/tree/main/crates/gpui_linux))

```
AndroidPlatform (impl gpui::Platform)
  ├── AndroidDispatcher     — ALooper (foreground) + thread pool (background)
  ├── AndroidWindow         — ANativeWindow + WgpuRenderer
  │     └── WgpuRenderer    — wgpu device/queue/swapchain/pipelines (from gpui_wgpu)
  │           └── AndroidAtlas  — etagere-backed GPU texture atlas
  ├── AndroidDisplay        — ANativeWindow geometry + AConfiguration density
  ├── AndroidTextSystem     — cosmic-text shaping + swash rasterisation (via gpui_wgpu)
  ├── keyboard              — Android NDK keycodes → gpui::Keystroke + gpui::Modifiers
  │                           + AndroidKeyboardLayout (impl gpui::PlatformKeyboardLayout)
  └── jni_entry             — JNI_OnLoad + ANativeActivity lifecycle callbacks
                              dispatches AKeyEvent → keyboard module → gpui::PlatformInput
```

## Building from Source

### Prerequisites

- Rust 1.75+
- For iOS: macOS with Xcode 15+
- For Android: Android NDK r25+ and `cargo-ndk`

### Compile checks

```bash
# Host check — pulls gpui + gpui_wgpu from the Zed repo
cargo check

# Android cross-check
cargo check --target aarch64-linux-android

# Run host-side tests (keyboard tests run only on Android target)
cargo test

# Verify no warnings
cargo clippy
```

### Note on first build

The first `cargo check` will clone the Zed repository to fetch the `gpui` and
`gpui_wgpu` crates. This is a large repo (~1GB) so the initial build may take
a few minutes. Subsequent builds use the cached checkout.

### Cross-compilation

```bash
# iOS device
cargo build --target aarch64-apple-ios --release

# iOS simulator (Apple Silicon)
cargo build --target aarch64-apple-ios-sim --release

# Android ARM64
cargo ndk -t arm64-v8a build --release

# Android ARMv7
cargo ndk -t armeabi-v7a build --release
```

## Examples

| Example | Description | Target |
|---------|-------------|--------|
| [`ios_app`](examples/ios_app/) | Complete iOS app with Swift AppDelegate, touch input, keyboard handling, and demo launcher | `aarch64-apple-ios` |
| [`android_app`](examples/android_app/) | Complete Android app with NativeActivity, keyboard integration, Gradle project, and event loop | `aarch64-linux-android` |

Each example includes a detailed README with step-by-step build and integration instructions.
Both examples use the **real `gpui` types** (`gpui::Platform`, `gpui::Keystroke`, etc.) — not local stubs.

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Ensure `cargo check --target aarch64-linux-android` passes
4. Ensure `cargo check` (host) passes
5. Run `cargo fmt --all` and `cargo clippy`
6. Open a Pull Request

## License

Licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE) (http://www.apache.org/licenses/LICENSE-2.0)
- [MIT license](LICENSE-MIT) (http://opensource.org/licenses/MIT)

at your option.
