@echo off
setlocal enabledelayedexpansion

:: ──────────────────────────────────────────────────────────────────────────────
:: build.bat — Build & run the GPUI example app on Android
:: ──────────────────────────────────────────────────────────────────────────────

:: -- Resolve paths ------------------------------------------------------------
set "SCRIPT_DIR=%~dp0"
if "%SCRIPT_DIR:~-1%"=="\" set "SCRIPT_DIR=%SCRIPT_DIR:~0,-1%"

pushd "%SCRIPT_DIR%\.."
set "GPUI_ROOT=%cd%"
popd

set "EXAMPLES_DIR=%SCRIPT_DIR%"
set "ANDROID_GRADLE_DIR=%EXAMPLES_DIR%\android\gradle"

:: -- Default Settings ---------------------------------------------------------
set "PLATFORM=%~1"
set "TARGET_KIND=device"
set "PROFILE=debug"
set "CLEAN=false"
set "NO_RUN=false"

:: -- Usage / Help -------------------------------------------------------------
if "%~1"=="" goto usage
if "%~1"=="-h" goto usage
if "%~1"=="--help" goto usage

:: -- Subcommand check ---------------------------------------------------------
if /I "%PLATFORM%"=="ios" (
    echo [ERROR] iOS builds require macOS and Xcode. You cannot build for iOS on Windows.
    exit /b 1
)
if /I NOT "%PLATFORM%"=="android" (
    echo [ERROR] Unknown subcommand: %PLATFORM%
    goto usage
)
shift

:: -- Parse arguments (Flat parser) --------------------------------------------
:parse_args
if "%~1" == "" goto validate_args
if /I "%~1" == "--device"    set "TARGET_KIND=device"& shift & goto parse_args
if /I "%~1" == "--emulator"  set "TARGET_KIND=emulator"& shift & goto parse_args
if /I "%~1" == "--release"   set "PROFILE=release"& shift & goto parse_args
if /I "%~1" == "--clean"     set "CLEAN=true"& shift & goto parse_args
if /I "%~1" == "--no-run"    set "NO_RUN=true"& shift & goto parse_args
if /I "%~1" == "-h"          goto usage
if /I "%~1" == "--help"      goto usage
shift
goto parse_args

:validate_args

:: ═════════════════════════════════════════════════════════════════════════════
:: Android Build Logic
:: ═════════════════════════════════════════════════════════════════════════════

echo.
echo [STEP] Android --- %PROFILE% --- %TARGET_KIND%
echo.

:: 1. Check prerequisites
where cargo-ndk >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] cargo-ndk not found. Install it with: cargo install cargo-ndk
    exit /b 1
)

:: Try to find ANDROID_HOME if not set
if "%ANDROID_HOME%"=="" if "%ANDROID_SDK_ROOT%"=="" (
    if EXIST "%LOCALAPPDATA%\Android\Sdk" set "ANDROID_HOME=%LOCALAPPDATA%\Android\Sdk"
)

set "RUST_TARGET=aarch64-linux-android"
set "NDK_ABI=arm64-v8a"
set "CARGO_PROFILE_FLAG="
set "GRADLE_TASK=assembleDebug"
set "APK_VARIANT=debug"

if /I "%PROFILE%"=="release" (
    set "CARGO_PROFILE_FLAG=--release"
    set "GRADLE_TASK=assembleRelease"
    set "APK_VARIANT=release"
)

:: 2. Ensure Rust target
echo [INFO] Ensuring Rust target %RUST_TARGET% is installed...
rustup target add %RUST_TARGET% >nul 2>nul

:: 3. Clean (optional)
if /I "%CLEAN%" NEQ "true" goto skip_clean
echo [INFO] Cleaning Rust build artifacts...
pushd "%EXAMPLES_DIR%"
cargo clean --target %RUST_TARGET% >nul 2>nul
popd

echo [INFO] Cleaning Gradle...
pushd "%ANDROID_GRADLE_DIR%"
call gradlew.bat clean >nul 2>nul
popd
:skip_clean

:: 4. Build the Rust shared library
echo.
echo [STEP] Building Rust shared library for %NDK_ABI% (%PROFILE%)
set "JNI_LIBS_DIR=%ANDROID_GRADLE_DIR%\app\src\main\jniLibs"

pushd "%EXAMPLES_DIR%"
cargo ndk -t %NDK_ABI% -o "%JNI_LIBS_DIR%" --platform 31 build %CARGO_PROFILE_FLAG%
set "RUST_RES=%ERRORLEVEL%"
popd

if %RUST_RES% NEQ 0 (
    echo [ERROR] Rust build failed.
    exit /b 1
)

:: 5. Assemble APK
echo.
echo [STEP] Assembling APK (%GRADLE_TASK%)
pushd "%ANDROID_GRADLE_DIR%"
call gradlew.bat %GRADLE_TASK%
set "GRADLE_RES=%ERRORLEVEL%"
popd

if %GRADLE_RES% NEQ 0 (
    echo [ERROR] Gradle build failed.
    exit /b 1
)

set "APK_PATH=%ANDROID_GRADLE_DIR%\app\build\outputs\apk\%APK_VARIANT%\app-%APK_VARIANT%.apk"
if NOT EXIST "%APK_PATH%" (
    echo [ERROR] APK not found at: %APK_PATH%
    exit /b 1
)
echo [INFO] APK: %APK_PATH%

:: 6. Install & Launch
if /I "%NO_RUN%"=="true" (
    echo [INFO] Skipping install ^& launch ^(--no-run^).
    goto done
)

echo/
echo [STEP] Installing ^& launching on Android

:: Check adb
where adb >nul 2>nul
if %ERRORLEVEL% NEQ 0 if EXIST "%ANDROID_HOME%\platform-tools\adb.exe" set "PATH=%ANDROID_HOME%\platform-tools;!PATH!"

where adb >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] adb not found. Add platform-tools to PATH.
    exit /b 1
)

:: Check for devices using findstr (more stable than FOR loops)
adb devices | findstr /v "List" | findstr "device" >nul
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] No connected Android device/emulator found.
    exit /b 1
)

echo [INFO] Installing APK...
adb install -r "%APK_PATH%"
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] APK installation failed.
    exit /b 1
)

echo [INFO] Launching app...
:: Use direct command, no complex line continuation
adb shell am start -n "dev.gpui.mobile.example/android.app.NativeActivity" -a android.intent.action.MAIN -c android.intent.category.LAUNCHER

echo [INFO] App launched on Android!
echo [INFO] View logs with: adb logcat -s gpui-mobile-example:D

:done
echo.
echo [INFO] Done!
exit /b 0

:usage
echo Usage:
echo   build.bat android [--device^|--emulator] [--release] [--clean] [--no-run]
echo.
echo Options:
echo   --device      Target a physical device (default)
echo   --emulator    Target an Android emulator
echo   --release     Release build (default: debug)
echo   --clean       Clean before building
echo   --no-run      Build only - skip install ^& launch
exit /b 0