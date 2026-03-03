# GPUI Mobile — Progress & Status

> **Last updated:** 2026-03-02 (GPUI trait impls complete — PlatformDispatcher, PlatformDisplay, PlatformTextSystem, PlatformWindow all wired)

## Project Goal

Bring **wgpu-based Android** (and iOS) platform support to [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), the GPU-accelerated UI framework from [Zed](https://zed.dev). This repository (`gpui-mobile`) provides the mobile platform crates that implement GPUI's `Platform` trait for Android (via wgpu/Vulkan) and iOS (via Blade/Metal).

---

## Architecture

```
gpui-mobile (this repo)
├── .cargo/
│   └── config.toml            # RUST_FONTCONFIG_DLOPEN=on for Android builds
├── src/
│   ├── lib.rs                 # Crate root; re-exports `gpui`; dispatches to ios/android
│   ├── android/
│   │   ├── mod.rs             # Local geometry stubs, AndroidBackend, TouchPoint, AndroidKeyEvent
│   │   ├── platform.rs        # AndroidPlatform + impl Platform ✅ (all todo!() resolved)
│   │   ├── window.rs          # AndroidWindow + AndroidPlatformWindow (impl PlatformWindow) ✅
│   │   ├── renderer.rs        # WgpuRenderer — shaders, draw calls, atlas (wgpu 28 ✅)
│   │   ├── atlas.rs           # WgpuAtlas — texture atlas (etagere, wgpu 28 ✅)
│   │   ├── display.rs         # AndroidDisplay (impl PlatformDisplay) ✅
│   │   ├── dispatcher.rs      # AndroidDispatcher (impl PlatformDispatcher) ✅
│   │   ├── keyboard.rs        # NDK keycode → gpui::Keystroke mapping ✅
│   │   ├── text.rs            # AndroidTextSystem — cosmic-text 0.17 shaping ✅
│   │   ├── jni_entry.rs       # ANativeActivity_onCreate / JNI bootstrap
│   │   └── shaders/           # WGSL shaders for the renderer
│   └── ios/                   # iOS platform (Blade/Metal) — not yet in focus
├── examples/
│   ├── android_app/
│   │   ├── main.rs            # Full GPUI-integrated example (compiles ✅, runtime todo)
│   │   ├── standalone/        # ✅ Minimal wgpu example — RUNS ON DEVICE
│   │   │   ├── Cargo.toml
│   │   │   └── lib.rs
│   │   └── gradle/            # ✅ Android Gradle project for APK packaging
│   │       ├── build.gradle.kts
│   │       ├── settings.gradle.kts
│   │       ├── gradle.properties
│   │       ├── local.properties
│   │       ├── gradle/wrapper/ # gradlew + gradle-wrapper.jar + properties
│   │       └── app/
│   │           ├── build.gradle.kts
│   │           └── src/main/
│   │               ├── AndroidManifest.xml
│   │               └── res/
│   └── ios_app/
│       └── main.rs
├── Cargo.toml                 # Crate manifest (depends on gpui, gpui_wgpu from Zed)
└── PROGRESS.md                # ← You are here
```

---

## What's Done ✅

### 1. Standalone Android Example — Running on Device

A **minimal self-contained Android app** (`examples/android_app/`) that:

- Uses `android-activity` 0.6 + `wgpu` 28 (Vulkan backend)
- Initialises a wgpu surface from `ANativeWindow`
- Renders a cycling-color clear pass at ~60 fps
- Logs to `adb logcat -s gpui-standalone`

**Successfully tested on:**

| Device | GPU | Backend | Resolution |
|--------|-----|---------|------------|
| Motorola (ZD222NCVYR) | Adreno (TM) 720 | Vulkan | 1220×2606 |

**Build & deploy steps (proven working):**

```bash
# 1. Compile the Rust library
cd examples/android_app
ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/29.0.14206865 \
  cargo ndk -t arm64-v8a build

# 2. Copy .so into Gradle project
mkdir -p gradle/app/src/main/jniLibs/arm64-v8a
cp target/aarch64-linux-android/debug/libgpui_android_example.so \
   gradle/app/src/main/jniLibs/arm64-v8a/

# 3. Build APK
cd gradle
./gradlew assembleDebug

# 4. Install & launch
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n com.gpui.mobile.example/android.app.NativeActivity
adb logcat -s gpui-standalone:V
```

### 2. Gradle Project Scaffolding

- Gradle 8.5 wrapper (gradlew + wrapper jar + properties)
- `local.properties` with SDK path
- `app/build.gradle.kts` configured for NativeActivity, API 26+, arm64-v8a
- `AndroidManifest.xml` with Vulkan feature declarations
- Placeholder launcher icons

### 3. Toolchain Verification

All prerequisites confirmed working on the dev machine:

- ✅ `rustup target aarch64-linux-android` installed
- ✅ `cargo-ndk` installed
- ✅ Android NDK 29.0.14206865
- ✅ Android SDK (build-tools 34–36)
- ✅ Java 17 (OpenJDK Homebrew)
- ✅ `adb` device connectivity

### 4. GPUI Integration — Compiles for Android ✅

The main `gpui-mobile` crate now **compiles cleanly** for `aarch64-linux-android`:

```bash
# This succeeds with only cosmetic warnings:
ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/29.0.14206865 \
  cargo ndk -t arm64-v8a build --example android_app
```

What was fixed:

- **wgpu 28 API drift** (renderer.rs, atlas.rs) — all renamed types/methods updated
- **cosmic-text 0.17 API changes** (text.rs) — references, extra args, weight param
- **`impl Platform for AndroidPlatform`** (platform.rs) — full trait implementation with stubs for desktop-only methods
- **`PlatformKeyboardMapper`** — `AndroidKeyboardMapper` stub implemented
- **Local geometry stubs** (mod.rs) — `Pixels`, `DevicePixels`, `Size`, `Point`, `Bounds` with public inner fields (gpui's `Pixels` has `pub(crate)` field)
- **fontconfig cross-compile fix** — `.cargo/config.toml` sets `RUST_FONTCONFIG_DLOPEN=on` automatically
- **`rustc-hash`** added as Android dependency for `FxBuildHasher` in keyboard mapper

Key design decisions:

- **Local geometry stubs** — `mod.rs` defines `Pixels(pub f32)`, `DevicePixels(pub i32)`, `Size`, `Point`, `Bounds` with public inner fields because upstream `gpui::Pixels` has `pub(crate)` visibility on its inner field; conversion to/from real GPUI types happens at the platform boundary
- **`unsafe` transmute for callbacks** — `on_quit`, `on_reopen`, `on_open_urls`, `on_keyboard_layout_change` use `transmute` to add `Send` bound since Android is single-threaded on the main thread; this should be audited
- **`todo!()` stubs** — All `todo!()` calls resolved in Session 3 (see below)

---

## What Needs Fixing 🔧

### All Compilation Errors — FIXED ✅

The following categories of errors were resolved (details kept for reference):

#### Category 1: wgpu 28 API Drift — FIXED ✅

| Old (wgpu 22–24) | New (wgpu 28) | Files |
|---|---|---|
| `wgpu::Maintain::Wait` | `wgpu::PollType::Wait { .. }` | renderer.rs |
| `wgpu::ImageCopyTexture` | `wgpu::TexelCopyTextureInfo` | atlas.rs |
| `wgpu::ImageDataLayout` | `wgpu::TexelCopyBufferLayout` | atlas.rs |
| `RenderPassColorAttachment` (3 fields) | + `depth_slice: None` field | renderer.rs ×3 |
| `request_adapter` returns `Option` | Returns `Result` | renderer.rs |
| `request_device(desc, trace)` | `request_device(desc)` + `experimental_features` + `trace` fields | renderer.rs |
| `push_constant_ranges` | `immediate_size` | renderer.rs |
| `multiview` | `multiview_mask` | renderer.rs |
| `entry_point: "name"` | `entry_point: Some("name")` | renderer.rs ×2 |
| `Instance::new(desc)` | `Instance::new(&desc)` | renderer.rs |

#### Category 2: cosmic-text 0.17 API — FIXED ✅

| Change | Fix |
|---|---|
| `AttrsList::new(Attrs::new())` | `AttrsList::new(&Attrs::new())` |
| `add_span(range, attrs)` | `add_span(range, &attrs)` |
| `get_font(id)` (1 arg) | `get_font(id, weight)` (2 args) |
| `layout_to_buffer` (7 args) | Added `match_mono_width: None` + `hinting: Hinting::default()` |

#### Category 3: fontconfig Cross-Compile — FIXED ✅

`.cargo/config.toml` now sets `RUST_FONTCONFIG_DLOPEN=on` automatically for all builds.

#### Category 4: `impl Platform for AndroidPlatform` — FIXED ✅

Full `impl Platform for AndroidPlatform` block added with ~50 trait methods:
- **Wired methods:** `run`, `quit`, `activate`, `on_quit`, `on_reopen`, `on_open_urls`, `on_keyboard_layout_change`, `read_from_clipboard`, `write_to_clipboard`, `write_credentials`, `read_credentials`, `delete_credentials`, `keyboard_layout`, `keyboard_mapper`, `app_path`, `should_auto_hide_scrollbars`, `can_select_mixed_files_and_dirs`, `window_appearance`, `thermal_state`
- **No-op stubs (desktop-only):** `hide`, `hide_other_apps`, `unhide_other_apps`, `set_menus`, `set_dock_menu`, `on_app_menu_action`, `on_will_open_app_menu`, `on_validate_app_menu_command`, `set_cursor_style`, `restart`, `reveal_path`
- **`todo!()` stubs (need deeper work):** `background_executor`, `foreground_executor`, `text_system`, `open_window`, `displays`, `primary_display`, `active_window`

### 5. GPUI Trait Implementations — All Wired ✅

All `todo!()` stubs have been replaced with real implementations:

#### `PlatformDispatcher` for `AndroidDispatcher` ✅

- `is_main_thread()` — delegates to `ALooper_forThread` comparison
- `dispatch()` — routes to the thread-pool (ignores priority for now)
- `dispatch_on_main_thread()` — enqueues on the ALooper main queue via wake pipe
- `dispatch_after()` — delayed task queue, flushed on each `tick()`
- `spawn_realtime()` — spawns a dedicated thread (for audio)
- `get_all_timings()` / `get_current_thread_timings()` — return empty (profiling not yet implemented)
- Enables: `BackgroundExecutor::new(dispatcher)` and `ForegroundExecutor::new(dispatcher)`

#### `PlatformTextSystem` via `gpui_wgpu::CosmicTextSystem` ✅

- Replaced the local `AndroidTextSystem` (custom port) with `gpui_wgpu::CosmicTextSystem` directly in `AndroidPlatformState`
- `CosmicTextSystem` already implements `PlatformTextSystem` in the upstream Zed crate
- Constructed with `"sans-serif"` fallback; scans `/system/fonts/` automatically on Android
- The local `text.rs` `AndroidTextSystem` is still available for standalone use but is no longer used by the platform layer

#### `PlatformDisplay` for `AndroidDisplay` ✅

- `id()` → `DisplayId::new(self.id as u32)`
- `uuid()` → deterministic v5 UUID from the display pointer
- `bounds()` → logical bounds in `gpui::Pixels` (physical size ÷ scale factor)
- Added `uuid = { version = "1", features = ["v5"] }` to Android dependencies

#### `PlatformWindow` via `AndroidPlatformWindow` ✅

- New wrapper struct `AndroidPlatformWindow` wrapping `Arc<AndroidWindow>`
- Implements `HasWindowHandle` (returns `AndroidNdkWindowHandle`)
- Implements `HasDisplayHandle` (returns `AndroidDisplayHandle`)
- Implements all ~40 `PlatformWindow` trait methods:
  - **Geometry:** `bounds()`, `content_size()`, `scale_factor()`, `window_bounds()` (always Fullscreen)
  - **State:** `is_maximized()` (true), `is_fullscreen()` (true), `is_active()`, `is_hovered()`
  - **Callbacks:** `on_request_frame`, `on_resize`, `on_close` bridged to AndroidWindow with `Send` transmute
  - **Input:** `set_input_handler`, `take_input_handler`, `modifiers`, `capslock`
  - **Rendering:** `draw()` (logs + no-op pending Scene bridge), `sprite_atlas()` (simple atlas), `gpu_specs()`
  - **No-ops:** `minimize`, `zoom`, `toggle_fullscreen`, `set_title`, desktop-only methods
- Includes `AndroidSimpleAtlas` implementing `PlatformAtlas` as a placeholder

#### `impl Platform` Wiring ✅

All `todo!()` calls in `impl Platform for AndroidPlatform` are resolved:

| Method | Before | After |
|---|---|---|
| `background_executor()` | `todo!()` | `BackgroundExecutor::new(dispatcher.clone())` |
| `foreground_executor()` | `todo!()` | `ForegroundExecutor::new(dispatcher.clone())` |
| `text_system()` | `todo!()` | `state.text_system.clone()` (CosmicTextSystem) |
| `displays()` | empty vec | maps `DisplayList` → `Vec<Rc<dyn PlatformDisplay>>` |
| `primary_display()` | `None` | maps primary `AndroidDisplay` → `Rc<dyn PlatformDisplay>` |
| `open_window()` | `bail!()` | descriptive error (ANativeActivity lifecycle) |
| `active_window()` | `None` | `None` with explanation |

---

## Next Steps (Prioritised)

### Immediate: End-to-End GPUI on Device ← YOU ARE HERE

1. **Wire `gpui::Application::new()`** — construct GPUI Application with `AndroidPlatform` in the example
2. **Bridge `gpui::Scene` → local renderer** — connect `AndroidPlatformWindow::draw()` to `WgpuRenderer::draw()`
3. **Render a simple GPUI view** (colored rectangle, text label) on device
4. **Bridge touch/key events** — translate AndroidWindow callbacks → `PlatformInput` in `on_input`

### Short-Term: Input & Lifecycle Polish

5. Wire `open_window` to ANativeActivity lifecycle (APP_CMD_INIT_WINDOW → AndroidPlatformWindow)
6. Verify touch input → GPUI mouse events
7. Verify keyboard input → GPUI keystrokes
8. Wire `active_window()` to return `AnyWindowHandle` from the platform-managed window

### Medium-Term: CI & Quality

9. Add CI jobs (cross-compile check for aarch64-linux-android)
10. Add a one-step build script (`cargo ndk` → APK → install)
11. Audit `unsafe` code (transmute for Send, raw pointer handling) and document safety invariants
12. Test on multiple devices (Vulkan-capable, GL-only fallback)
13. Replace `AndroidSimpleAtlas` with the real `WgpuAtlas` from the renderer
14. iOS platform parity

---

## Environment & Dependencies

| Dependency | Version | Notes |
|---|---|---|
| Rust | stable | `aarch64-linux-android` target |
| cargo-ndk | latest | Cross-compile helper |
| Android NDK | 29.0.14206865 | In `~/Library/Android/sdk/ndk/` |
| Android SDK | API 26+ (build-tools 36) | |
| Java | 17 (OpenJDK) | For Gradle |
| Gradle | 8.5 | Via wrapper in `examples/android_app/gradle/` |
| gpui | git (zed-industries/zed) | Core UI framework |
| gpui_wgpu | git (zed-industries/zed) | wgpu renderer + cosmic-text |
| wgpu | 28.0 | GPU abstraction |
| cosmic-text | 0.17.2 | Text shaping |
| android-activity | 0.6 | NativeActivity glue (standalone example) |

---

## Key Files Quick Reference

| File | Purpose |
|---|---|
| `Cargo.toml` | Crate manifest, git deps on gpui/gpui_wgpu |
| `.cargo/config.toml` | Sets `RUST_FONTCONFIG_DLOPEN=on` for Android |
| `src/android/mod.rs` | Android module root, local geometry stubs |
| `src/android/platform.rs` | `AndroidPlatform` + `impl Platform` ✅ (all todo!() resolved) |
| `src/android/renderer.rs` | wgpu 28 renderer ✅ |
| `src/android/atlas.rs` | Texture atlas ✅ |
| `src/android/text.rs` | AndroidTextSystem (standalone port) ✅ |
| `src/android/keyboard.rs` | Key mapping ✅ |
| `src/android/dispatcher.rs` | AndroidDispatcher + `impl PlatformDispatcher` ✅ |
| `src/android/display.rs` | AndroidDisplay + `impl PlatformDisplay` ✅ |
| `src/android/window.rs` | AndroidWindow + AndroidPlatformWindow (`impl PlatformWindow`) ✅ |
| `examples/android_app/` | ✅ Minimal working example (on device) |
| `examples/android_app/gradle/` | ✅ Gradle APK packaging (working) |

---

## Resuming Work

To pick up where you left off:

```bash
# 1. Verify device is connected
adb devices

# 2. Build the full gpui-mobile crate (should compile cleanly, 0 errors)
cd /path/to/gpui
ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/29.0.14206865 \
  cargo ndk -t arm64-v8a build --example android_app

# 3. Run the example on device (proven working)
cd examples/android_app
ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/29.0.14206865 \
  cargo ndk -t arm64-v8a build
cp target/aarch64-linux-android/debug/libgpui_android_example.so \
   gradle/app/src/main/jniLibs/arm64-v8a/
cd gradle && ./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n com.gpui.mobile.example/android.app.NativeActivity
adb logcat -s gpui-standalone:V

# 4. Next: wire gpui::Application::new() and render a GPUI view on device.
```

---

## Session Log

Chronological record of work sessions for context when resuming.

### Session 1 — Initial scaffolding (prior)

- Created `gpui-mobile` crate with Android module structure
- Wrote `AndroidPlatform`, `AndroidWindow`, `WgpuRenderer`, `AndroidAtlas`, `AndroidDisplay`, `AndroidDispatcher`, `AndroidTextSystem`, `keyboard.rs`, `jni_entry.rs`
- Set up `Cargo.toml` with git dependencies on `gpui` and `gpui_wgpu` from the Zed monorepo
- Wrote examples (`android_app/main.rs`, `ios_app/main.rs`)
- Code was written against wgpu ~22–24 and cosmic-text ~0.12; compiling revealed 54 errors

### Session 2 — Compilation fixes + standalone example (2026-03-02)

1. Explored project structure, verified prerequisites (`cargo-ndk`, NDK 29, adb device ZD222NCVYR)
2. Created standalone example (`examples/android_app/`) using `android-activity` 0.6 + `wgpu` 28
3. Set up Gradle wrapper (8.5), `local.properties`, placeholder icons, fixed `settings.gradle.kts`
4. Built standalone → APK → **installed and running on Motorola** (Adreno 720, Vulkan, 1220×2606, Rgba8UnormSrgb)
5. Fixed all 54 compilation errors in `gpui-mobile`:
   - `renderer.rs`: wgpu 28 renames (`PollType`, `TexelCopyTextureInfo`, `depth_slice`, `immediate_size`, `multiview_mask`, `entry_point: Option`, reference `InstanceDescriptor`, `DeviceDescriptor` fields, `request_device` 1-arg, `request_adapter` `Result`)
   - `atlas.rs`: `TexelCopyTextureInfo`, `TexelCopyBufferLayout`
   - `text.rs`: cosmic-text 0.17 (`&Attrs`, `get_font(id, weight)`, `layout_to_buffer` + `Hinting`)
   - `mod.rs`: added local geometry stubs (`Pixels`, `DevicePixels`, `Size`, `Point`, `Bounds`)
   - `platform.rs`: added full `impl Platform for AndroidPlatform` (~50 methods) + `AndroidKeyboardMapper`
6. Created `.cargo/config.toml` with `RUST_FONTCONFIG_DLOPEN=on`
7. Added `rustc-hash` dependency for `FxBuildHasher`
8. Created this `PROGRESS.md`
9. **Result:** `cargo ndk -t arm64-v8a build --example android_app` succeeds with 0 errors, 2 warnings

### Session 3 — GPUI Trait Implementations (2026-03-02)

Implemented all four GPUI platform trait impls and wired them into `AndroidPlatform`:

1. **`impl PlatformDispatcher for AndroidDispatcher`** (`dispatcher.rs`)
   - Added imports: `gpui::{PlatformDispatcher, Priority, RunnableVariant, ThreadTaskTimings}`
   - Implemented all 7 required methods: `get_all_timings`, `get_current_thread_timings`, `is_main_thread`, `dispatch`, `dispatch_on_main_thread`, `dispatch_after`, `spawn_realtime`
   - Background tasks route to the existing thread-pool; main-thread tasks use the ALooper wake pipe
   - Realtime tasks spawn a dedicated thread (matching Linux dispatcher behaviour)

2. **`impl PlatformDisplay for AndroidDisplay`** (`display.rs`)
   - Added imports: `gpui::{DisplayId, PlatformDisplay}`
   - Implemented `id()` (truncates u64 → u32 via `DisplayId::new`), `uuid()` (v5 UUID from display pointer), `bounds()` (logical pixels)
   - Added `uuid = "1"` with `v5` feature to Android dependencies in `Cargo.toml`

3. **Switched `text_system` to `gpui_wgpu::CosmicTextSystem`** (`platform.rs`)
   - Replaced `Arc<AndroidTextSystem>` with `Arc<CosmicTextSystem>` in `AndroidPlatformState`
   - `CosmicTextSystem` already implements `PlatformTextSystem` upstream
   - `text_system()` now returns `self.state.lock().text_system.clone()` directly

4. **`impl PlatformWindow` via `AndroidPlatformWindow`** (`window.rs`)
   - New struct `AndroidPlatformWindow` wrapping `Arc<AndroidWindow>`
   - Implements `HasWindowHandle` (AndroidNdk), `HasDisplayHandle` (Android)
   - Implements all ~40 `PlatformWindow` methods with appropriate Android semantics
   - Callbacks (`on_request_frame`, `on_resize`, `on_close`) use `unsafe transmute` to add `Send` bound (main-thread-only invariant)
   - Includes `AndroidSimpleAtlas` implementing `PlatformAtlas` as a placeholder
   - Exported from `mod.rs`

5. **Wired `impl Platform for AndroidPlatform`** (`platform.rs`)
   - `background_executor()` → `BackgroundExecutor::new(dispatcher.clone())`
   - `foreground_executor()` → `ForegroundExecutor::new(dispatcher.clone())`
   - `text_system()` → `state.text_system.clone()` (CosmicTextSystem)
   - `displays()` → maps `DisplayList` → `Vec<Rc<dyn PlatformDisplay>>`
   - `primary_display()` → maps primary → `Rc<dyn PlatformDisplay>`
   - Improved error messages for `open_window`, `open_url`, `open_with_system`

6. **Result:** `cargo ndk -t arm64-v8a check` and `cargo ndk -t arm64-v8a check --example android_app` both succeed with 0 errors, only 1 cosmetic warning (unused `iter_mut` in atlas.rs)
