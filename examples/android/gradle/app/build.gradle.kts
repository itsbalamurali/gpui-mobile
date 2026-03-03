// App module build.gradle.kts for the GPUI Mobile Android Example.
//
// This module packages the pre-compiled Rust native library into an APK
// that uses Android's NativeActivity to host the GPUI application.
//
// The Rust library must be compiled separately and placed into
// app/src/main/jniLibs/<abi>/ before building the APK.
//
// Quick start:
//   cd <repo-root>
//   cargo ndk -t arm64-v8a -o examples/android_app/gradle/app/src/main/jniLibs \
//       build --example android_app --release
//   cd examples/android_app/gradle
//   ./gradlew assembleDebug

plugins {
    id("com.android.application")
}

android {
    namespace = "com.gpui.mobile.example"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.gpui.mobile.example"
        minSdk = 26          // Vulkan 1.0 is mandatory from API 26+
        targetSdk = 34
        versionCode = 1
        versionName = "1.0.0"

        // Tell NativeActivity which .so to load.
        // This must match the cdylib / example output name.
        ndk {
            abiFilters += listOf("arm64-v8a")
        }

        // Forward the library name to the manifest via a placeholder.
        manifestPlaceholders["nativeLibraryName"] = "gpui_android_example"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
        debug {
            isDebuggable = true
            isJniDebuggable = true
        }
    }

    // We do NOT use CMake / ndk-build — the native library is compiled
    // externally via cargo-ndk and placed directly into jniLibs.
    //
    // Disable the built-in native build system so Gradle doesn't look for
    // a CMakeLists.txt or Android.mk.
    externalNativeBuild {
        // Intentionally left empty.
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    // Tell Gradle where the pre-built .so files live.
    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    packaging {
        // Prevent stripping of the Rust library — cargo already strips in
        // release mode and stripping again can break backtraces.
        jniLibs {
            keepDebugSymbols += listOf(
                "*/arm64-v8a/libgpui_android_example.so",
                "*/armeabi-v7a/libgpui_android_example.so",
                "*/x86_64/libgpui_android_example.so",
                "*/x86/libgpui_android_example.so"
            )
        }
    }

    // Lint configuration — relaxed for an example project.
    lint {
        abortOnError = false
        checkReleaseBuilds = false
    }
}

dependencies {
    // No Java/Kotlin dependencies are needed — the app is 100% native Rust.
    // If you need to call Java APIs from Rust (e.g. ClipboardManager,
    // SensorManager), add the appropriate AndroidX libraries here and
    // access them via JNI from the Rust side.
}
