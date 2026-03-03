# ProGuard / R8 rules for the GPUI Mobile Android Example.
#
# This is a pure native (Rust) application using NativeActivity — there is no
# Java or Kotlin application code to shrink, optimize, or obfuscate.
#
# These rules are intentionally empty.  They exist only because the
# build.gradle.kts references this file in the release buildType's
# proguardFiles configuration.

# Keep NativeActivity since it is referenced by name in AndroidManifest.xml.
-keep class android.app.NativeActivity { *; }
