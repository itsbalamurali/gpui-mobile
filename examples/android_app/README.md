# Android Example App — GPUI Mobile

A multi-screen GPUI application with navigation, interactive touch input, and
theming — rendered via `gpui_wgpu` (wgpu/Vulkan) and hosted inside a
`NativeActivity`.

## Quick Start

```bash
# 1. Prerequisites (one-time)
rustup target add aarch64-linux-android
cargo install cargo-ndk
# Ensure ANDROID_NDK_HOME is set or Android Studio NDK is installed

# 2. Build the native .so
cd examples/android_app
cargo ndk -t arm64-v8a -o gradle/app/src/main/jniLibs build

# 3. Build the APK
cd gradle
./gradlew assembleDebug

# 4. Install and run on a connected device
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n com.gpui.mobile.example/android.app.NativeActivity

# 5. Watch logs
adb logcat -s gpui-android-example:V
```

## Architecture

### Entry Point — `android-activity` crate

This example uses the [`android-activity`](https://crates.io/crates/android-activity)
crate (with the `native-activity` feature) instead of hand-rolled JNI/NDK glue.
The crate exports `ANativeActivity_onCreate` automatically and calls the
user-defined `android_main(app: AndroidApp)` function on a dedicated native thread.

```
Android OS loads libgpui_android_example.so via NativeActivity
  │
  └─ ANativeActivity_onCreate()           (android-activity crate)
       └─ android_main(app)               (main.rs — YOUR CODE)
            ├─ init logger + panic hook
            ├─ jni_entry::init_platform(&app)
            │    └─ Creates AndroidPlatform + stores global state
            ├─ platform.set_on_init_window(callback)
            │    └─ Deferred — runs when native surface is ready
            └─ jni_entry::run_event_loop(&app)
                 │
                 ├─ poll_events (16ms timeout, ~60 fps)
                 │    └─ MainEvent::InitWindow
                 │         └─ Creates wgpu/Vulkan surface + AndroidWindow
                 │
                                  ├─ Deferred on_init_window callback (runs once)
                                  │    ├─ shared_platform() → SharedPlatform (Rc wrapper)
                                  │    ├─ Application::with_platform(shared).run(|cx| { ... })
                                  │    │    └─ cx.open_window(..., |_, cx| cx.new(Router::new))
                                  │    │         ├─ Finds the existing primary window
                                  │    │         └─ Router renders active screen + tab bar
                                  │    └─ GPUI view system is now wired up
                 │
                 ├─ flush_main_thread_tasks()
                 ├─ window.request_frame()
                 │    └─ Fires GPUI on_request_frame → layout → paint → draw
                 │
                 └─ (repeats until Destroy / quit)
```

### Window Lifecycle

On Android, windows are **not** created by calling `cx.open_window(...)` directly.
The system delivers a `MainEvent::InitWindow` lifecycle event when the native
surface is ready.  The example registers a callback via
`platform.set_on_init_window(...)` which fires once the `AndroidWindow` exists.
Inside that callback, `Application::with_platform(...).run(...)` is called, and
`cx.open_window(...)` finds the already-created primary window and wires it into
GPUI's view system.

### Rendering

The renderer is `gpui_wgpu::WgpuRenderer` from the Zed repository — the same
renderer used on Linux and web.  It natively consumes `gpui::Scene`, so there is
no type bridging or conversion layer between GPUI's paint output and the GPU
pipeline.  Vulkan is the preferred backend; OpenGL ES is the fallback.

### Frame Driving

GPUI's rendering is driven by `window.request_frame()` called on every event loop
iteration.  This invokes the `on_request_frame` callback that GPUI registers
during `cx.open_window(...)`, which triggers the layout → paint → draw cycle.

**Important implementation detail**: The `request_frame` callback is taken out of
the window's `Mutex` before invocation and put back afterwards.  This avoids a
deadlock: the callback runs layout → paint → `PlatformWindow::draw` →
`AndroidWindow::draw`, which needs the same lock to access the renderer.

## The Example App

The app uses a `Router` view as its root, which owns navigation state and
delegates rendering to the currently active screen.  All screens share state
(counter value, dark mode flag, user name) via the `Router` struct.

### Screens

| Screen | File | Description |
|---|---|---|
| **Home** | `src/screens/home.rs` | Welcome message, colour swatches, stats card, quick-nav cards |
| **Counter** | `src/screens/counter.rs` | Increment / decrement / reset a shared tap counter with milestone tracking |
| **Settings** | `src/screens/settings.rs` | Toggle dark mode, reset counter, change user name, theme preview |
| **About** | `src/screens/about.rs` | App info, technology stack, architecture details, credits |

### Navigation

- **Bottom tab bar** — four tabs (🏠 Home, 🔢 Counter, ⚙ Settings, ℹ About)
  with an active indicator highlight.
- **Back button** — appears in the top nav bar when there is navigation history;
  pops the previous screen from the stack.
- **Navigation history** — a `Vec<Screen>` stack on the `Router`; `navigate_to()`
  pushes, `go_back()` pops.

### Theming

The app supports dark mode (Catppuccin Mocha) and light mode (Catppuccin Latte),
toggled from the Settings screen.  The theme propagates to all screens because
the `Router` owns the `dark_mode` flag and each screen reads it at render time.

```rust
Application::with_platform(shared.into_rc()).run(|cx: &mut App| {
    cx.open_window(
        WindowOptions { window_bounds: None, ..Default::default() },
        |_, cx| cx.new(|_| Router::new()),
    ).unwrap();
    cx.activate(true);
});
```

Touch events are translated to `MouseDown`/`MouseUp`/`MouseMove` events by
`AndroidPlatformWindow::on_input`, so standard GPUI mouse handlers like
`on_mouse_down` work for touch input.

## Project Structure

```
examples/android_app/
├── Cargo.toml                       # crate-type = ["cdylib"], depends on gpui-mobile + gpui
├── .cargo/
│   └── config.toml                  # RUST_FONTCONFIG_DLOPEN=on
├── README.md                        # This file
│
├── src/                             # Rust source code
│   ├── lib.rs                       #   android_main entry point + GPUI Application setup
│   └── screens/                     #   All navigable screens
│       ├── mod.rs                   #     Screen enum, Router view, nav bar, tab bar
│       ├── home.rs                  #     Home screen (welcome, swatches, stats, nav cards)
│       ├── counter.rs               #     Counter screen (increment, decrement, milestones)
│       ├── settings.rs              #     Settings screen (dark mode, reset, profile)
│       └── about.rs                 #     About screen (tech stack, architecture, credits)
│
└── gradle/                          # Android Gradle project (packages .so → APK)
    ├── build.gradle.kts             #   Root build script + cargo-ndk convenience tasks
    ├── settings.gradle.kts
    ├── gradle.properties
    ├── local.properties             #   sdk.dir=... (auto-generated)
    ├── gradlew
    └── app/
        ├── build.gradle.kts         #   compileSdk=34, minSdk=26, nativeLibraryName
        ├── proguard-rules.pro
        └── src/main/
            ├── AndroidManifest.xml  #   NativeActivity + lib_name placeholder
            ├── res/                 #   Icons + strings
            └── jniLibs/
                └── arm64-v8a/       #   ← .so copied here by cargo-ndk -o
```

## Detailed Build Steps

### Prerequisites

| Requirement | Version | Check |
|---|---|---|
| Rust (stable) | latest | `rustc --version` |
| Android target | aarch64-linux-android | `rustup target list --installed` |
| cargo-ndk | latest | `cargo ndk --version` |
| Android NDK | r27+ | `ls $HOME/Library/Android/sdk/ndk/` |
| Android SDK | API 26+ | `ls $HOME/Library/Android/sdk/platforms/` |
| Java | 17+ | `java -version` |

Install missing prerequisites:

```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
# NDK: install via Android Studio → SDK Manager → SDK Tools → NDK
```

### Step 1: Build the Rust cdylib

From the `examples/android_app/` directory:

```bash
# Debug build (large .so, fast compile)
cargo ndk -t arm64-v8a -o gradle/app/src/main/jniLibs build

# Release build (small .so, slower compile)
cargo ndk -t arm64-v8a -o gradle/app/src/main/jniLibs build --release
```

The `-o` flag tells `cargo-ndk` to copy the output `.so` directly into the
Gradle `jniLibs` directory — no manual copy step needed.

### Step 2: Build the APK

```bash
cd gradle
./gradlew assembleDebug
```

Output: `gradle/app/build/outputs/apk/debug/app-debug.apk`

### Step 3: Install and Run

```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n com.gpui.mobile.example/android.app.NativeActivity
```

### One-liner (build + package + install)

```bash
cd examples/android_app \
  && cargo ndk -t arm64-v8a -o gradle/app/src/main/jniLibs build \
  && cd gradle \
  && ./gradlew assembleDebug \
  && adb install -r app/build/outputs/apk/debug/app-debug.apk \
  && adb shell am start -n com.gpui.mobile.example/android.app.NativeActivity
```

## Key Files Reference

| File | Purpose |
|---|---|
| `Cargo.toml` | cdylib output; depends on `gpui-mobile`, `gpui`, `android-activity` |
| `src/lib.rs` | `android_main` entry point; links `gpui-mobile` via `extern crate`, opens window with `Router` |
| `src/screens/mod.rs` | `Screen` enum, `Router` view (nav bar, tab bar, screen dispatch) |
| `src/screens/home.rs` | Home screen layout (welcome, swatches, stats, nav cards) |
| `src/screens/counter.rs` | Counter screen with interactive buttons and milestones |
| `src/screens/settings.rs` | Settings screen with toggles and actions |
| `src/screens/about.rs` | About screen with tech stack and credits |
| `.cargo/config.toml` | `RUST_FONTCONFIG_DLOPEN=on` (required for cross-compile) |
| `gradle/app/build.gradle.kts` | `nativeLibraryName = "gpui_android_example"` |
| `gradle/app/src/main/AndroidManifest.xml` | NativeActivity, `lib_name` placeholder |

## Configuration Gotchas

### `RUST_FONTCONFIG_DLOPEN`

Must be set to `"on"` or the `yeslogic-fontconfig-sys` crate will try to link
fontconfig at build time, which fails on Android.  Configured in
`.cargo/config.toml`.

### `ANDROID_NDK_HOME`

`cargo-ndk` needs to find the NDK.  Either:
- Set `ANDROID_NDK_HOME` env var, or
- Let it auto-detect from `$HOME/Library/Android/sdk/ndk/<latest>/`

### Library name must match

The chain of names must be consistent:

1. `Cargo.toml` → `[lib] name = "gpui_android_example"` → produces `libgpui_android_example.so`
2. `build.gradle.kts` → `manifestPlaceholders["nativeLibraryName"] = "gpui_android_example"`
3. `AndroidManifest.xml` → `android:value="${nativeLibraryName}"` (resolved at build time)

If any of these mismatch, the app crashes with "ANativeActivity_onCreate not found".

### `async-task` patch

Both `gpui/Cargo.toml` and `examples/android_app/Cargo.toml` must have the same
`[patch.crates-io]` for `async-task` — the Zed workspace uses a forked version.

## Troubleshooting

### Black screen / "isn't responding"

1. **No frame driving**: The event loop must call `window.request_frame()` every
   iteration.  Without it, GPUI's paint cycle never runs.
2. **Deadlock**: If `request_frame()` holds the window lock while invoking the
   callback, and the callback calls `draw()` (which needs the same lock), the
   app freezes.  The callback must be taken out of the lock first.
3. **Scene type mismatch**: If using a local renderer with different primitive
   types than `gpui::Scene`, the bytemuck cast silently drops all primitives.
   Use `gpui_wgpu::WgpuRenderer` which natively consumes `gpui::Scene`.

### `ANativeActivity_onCreate` not found

The `.so` doesn't export the symbol.  This crate uses `android-activity` which
provides it automatically.  Make sure you're building the cdylib target:

```bash
cargo ndk -t arm64-v8a build   # from examples/android_app/
```

Verify: `nm -D target/aarch64-linux-android/debug/libgpui_android_example.so | grep ANativeActivity`

### Font panic (`.SystemUIFont` not found)

GPUI tries to resolve system fonts that don't exist on Android.  The platform
loads Roboto and other system fonts from `/system/fonts/` during
`AndroidPlatform::new()`.  If no fonts are found, check that the device has
`/system/fonts/Roboto-Regular.ttf`.

### App crashes on rotation

The manifest includes `android:configChanges="orientation|screenSize|..."` to
prevent the activity from being recreated.  Verify the manifest wasn't changed.

### APK is huge (300+ MB)

That's a debug build.  Use `--release` for a ~20 MB `.so`:

```bash
cargo ndk -t arm64-v8a -o gradle/app/src/main/jniLibs build --release
```

### Touch not working

Touch events are translated to GPUI mouse events in
`AndroidPlatformWindow::on_input`.  Use standard GPUI handlers:
- `on_mouse_down(MouseButton::Left, ...)` for tap
- `cx.listener(|this, event, window, cx| { ... })` for stateful handlers

### Adding a new screen

1. Create `src/screens/my_screen.rs` with a `pub fn render(router: &Router) -> impl IntoElement`.
2. Add `pub mod my_screen;` to `src/screens/mod.rs`.
3. Add a variant to the `Screen` enum and a `title()` match arm.
4. Add a tab in `render_tab_bar()` and a match arm in `render_current_screen()`.
5. Add a `render_my_screen()` helper that delegates to `my_screen::render(self)`.

## Supported ABIs

| ABI | Target Triple | Status |
|---|---|---|
| arm64-v8a | `aarch64-linux-android` | ✅ Primary / tested |
| armeabi-v7a | `armv7-linux-androideabi` | ⚠️ Untested |
| x86_64 | `x86_64-linux-android` | ⚠️ Emulator only |
| x86 | `i686-linux-android` | ⚠️ Emulator only |

## License

MIT OR Apache-2.0