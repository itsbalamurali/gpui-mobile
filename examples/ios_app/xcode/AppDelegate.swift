//
//  AppDelegate.swift
//  GPUIMobileExample
//
//  A minimal UIKit app delegate that drives the gpui-mobile Rust library.
//
//  This file is the Swift half of the iOS example. It:
//    1. Calls into the C-ABI functions exported by gpui-mobile to initialise
//       the Rust runtime and create a GPUI window.
//    2. Sets up a CADisplayLink to drive frame rendering at the display's
//       native refresh rate.
//    3. Forwards UIKit touch events to the Rust layer so GPUI can process them.
//
//  To use this in an Xcode project:
//    - Add a bridging header that declares the C functions (see BridgingHeader.h).
//    - Link the compiled Rust static library (libgpui_mobile.a).
//    - Set this class as the @UIApplicationMain entry point.

import UIKit

// MARK: - C-ABI function declarations
//
// These are the `#[no_mangle] pub extern "C"` functions exported by
// gpui-mobile's `ios::ffi` module.  In a real project you'd put these in
// a bridging header; here they're declared inline for clarity.

@_silgen_name("gpui_ios_initialize")
func gpui_ios_initialize() -> UnsafeMutableRawPointer?

@_silgen_name("gpui_ios_did_finish_launching")
func gpui_ios_did_finish_launching(_ appPtr: UnsafeMutableRawPointer?)

@_silgen_name("gpui_ios_get_window")
func gpui_ios_get_window() -> UnsafeMutableRawPointer?

@_silgen_name("gpui_ios_request_frame")
func gpui_ios_request_frame(_ windowPtr: UnsafeMutableRawPointer)

@_silgen_name("gpui_ios_handle_touch")
func gpui_ios_handle_touch(
    _ windowPtr: UnsafeMutableRawPointer,
    _ touchPtr: UnsafeMutableRawPointer,
    _ eventPtr: UnsafeMutableRawPointer
)

@_silgen_name("gpui_ios_handle_key_event")
func gpui_ios_handle_key_event(
    _ windowPtr: UnsafeMutableRawPointer,
    _ keyCode: UInt32,
    _ modifiers: UInt32,
    _ isKeyDown: Bool
)

@_silgen_name("gpui_ios_show_keyboard")
func gpui_ios_show_keyboard(_ windowPtr: UnsafeMutableRawPointer)

@_silgen_name("gpui_ios_hide_keyboard")
func gpui_ios_hide_keyboard(_ windowPtr: UnsafeMutableRawPointer)

@_silgen_name("gpui_ios_will_enter_foreground")
func gpui_ios_will_enter_foreground(_ appPtr: UnsafeMutableRawPointer?)

@_silgen_name("gpui_ios_did_become_active")
func gpui_ios_did_become_active(_ appPtr: UnsafeMutableRawPointer?)

@_silgen_name("gpui_ios_will_resign_active")
func gpui_ios_will_resign_active(_ appPtr: UnsafeMutableRawPointer?)

@_silgen_name("gpui_ios_did_enter_background")
func gpui_ios_did_enter_background(_ appPtr: UnsafeMutableRawPointer?)

@_silgen_name("gpui_ios_will_terminate")
func gpui_ios_will_terminate(_ appPtr: UnsafeMutableRawPointer?)

@_silgen_name("gpui_ios_run_demo")
func gpui_ios_run_demo()

// Optional: the example-specific entry point
@_silgen_name("gpui_example_ios_start")
func gpui_example_ios_start()

@_silgen_name("gpui_example_ios_run_demo")
func gpui_example_ios_run_demo()

// MARK: - AppDelegate

@UIApplicationMain
class AppDelegate: UIResponder, UIApplicationDelegate {

    var window: UIWindow?

    /// The opaque pointer to the Rust-side `IosWindow` struct.
    private var gpuiWindowPtr: UnsafeMutableRawPointer?

    /// CADisplayLink that drives rendering at the display refresh rate.
    private var displayLink: CADisplayLink?

    // MARK: Lifecycle

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {

        // 1. Initialise the Rust runtime and GPUI platform layer.
        let sentinel = gpui_ios_initialize()
        if sentinel == nil {
            print(
                "[GPUIMobileExample] Warning: gpui_ios_initialize returned nil (already initialised?)"
            )
        }

        // 2. Invoke the finish-launching callback which creates the GPUI
        //    window (backed by a UIWindow + CAMetalLayer view).
        gpui_ios_did_finish_launching(nil)

        // 3. Retrieve the window pointer so we can feed it frames and touches.
        gpuiWindowPtr = gpui_ios_get_window()
        if gpuiWindowPtr == nil {
            print("[GPUIMobileExample] Warning: no GPUI window after finish-launching")
        } else {
            print("[GPUIMobileExample] GPUI window acquired: \(gpuiWindowPtr!)")
        }

        // 4. Create a UIWindow that fills the screen.
        //    The Rust layer creates its own CAMetalLayer-backed UIView and
        //    adds it to this window's root view controller.
        let uiWindow = UIWindow(frame: UIScreen.main.bounds)
        uiWindow.rootViewController = RootViewController()
        uiWindow.makeKeyAndVisible()
        self.window = uiWindow

        // 5. Start the display link to drive rendering.
        displayLink = CADisplayLink(target: self, selector: #selector(renderFrame))
        displayLink?.add(to: .main, forMode: .default)

        print("[GPUIMobileExample] Application launched successfully")
        return true
    }

    func applicationWillEnterForeground(_ application: UIApplication) {
        gpui_ios_will_enter_foreground(nil)
        displayLink?.isPaused = false
    }

    func applicationDidBecomeActive(_ application: UIApplication) {
        gpui_ios_did_become_active(nil)
    }

    func applicationWillResignActive(_ application: UIApplication) {
        gpui_ios_will_resign_active(nil)
    }

    func applicationDidEnterBackground(_ application: UIApplication) {
        gpui_ios_did_enter_background(nil)
        // Pause rendering while in the background to save battery.
        displayLink?.isPaused = true
    }

    func applicationWillTerminate(_ application: UIApplication) {
        displayLink?.invalidate()
        displayLink = nil
        gpui_ios_will_terminate(nil)
    }

    // MARK: Rendering

    /// Called on every display refresh (typically 60 Hz or 120 Hz on ProMotion).
    @objc private func renderFrame() {
        guard let wPtr = gpuiWindowPtr else { return }
        gpui_ios_request_frame(wPtr)
    }

    // MARK: Touch forwarding

    /// Called by the root view controller when it receives touches.
    func forwardTouch(_ touch: UITouch, event: UIEvent) {
        guard let wPtr = gpuiWindowPtr else { return }

        let touchPtr = Unmanaged.passUnretained(touch).toOpaque()
        let eventPtr = Unmanaged.passUnretained(event).toOpaque()
        gpui_ios_handle_touch(wPtr, touchPtr, eventPtr)
    }

    /// Called by the root view controller for hardware-keyboard key events.
    func forwardKeyEvent(keyCode: UInt32, modifiers: UInt32, isDown: Bool) {
        guard let wPtr = gpuiWindowPtr else { return }
        gpui_ios_handle_key_event(wPtr, keyCode, modifiers, isDown)
    }
}

// MARK: - RootViewController

/// A minimal root view controller that forwards all touch events to the
/// AppDelegate, which in turn passes them to the Rust GPUI layer.
class RootViewController: UIViewController {

    override var prefersStatusBarHidden: Bool { true }
    override var prefersHomeIndicatorAutoHidden: Bool { true }
    override var preferredScreenEdgesDeferringSystemGestures: UIRectEdge { .all }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        view.isMultipleTouchEnabled = true
    }

    // MARK: Touch handling

    private var appDelegate: AppDelegate? {
        UIApplication.shared.delegate as? AppDelegate
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let event = event else { return }
        for touch in touches {
            appDelegate?.forwardTouch(touch, event: event)
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let event = event else { return }
        for touch in touches {
            appDelegate?.forwardTouch(touch, event: event)
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let event = event else { return }
        for touch in touches {
            appDelegate?.forwardTouch(touch, event: event)
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let event = event else { return }
        for touch in touches {
            appDelegate?.forwardTouch(touch, event: event)
        }
    }

    // MARK: Key handling (external keyboard)

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        for press in presses {
            if let key = press.key {
                appDelegate?.forwardKeyEvent(
                    keyCode: UInt32(key.keyCode.rawValue),
                    modifiers: UInt32(key.modifierFlags.rawValue),
                    isDown: true
                )
            }
        }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        for press in presses {
            if let key = press.key {
                appDelegate?.forwardKeyEvent(
                    keyCode: UInt32(key.keyCode.rawValue),
                    modifiers: UInt32(key.modifierFlags.rawValue),
                    isDown: false
                )
            }
        }
    }
}
