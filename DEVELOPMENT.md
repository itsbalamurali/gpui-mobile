# Development

## NativeActivity lifecycle regression

`android::jni::init_platform` returns a `PlatformSession` guard. Keep it in a
binding in `android_main` until after the GPUI `Application` has been dropped:

```rust,ignore
let _platform_session = jni::init_platform(&app);
let platform = jni::shared_platform().expect("registered platform");
Application::with_platform(platform.into_rc()).run(|cx| {
    // Open the initial window here.
});
```

Each NativeActivity gets its own AndroidApp, platform, and native-thread Looper.
The guard unregisters the dispatcher and frame-source Looper, cancels queued
tasks, resets readiness, and clears the Activity registration on exit. Do not
drop the guard immediately after initialization. It dereferences to the platform
Arc for initialization callbacks; `jni::platform()` now returns a cloned Arc
rather than a process-lifetime reference.

The host-driven entry point remains process-lived. Its platform registration is
not cleared when its Java Activity is recreated.

After building the Android example's native library (see `example/README.md`),
build and run the two independent instrumentation scenarios on an unlocked device:

```sh
cd example/android/gradle
./gradlew assembleDebug assembleDebugAndroidTest
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb install -r app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
adb shell am instrument -w \
  dev.gpui.mobile.example.test/dev.gpui.mobile.example.ActivityLifecycleTest
adb shell am instrument -w -e scenario background \
  dev.gpui.mobile.example.test/dev.gpui.mobile.example.ActivityLifecycleTest
```

The default scenario recreates the Activity three times in one process, waiting
for a focused, ready native window after each recreation. The background scenario
leaves the app and checks that the same Activity regains a ready native window.
Success prints `PASS`. A native crash terminates instrumentation and is a failure.
Before the fix, the first recreation reuses the first Activity's dispatcher and
aborts with `must construct App on main thread`.
