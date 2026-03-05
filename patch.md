# Android Patches for gpui_wgpu and Window Lifecycle

## Problem

Three interconnected bugs caused the GPUI Android app to freeze or crash when returning from the background:

1. **Atlas panic**: Creating a new `WgpuRenderer` on resume gave it an empty `WgpuAtlas`, but GPUI's scene cache still held `AtlasTextureId` references from the old atlas → index-out-of-bounds panic.
2. **set_active deadlock**: The `active_status_callback` wraps a GPUI closure that acquires its own `parking_lot::Mutex`. Calling it while holding the window state lock caused a deadlock chain.
3. **Event loop deadlock**: Android's `android-activity` crate blocks Java lifecycle callbacks on a condvar until the native thread processes the command. If the handler inside `poll_events` tried to acquire the window state lock (held by a background render thread), deadlock.

## Patch 1: Surface replacement in gpui_wgpu (crates/gpui_wgpu/src/wgpu_renderer.rs)

**Why**: Instead of destroying the entire renderer (including the atlas) when the native window is destroyed, we only unconfigure the surface. When the window is recreated, we replace the surface on the existing renderer, keeping the atlas and all cached textures intact.

```rust
/// Mark the surface as unconfigured so rendering is skipped until a new
/// surface is provided via `replace_surface`.  This does NOT drop the
/// renderer — the device, queue, atlas, and pipelines stay alive.
pub fn unconfigure_surface(&mut self) {
    self.surface_configured = false;
    // Drop intermediate textures since they reference the old surface size.
    self.path_intermediate_texture = None;
    self.path_intermediate_view = None;
    self.path_msaa_texture = None;
    self.path_msaa_view = None;
}

/// Replace the wgpu surface with a new one (e.g. after Android destroys
/// and recreates the native window).  Keeps the device, queue, atlas, and
/// all pipelines intact so cached `AtlasTextureId`s remain valid.
///
/// The `instance` must be the same `wgpu::Instance` that was used to create
/// the adapter and device (i.e., from the `WgpuContext`).
#[cfg(not(target_family = "wasm"))]
pub fn replace_surface<W: HasWindowHandle + HasDisplayHandle>(
    &mut self,
    window: &W,
    config: WgpuSurfaceConfig,
    instance: &wgpu::Instance,
) -> anyhow::Result<()> {
    let window_handle = window
        .window_handle()
        .map_err(|e| anyhow::anyhow!("Failed to get window handle: {e}"))?;
    let display_handle = window
        .display_handle()
        .map_err(|e| anyhow::anyhow!("Failed to get display handle: {e}"))?;

    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: display_handle.as_raw(),
        raw_window_handle: window_handle.as_raw(),
    };

    let surface = unsafe {
        instance
            .create_surface_unsafe(target)
            .map_err(|e| anyhow::anyhow!("Failed to create surface: {e}"))?
    };

    let width = (config.size.width.0 as u32).max(1);
    let height = (config.size.height.0 as u32).max(1);

    let alpha_mode = if config.transparent {
        self.transparent_alpha_mode
    } else {
        self.opaque_alpha_mode
    };

    self.surface_config.width = width;
    self.surface_config.height = height;
    self.surface_config.alpha_mode = alpha_mode;
    surface.configure(&self.device, &self.surface_config);

    self.surface = surface;
    self.surface_configured = true;

    // Invalidate intermediate textures — they'll be recreated lazily.
    self.path_intermediate_texture = None;
    self.path_intermediate_view = None;
    self.path_msaa_texture = None;
    self.path_msaa_view = None;

    Ok(())
}
```

**Critical detail**: The `instance` parameter must be the **same** `wgpu::Instance` that created the original adapter and device. Creating a new Instance causes a "Device does not exist" panic because the wgpu device is bound to its originating Instance.

## Patch 2: term_window keeps renderer alive (src/android/window.rs)

**Why**: Previously `term_window` destroyed the renderer. Now it only unconfigures the surface, keeping the renderer (and atlas) alive across background transitions.

```rust
pub fn term_window(&self) {
    let mut state = self.state.lock();

    // Unconfigure the surface so the renderer stops trying to present,
    // but keep the renderer alive so the atlas (with all cached
    // texture IDs) survives across the background/foreground cycle.
    if let Some(ref mut renderer) = state.renderer {
        renderer.unconfigure_surface();
    }

    // Release our reference on the native window.
    if !state.native_window.is_null() {
        unsafe { ANativeWindow_release(state.native_window) };
        state.native_window = std::ptr::null_mut();
    }

    state.is_active = false;
    self.active.store(false, std::sync::atomic::Ordering::Relaxed);
}
```

## Patch 3: init_window uses replace_surface (src/android/window.rs)

**Why**: On resume, if a renderer already exists (kept alive by the new `term_window`), we replace its surface instead of creating a new renderer.

```rust
pub unsafe fn init_window(
    &self,
    native_window: *mut ANativeWindow,
    gpu_context: &mut Option<WgpuContext>,
) -> Result<()> {
    // ... (native window setup, width/height query) ...

    let transparent = state.transparent;

    // If a renderer already exists (kept alive across term_window), just
    // replace its surface.  This preserves the atlas and all cached
    // AtlasTextureIds so GPUI's scene cache remains valid.
    if state.renderer.is_some() {
        let raw = unsafe { Self::raw_window(native_window) };
        let config = WgpuSurfaceConfig {
            size: gpui::size(gpui::DevicePixels(width), gpui::DevicePixels(height)),
            transparent,
        };
        let instance = state.gpu_context.as_ref()
            .ok_or_else(|| anyhow::anyhow!("gpu_context missing"))?
            .instance.clone();
        state.renderer.as_mut().unwrap()
            .replace_surface(&raw, config, &instance)?;
    } else {
        // First init — create a fresh renderer.
        // ... (existing create_renderer logic) ...
    }

    state.is_active = true;
    self.active.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}
```

## Patch 4: set_active deadlock fix (src/android/window.rs)

**Why**: The `active_status_callback` wraps a GPUI closure that acquires its own Mutex. Calling it while holding the window state lock creates a deadlock. Fix: take the callback out, drop the lock, invoke it, put it back.

```rust
pub fn set_active(&self, active: bool) {
    use std::sync::atomic::Ordering;
    let prev = self.active.swap(active, Ordering::Relaxed);
    if prev != active {
        // Take the callback OUT of the state so we can invoke it WITHOUT
        // holding the window state lock.
        let mut taken_cb: Option<Box<dyn FnMut(bool) + Send>> = None;
        if let Some(mut state) = self.state.try_lock() {
            state.is_active = active;
            taken_cb = state.active_status_callback.take();
        }
        // Fire callback outside the lock.
        if let Some(mut cb) = taken_cb {
            cb(active);
            // Put it back so future calls still fire.
            if let Some(mut state) = self.state.try_lock() {
                state.active_status_callback = Some(cb);
            }
        }
    }
}
```

## Patch 5: Deferred lifecycle processing (src/android/jni.rs)

**Why**: Android's `android-activity` crate's Java callbacks block on a condvar waiting for the native thread to process the command. If our handler inside `poll_events` tries to acquire the window state lock, and a background render thread holds it, we deadlock. Fix: set atomic flags inside handlers, process them after `poll_events` returns.

```rust
static PAUSE_PENDING: AtomicBool = AtomicBool::new(false);
static RESUME_PENDING: AtomicBool = AtomicBool::new(false);

// Inside handle_main_event (within poll_events):
MainEvent::Pause => {
    PAUSE_PENDING.store(true, Ordering::Relaxed);
}
MainEvent::LostFocus => {
    PAUSE_PENDING.store(true, Ordering::Relaxed);
}
MainEvent::Resume { .. } => {
    RESUME_PENDING.store(true, Ordering::Relaxed);
}
MainEvent::GainedFocus => {
    RESUME_PENDING.store(true, Ordering::Relaxed);
}

// After poll_events returns (safe to acquire locks):
if PAUSE_PENDING.swap(false, Ordering::Relaxed) {
    if let Some(platform) = PLATFORM.get() {
        platform.did_enter_background();
        if let Some(win) = platform.primary_window() {
            win.set_active(false);
        }
    }
}
if RESUME_PENDING.swap(false, Ordering::Relaxed) {
    if let Some(platform) = PLATFORM.get() {
        platform.did_become_active();
        if let Some(win) = platform.primary_window() {
            win.set_active(true);
        }
    }
}
```

## Patch 6: JNI classloader fix (src/android/jni.rs)

**Why**: `FindClass` from native threads uses the system classloader, which can't find app classes like `GpuiHelper`. Fix: use the Activity's classloader.

```rust
pub fn find_app_class<'local>(
    env: &mut jni::Env<'local>,
    class_name: &str,
) -> Result<jni::objects::JClass<'local>, String> {
    let act = activity(env)?;
    let act_class = env.get_object_class(&act)
        .map_err(|e| format!("getClass failed: {e}"))?;
    let class_loader = env
        .call_method(&act_class, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .and_then(|v| v.l())
        .map_err(|e| { let _ = env.exception_clear(); format!("getClassLoader: {e}") })?;
    let jname = env.new_string(class_name).e()?;
    let loaded = env
        .call_method(&class_loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;",
                     &[JValue::Object(&jname)])
        .and_then(|v| v.l())
        .map_err(|e| { let _ = env.exception_clear(); format!("loadClass({class_name}): {e}") })?;
    std::mem::forget(act);
    Ok(unsafe { jni::objects::JClass::from_raw(env, loaded.as_raw()) })
}
```

## Lifecycle Flow

```
Foreground → Background:
  Pause       → set atomic PAUSE_PENDING
  LostFocus   → set atomic PAUSE_PENDING
  (after poll) → platform.did_enter_background() + win.set_active(false)
  TerminateWindow → renderer.unconfigure_surface() (renderer kept alive)
  Stop/SaveState  → no-op

Background → Foreground:
  Start       → no-op
  Resume      → set atomic RESUME_PENDING
  (after poll) → platform.did_become_active() + win.set_active(true)
  InitWindow  → renderer.replace_surface(new_window, config, instance)
  GainedFocus → set atomic RESUME_PENDING
```

## Verified

Tested 3 consecutive background/foreground cycles on Motorola device (Adreno 720 GPU):
- Zero panics
- Zero deadlocks
- Same PID throughout (process not killed)
- Atlas textures preserved across cycles
- Rendering resumes immediately on return

## Source

The `gpui_wgpu` crate (`crates/gpui_wgpu/`) is a local fork of the Zed project's `gpui_wgpu` crate (rev `4dd42a0`) with the `unconfigure_surface` and `replace_surface` additions.
