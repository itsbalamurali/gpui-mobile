#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# build.sh — Build & run the GPUI example app on iOS or Android
#
# Usage:
#   ./build.sh ios   [--device | --simulator] [--release] [--clean]
#   ./build.sh android [--device | --emulator] [--release] [--clean]
#
# Subcommands:
#   ios       Build the Rust static library, generate the Xcode project via
#             XcodeGen, build the .app bundle, and install+launch on a
#             connected iPhone (default) or the iOS Simulator.
#
#   android   Build the Rust shared library via cargo-ndk, assemble the APK
#             via Gradle, and install+launch on a connected Android device
#             (default) or emulator.
#
# Options:
#   --device      Target a physical device (default for both platforms).
#   --simulator   (iOS only) Target the iOS Simulator instead of a device.
#   --emulator    (Android only) Target an Android emulator instead of a device.
#   --release     Build in release mode (default is debug).
#   --clean       Run a clean build (cargo clean + xcode/gradle clean).
#   --no-run      Build only — do not install or launch the app.
#   -h, --help    Show this help message.
#
# Prerequisites:
#   iOS:     Xcode, XcodeGen (brew install xcodegen), rustup target
#            aarch64-apple-ios / aarch64-apple-ios-sim
#   Android: Android SDK + NDK, cargo-ndk (cargo install cargo-ndk),
#            rustup target aarch64-linux-android
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Resolve paths ────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# SCRIPT_DIR = gpui/examples
GPUI_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# GPUI_ROOT  = gpui/
EXAMPLES_DIR="$SCRIPT_DIR"
IOS_DIR="$EXAMPLES_DIR/ios"
ANDROID_GRADLE_DIR="$EXAMPLES_DIR/android/gradle"

# ── Colours (if stdout is a terminal) ────────────────────────────────────────

if [ -t 1 ]; then
    BOLD='\033[1m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    RED='\033[0;31m'
    CYAN='\033[0;36m'
    RESET='\033[0m'
else
    BOLD='' GREEN='' YELLOW='' RED='' CYAN='' RESET=''
fi

info()  { echo -e "${GREEN}▸${RESET} $*"; }
warn()  { echo -e "${YELLOW}⚠${RESET} $*"; }
error() { echo -e "${RED}✘${RESET} $*" >&2; }
step()  { echo -e "\n${BOLD}${CYAN}══ $* ══${RESET}\n"; }

# ── Usage ────────────────────────────────────────────────────────────────────

usage() {
    cat <<'EOF'
Usage:
  ./build.sh ios     [--device|--simulator] [--release] [--clean] [--no-run]
  ./build.sh android [--device|--emulator]  [--release] [--clean] [--no-run]

Subcommands:
  ios       Build & run on iOS (physical device by default)
  android   Build & run on Android (physical device by default)

Options:
  --device      Target a physical device (default)
  --simulator   (iOS) Target the iOS Simulator
  --emulator    (Android) Target an Android emulator
  --release     Release build (default: debug)
  --clean       Clean before building
  --no-run      Build only — skip install & launch
  -h, --help    Show this help
EOF
}

# ── Parse arguments ──────────────────────────────────────────────────────────

PLATFORM=""
TARGET_KIND="device"   # device | simulator | emulator
PROFILE="debug"
CLEAN=false
NO_RUN=false

if [[ $# -lt 1 ]]; then
    usage
    exit 1
fi

case "${1:-}" in
    ios|android) PLATFORM="$1"; shift ;;
    -h|--help)   usage; exit 0 ;;
    *)           error "Unknown subcommand: $1"; usage; exit 1 ;;
esac

while [[ $# -gt 0 ]]; do
    case "$1" in
        --device)    TARGET_KIND="device"    ;;
        --simulator) TARGET_KIND="simulator" ;;
        --emulator)  TARGET_KIND="emulator"  ;;
        --release)   PROFILE="release"       ;;
        --clean)     CLEAN=true              ;;
        --no-run)    NO_RUN=true             ;;
        -h|--help)   usage; exit 0           ;;
        *) error "Unknown option: $1"; usage; exit 1 ;;
    esac
    shift
done

# Validate combos
if [[ "$PLATFORM" == "ios" && "$TARGET_KIND" == "emulator" ]]; then
    error "--emulator is only valid for the android subcommand. Did you mean --simulator?"
    exit 1
fi
if [[ "$PLATFORM" == "android" && "$TARGET_KIND" == "simulator" ]]; then
    error "--simulator is only valid for the ios subcommand. Did you mean --emulator?"
    exit 1
fi

# ═════════════════════════════════════════════════════════════════════════════
# iOS
# ═════════════════════════════════════════════════════════════════════════════

build_ios() {
    step "iOS — ${PROFILE} — ${TARGET_KIND}"

    # ── Determine Rust target triple ─────────────────────────────────────
    local rust_target
    local xcode_sdk
    local xcode_destination

    if [[ "$TARGET_KIND" == "simulator" ]]; then
        rust_target="aarch64-apple-ios-sim"
        xcode_sdk="iphonesimulator"
        xcode_destination="platform=iOS Simulator,name=iPhone 16 Pro"
    else
        rust_target="aarch64-apple-ios"
        xcode_sdk="iphoneos"
        # For physical devices, use a generic destination so Xcode picks the
        # first connected device automatically.
        xcode_destination="generic/platform=iOS"
    fi

    local cargo_profile_flag=""
    local cargo_profile_dir="debug"
    if [[ "$PROFILE" == "release" ]]; then
        cargo_profile_flag="--release"
        cargo_profile_dir="release"
    fi

    local xcode_config
    if [[ "$PROFILE" == "release" ]]; then
        xcode_config="Release"
    else
        xcode_config="Debug"
    fi

    # ── Ensure Rust target is installed ──────────────────────────────────
    info "Ensuring Rust target ${rust_target} is installed..."
    rustup target add "$rust_target" 2>/dev/null || true

    # ── Clean (optional) ─────────────────────────────────────────────────
    if $CLEAN; then
        info "Cleaning Rust build artifacts..."
        cd "$GPUI_ROOT"
        cargo clean --target "$rust_target" 2>/dev/null || true

        if [[ -d "$IOS_DIR/GpuiExample.xcodeproj" ]]; then
            info "Cleaning Xcode derived data..."
            xcodebuild clean \
                -project "$IOS_DIR/GpuiExample.xcodeproj" \
                -scheme GpuiExample \
                -configuration "$xcode_config" \
                2>/dev/null || true
        fi
    fi

    # ── Build the example crate (it depends on gpui-mobile, so both are built) ─
    step "Building example crate for ${rust_target} (${PROFILE})"

    cd "$EXAMPLES_DIR"
    cargo build \
        --target "$rust_target" \
        $cargo_profile_flag \
        2>&1

    local example_lib="$EXAMPLES_DIR/target/${rust_target}/${cargo_profile_dir}/libgpui_mobile_example.a"
    if [[ ! -f "$example_lib" ]]; then
        error "Example static library not found at: $example_lib"
        exit 1
    fi
    info "Example static library: $example_lib"

    # ── Generate Xcode project via XcodeGen ──────────────────────────────
    step "Generating Xcode project with XcodeGen"

    if ! command -v xcodegen &>/dev/null; then
        error "XcodeGen not found. Install it with: brew install xcodegen"
        exit 1
    fi

    cd "$IOS_DIR"
    xcodegen generate --spec project.yml
    info "Xcode project generated at: $IOS_DIR/GpuiExample.xcodeproj"

    # ── Build with xcodebuild ────────────────────────────────────────────
    step "Building Xcode project (${xcode_config}, ${xcode_sdk})"

    local build_dir="$IOS_DIR/build"
    mkdir -p "$build_dir"

    # For physical devices we need to resolve to a specific device for
    # the build-for-running / install steps.
    local resolved_destination="$xcode_destination"
    local device_id=""

    if [[ "$TARGET_KIND" == "device" ]]; then
        # Use xcodebuild -showdestinations to get the Xcode-native device ID.
        # devicectl returns a CoreDevice UUID which does NOT match the UDID
        # that xcodebuild expects in "-destination id=...".
        device_id=$(xcodebuild \
            -project GpuiExample.xcodeproj \
            -scheme GpuiExample \
            -showdestinations 2>/dev/null \
            | grep "platform:iOS," \
            | grep -v Simulator \
            | grep -v placeholder \
            | head -1 \
            | sed -E 's/.*id:([^,}]+).*/\1/' \
            | tr -d '[:space:]') || true

        if [[ -z "$device_id" ]]; then
            # Fallback: try xctrace
            device_id=$(xcrun xctrace list devices 2>/dev/null \
                | grep -i "iphone\|ipad" \
                | grep -v Simulator \
                | head -1 \
                | sed -E 's/.*\(([A-Fa-f0-9-]+)\).*/\1/') || true
        fi

        if [[ -n "$device_id" ]]; then
            resolved_destination="id=${device_id}"
            info "Targeting connected device: ${device_id}"
        else
            warn "No connected iOS device found — building for generic iOS device."
            warn "The app will be built but cannot be installed without a device."
        fi
    fi

    xcodebuild \
        -project GpuiExample.xcodeproj \
        -scheme GpuiExample \
        -configuration "$xcode_config" \
        -destination "$resolved_destination" \
        -derivedDataPath "$build_dir" \
        -allowProvisioningUpdates \
        CODE_SIGN_STYLE=Automatic \
        build \
        2>&1 | tail -30

    info "Xcode build complete."

    # ── Install & launch ─────────────────────────────────────────────────
    if $NO_RUN; then
        info "Skipping install & launch (--no-run)."
        return 0
    fi

    if [[ "$TARGET_KIND" == "simulator" ]]; then
        _ios_run_simulator "$build_dir" "$xcode_config"
    else
        _ios_run_device "$build_dir" "$xcode_config" "$device_id"
    fi
}

_ios_run_simulator() {
    local build_dir="$1"
    local xcode_config="$2"

    step "Installing on iOS Simulator"

    # Find the .app bundle
    local app_path
    app_path=$(find "$build_dir" -path "*/Build/Products/${xcode_config}-iphonesimulator/GpuiExample.app" -type d | head -1)

    if [[ -z "$app_path" ]]; then
        error "Could not find .app bundle in build output."
        exit 1
    fi
    info "App bundle: $app_path"

    # Boot a simulator if needed
    local sim_id
    sim_id=$(xcrun simctl list devices available | grep "iPhone" | head -1 | sed -E 's/.*\(([A-F0-9-]+)\).*/\1/') || true

    if [[ -z "$sim_id" ]]; then
        error "No available iOS simulator found."
        exit 1
    fi

    info "Booting simulator ${sim_id}..."
    xcrun simctl boot "$sim_id" 2>/dev/null || true
    open -a Simulator 2>/dev/null || true

    info "Installing app on simulator..."
    xcrun simctl install "$sim_id" "$app_path"

    info "Launching app..."
    xcrun simctl launch "$sim_id" dev.zed.gpui-ios-example
    info "App launched on simulator! 🚀"
}

_ios_run_device() {
    local build_dir="$1"
    local xcode_config="$2"
    local device_id="$3"

    step "Installing on physical iOS device"

    if [[ -z "$device_id" ]]; then
        error "No connected iOS device found. Connect a device and try again."
        exit 1
    fi

    # Find the .app bundle for iphoneos
    local app_path
    app_path=$(find "$build_dir" -path "*/Build/Products/${xcode_config}-iphoneos/GpuiExample.app" -type d | head -1)

    if [[ -z "$app_path" ]]; then
        error "Could not find .app bundle in build output."
        error "Make sure code signing is configured (DEVELOPMENT_TEAM in project.yml)."
        exit 1
    fi
    info "App bundle: $app_path"

    # Install using devicectl (Xcode 15+)
    if command -v xcrun &>/dev/null && xcrun devicectl list devices &>/dev/null 2>&1; then
        info "Installing via devicectl to device ${device_id}..."
        xcrun devicectl device install app \
            --device "$device_id" \
            "$app_path" \
            2>&1

        info "Launching app on device..."
        xcrun devicectl device process launch \
            --device "$device_id" \
            dev.zed.gpui-ios-example \
            2>&1 || true

        info "App launched on device! 🚀"
    else
        # Fallback: ios-deploy
        if command -v ios-deploy &>/dev/null; then
            info "Installing & launching via ios-deploy..."
            ios-deploy --bundle "$app_path" --debug --no-wifi 2>&1
        else
            warn "Could not install on device."
            warn "Install Xcode 15+ (for devicectl) or ios-deploy (npm -g install ios-deploy)."
            info "App bundle is at: $app_path"
        fi
    fi
}

# ═════════════════════════════════════════════════════════════════════════════
# Android
# ═════════════════════════════════════════════════════════════════════════════

build_android() {
    step "Android — ${PROFILE} — ${TARGET_KIND}"

    # ── Check prerequisites ──────────────────────────────────────────────
    if ! command -v cargo-ndk &>/dev/null; then
        error "cargo-ndk not found. Install it with: cargo install cargo-ndk"
        exit 1
    fi

    if [[ -z "${ANDROID_HOME:-}" && -z "${ANDROID_SDK_ROOT:-}" ]]; then
        # Try common default locations
        if [[ -d "$HOME/Library/Android/sdk" ]]; then
            export ANDROID_HOME="$HOME/Library/Android/sdk"
        elif [[ -d "$HOME/Android/Sdk" ]]; then
            export ANDROID_HOME="$HOME/Android/Sdk"
        else
            warn "ANDROID_HOME / ANDROID_SDK_ROOT not set. Gradle may fail."
        fi
    fi

    local rust_target="aarch64-linux-android"
    local ndk_abi="arm64-v8a"

    local cargo_profile_flag=""
    if [[ "$PROFILE" == "release" ]]; then
        cargo_profile_flag="--release"
    fi

    local gradle_task
    if [[ "$PROFILE" == "release" ]]; then
        gradle_task="assembleRelease"
    else
        gradle_task="assembleDebug"
    fi

    local apk_variant
    if [[ "$PROFILE" == "release" ]]; then
        apk_variant="release"
    else
        apk_variant="debug"
    fi

    # ── Ensure Rust target is installed ──────────────────────────────────
    info "Ensuring Rust target ${rust_target} is installed..."
    rustup target add "$rust_target" 2>/dev/null || true

    # ── Clean (optional) ─────────────────────────────────────────────────
    if $CLEAN; then
        info "Cleaning Rust build artifacts..."
        cd "$EXAMPLES_DIR"
        cargo clean --target "$rust_target" 2>/dev/null || true

        info "Cleaning Gradle..."
        cd "$ANDROID_GRADLE_DIR"
        ./gradlew clean 2>/dev/null || true
    fi

    # ── Build the Rust shared library via cargo-ndk ──────────────────────
    step "Building Rust shared library for ${ndk_abi} (${PROFILE})"

    local jni_libs_dir="$ANDROID_GRADLE_DIR/app/src/main/jniLibs"

    cd "$EXAMPLES_DIR"
    cargo ndk \
        -t "$ndk_abi" \
        -o "$jni_libs_dir" \
        build \
        $cargo_profile_flag \
        2>&1

    local so_path="$jni_libs_dir/${ndk_abi}/libgpui_mobile_example.so"
    if [[ ! -f "$so_path" ]]; then
        error "Shared library not found at: $so_path"
        exit 1
    fi
    info "Shared library: $so_path ($(du -h "$so_path" | cut -f1))"

    # ── Assemble APK via Gradle ──────────────────────────────────────────
    step "Assembling APK (${gradle_task})"

    cd "$ANDROID_GRADLE_DIR"
    ./gradlew "$gradle_task" 2>&1

    local apk_path="$ANDROID_GRADLE_DIR/app/build/outputs/apk/${apk_variant}/app-${apk_variant}.apk"
    if [[ ! -f "$apk_path" ]]; then
        error "APK not found at: $apk_path"
        exit 1
    fi
    info "APK: $apk_path ($(du -h "$apk_path" | cut -f1))"

    # ── Install & launch ─────────────────────────────────────────────────
    if $NO_RUN; then
        info "Skipping install & launch (--no-run)."
        return 0
    fi

    _android_install_and_launch "$apk_path"
}

_android_install_and_launch() {
    local apk_path="$1"

    step "Installing & launching on Android"

    if ! command -v adb &>/dev/null; then
        # Try finding adb from ANDROID_HOME
        local adb_candidate="${ANDROID_HOME:-}/platform-tools/adb"
        if [[ -x "$adb_candidate" ]]; then
            export PATH="${ANDROID_HOME}/platform-tools:$PATH"
        else
            error "adb not found. Make sure Android SDK platform-tools are in your PATH."
            info "APK is at: $apk_path"
            exit 1
        fi
    fi

    # Check for connected device / running emulator
    local device_count
    device_count=$(adb devices 2>/dev/null | grep -cE '\t(device|emulator)') || true

    if [[ "$device_count" -eq 0 ]]; then
        if [[ "$TARGET_KIND" == "emulator" ]]; then
            warn "No running emulator found. Attempting to start one..."
            local avd_name
            avd_name=$(emulator -list-avds 2>/dev/null | head -1) || true
            if [[ -n "$avd_name" ]]; then
                info "Starting emulator: $avd_name"
                emulator -avd "$avd_name" -no-snapshot-load &
                info "Waiting for emulator to boot..."
                adb wait-for-device
                sleep 10
            else
                error "No AVDs found. Create one with Android Studio or avdmanager."
                exit 1
            fi
        else
            error "No connected Android device found. Connect a device and try again."
            info "APK is at: $apk_path"
            exit 1
        fi
    fi

    info "Installing APK..."
    adb install -r "$apk_path" 2>&1

    info "Launching app..."
    adb shell am start \
        -n "com.gpui.mobile.example/android.app.NativeActivity" \
        -a android.intent.action.MAIN \
        -c android.intent.category.LAUNCHER \
        2>&1

    info "App launched on Android! 🚀"

    echo ""
    info "View logs with:  adb logcat -s gpui-mobile-example:D"
}

# ═════════════════════════════════════════════════════════════════════════════
# Main dispatch
# ═════════════════════════════════════════════════════════════════════════════

case "$PLATFORM" in
    ios)     build_ios     ;;
    android) build_android ;;
esac

echo ""
info "${BOLD}Done!${RESET}"
