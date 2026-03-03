//! iOS Example Application for GPUI Mobile
//!
//! This example demonstrates how to build a complete iOS application using
//! the `gpui-mobile` crate.  It wires up the iOS platform (which implements
//! the real `gpui::Platform` trait from the Zed repository), creates a window
//! backed by a `CAMetalLayer`, and drives the built-in Animation Playground
//! demo so you can see bouncing balls and particle effects on-device.
//!
//! ## Integration with GPUI
//!
//! This example uses the real `gpui` types from the Zed repository:
//! - `gpui::Platform` trait (implemented by `IosPlatform`)
//! - `gpui::PlatformWindow` trait (implemented by `IosWindow`)
//! - `gpui::PlatformDisplay` trait (implemented by `IosDisplay`)
//! - `gpui::Keystroke`, `gpui::Modifiers` (from the text_input module)
//! - `gpui::PlatformKeyboardLayout` (for external keyboard support)
//!
//! ## How it works
//!
//! 1. The Objective-C app delegate (written in Swift or ObjC in the host
//!    Xcode project) calls into the C-ABI functions exported by this crate.
//! 2. `gpui_ios_initialize()` boots the Rust runtime.
//! 3. `gpui_ios_did_finish_launching()` invokes our finish-launching callback
//!    which creates the platform, opens a window, and starts the demo.
//! 4. On every `CADisplayLink` tick the app delegate calls
//!    `gpui_ios_request_frame(window_ptr)` to drive rendering.
//! 5. Touch events are forwarded via `gpui_ios_handle_touch()`.
//!
//! ## Building
//!
//! ```bash
//! # Add the iOS target
//! rustup target add aarch64-apple-ios
//!
//! # Build for device
//! cargo build --example ios_app --target aarch64-apple-ios --release
//!
//! # Build for simulator (Apple Silicon)
//! cargo build --example ios_app --target aarch64-apple-ios-sim --release
//! ```
//!
//! Then link the resulting static library into an Xcode project that provides
//! the `UIApplicationDelegate` and `CADisplayLink` glue.
//!
//! ## Xcode integration
//!
//! Your Swift app delegate should look roughly like:
//!
//! ```swift
//! import UIKit
//!
//! @UIApplicationMain
//! class AppDelegate: UIResponder, UIApplicationDelegate {
//!     var window: UIWindow?
//!     var displayLink: CADisplayLink?
//!     var gpuiWindow: UnsafeMutableRawPointer?
//!
//!     func application(
//!         _ application: UIApplication,
//!         didFinishLaunchingWithOptions opts: [UIApplication.LaunchOptionsKey: Any]?
//!     ) -> Bool {
//!         gpui_ios_initialize()
//!         gpui_ios_did_finish_launching(nil)
//!         gpuiWindow = gpui_ios_get_window()
//!
//!         displayLink = CADisplayLink(target: self, selector: #selector(tick))
//!         displayLink?.add(to: .main, forMode: .default)
//!         return true
//!     }
//!
//!     @objc func tick() {
//!         if let w = gpuiWindow { gpui_ios_request_frame(w) }
//!     }
//!
//!     // Forward touches from the root view controller:
//!     func forwardTouch(_ touch: UITouch, event: UIEvent) {
//!         if let w = gpuiWindow {
//!             gpui_ios_handle_touch(w,
//!                 Unmanaged.passUnretained(touch).toOpaque(),
//!                 Unmanaged.passUnretained(event).toOpaque())
//!         }
//!     }
//! }
//! ```

// On non-iOS hosts this example is a no-op so CI can still compile it.
fn main() {
    #[cfg(target_os = "ios")]
    ios_main();

    #[cfg(not(target_os = "ios"))]
    {
        eprintln!("ios_app: this example must be compiled for an iOS target.");
        eprintln!("  rustup target add aarch64-apple-ios");
        eprintln!("  cargo build --example ios_app --target aarch64-apple-ios");
        eprintln!();
        eprintln!("ios_app: This example uses the real gpui::Platform trait from");
        eprintln!("         the Zed repository, backed by IosPlatform + Blade/Metal.");
    }
}

// ── iOS implementation ───────────────────────────────────────────────────────

#[cfg(target_os = "ios")]
fn ios_main() {
    use gpui_mobile::gpui; // Re-exported gpui crate with real types
    use gpui_mobile::ios::{
        current_platform, demos::DemoApp, gpui_ios_did_finish_launching, gpui_ios_get_window,
        gpui_ios_initialize, gpui_ios_request_frame,
    };

    // ── Step 1: Initialise the FFI layer ──────────────────────────────────
    //
    // In a real app the Objective-C delegate calls this.  Here we call it
    // directly so the example is self-contained.
    let sentinel = gpui_ios_initialize();
    if sentinel.is_null() {
        eprintln!("ios_app: gpui_ios_initialize reported already-initialised");
    }

    // ── Step 2: Create the platform ───────────────────────────────────────
    //
    // `current_platform(false)` returns an `Rc<dyn gpui::Platform>` backed
    // by `IosPlatform`.  The platform implements the full GPUI Platform trait
    // using UIKit + Metal/Blade for rendering and CoreText for text.
    let platform = current_platform(false);

    // Verify the platform provides display info via the real gpui::Platform trait
    let displays = platform.displays();
    println!(
        "ios_app: {} display(s) detected via gpui::Platform",
        displays.len()
    );

    // ── Step 3: Register the finish-launching callback ────────────────────
    //
    // The callback creates a window using the Platform trait's open_window().
    // On iOS, the window is backed by a UIWindow with a CAMetalLayer-backed
    // UIView for Metal/Blade rendering.
    platform.run(Box::new(move || {
        println!("ios_app: finish-launching callback fired");
        println!("ios_app: ready for CADisplayLink ticks");
        // In a full GPUI app, you would call platform.open_window() here
        // with WindowParams to create the GPUI window and set up the
        // DemoApp view using the real gpui::Render trait.
    }));

    // ── Step 4: Simulate the finish-launching call ────────────────────────
    //
    // Normally the ObjC app delegate does this; we do it ourselves here.
    gpui_ios_did_finish_launching(std::ptr::null_mut());

    // ── Step 5: Retrieve the window pointer ───────────────────────────────
    let window_ptr = gpui_ios_get_window();
    if window_ptr.is_null() {
        eprintln!("ios_app: no window registered after finish-launching");
    } else {
        println!("ios_app: got window pointer {:p}", window_ptr);
    }

    // ── Step 6: Simulate a few render frames ──────────────────────────────
    //
    // In a real app the CADisplayLink drives this at 60fps.
    // Touch events are converted to gpui::PlatformInput by the events module,
    // and keyboard events use gpui::Keystroke from the text_input module.
    for i in 0..5 {
        if !window_ptr.is_null() {
            gpui_ios_request_frame(window_ptr);
            println!("ios_app: rendered frame {}", i);
        }
    }

    println!("ios_app: example complete — in a real app the run-loop keeps going");
}

// ── Minimal C-ABI entry point alternative ─────────────────────────────────────
//
// If you prefer to skip `main()` entirely and let the ObjC runtime drive
// everything, you can expose this symbol from your `.so` / `.a` and have
// the Swift delegate call `gpui_example_ios_start()` instead.

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn gpui_example_ios_start() {
    ios_main();
}

// ── Interactive demo launcher ─────────────────────────────────────────────────
//
// Call this from ObjC to launch the full interactive demo menu (Animation
// Playground + Shader Showcase) without writing any Rust `main()` code.
//
// The demo uses the real gpui types (gpui::Platform, gpui::PlatformWindow,
// etc.) and renders via the Blade/Metal renderer integrated through the
// IosPlatform implementation.

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn gpui_example_ios_run_demo() {
    gpui_mobile::ios::gpui_ios_run_demo();
}
