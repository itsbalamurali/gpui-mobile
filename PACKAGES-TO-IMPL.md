and for the keyboard handling for android and ios see https://raw.githubusercontent.com/zed-industries/zed/refs/heads/main/crates/gpui_macos/src/events.rs
  https://raw.githubusercontent.com/zed-industries/zed/refs/heads/main/crates/gpui_macos/src/keyboard.rs and 
  
We implement the basic packages used in the apps in the `src/packages` module (e.g., `src/packages/connectivity`, `src/packages/sensors`, etc.). Feature-gate each package. Visit the GitHub repo of each package, review the source code, and implement the complete functionality. Also check the example code to understand usage. Implement in the following order of priority:

eg: src/packages/connectivity/mod.rs,android/,ios/ etc., we can have a common interface in mod.rs and platform-specific implementations in android/ and ios/ folders. The mod.rs file can use conditional compilation to include the appropriate platform-specific implementation based on the target platform.
Create todos for each package implementation, and track progress in the `TODO.md` file.

 
**Tier 2 — Networking & Location (critical for connected/location-aware apps)**

8. https://pub.dev/packages/geolocator
9. https://pub.dev/packages/location

**Tier 3 — User-Facing Features (notifications, sharing, media)**

10. https://pub.dev/packages/flutter_local_notifications
12. https://pub.dev/packages/video_player
13. https://pub.dev/packages/just_audio


**Tier 5 — Platform-Specific & UI Helpers**

17. https://pub.dev/packages/android_intent_plus
18. https://pub.dev/packages/android_alarm_manager_plus
19. https://pub.dev/packages/infinite_scroll_pagination

---

**Rationale for ordering:**

Tier 1 packages are dependencies or utilities that nearly every other package or feature relies on (storage, paths, app info, launching URLs). Tier 2 covers connectivity and location, which gate many runtime behaviors. Tier 3 adds the user-visible features people expect (notifications, sharing, media playback). Tier 4 is hardware access that's important but more niche. Tier 5 contains Android-only packages and a UI helper, which have the narrowest scope.
