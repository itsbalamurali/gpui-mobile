#!/usr/bin/env bash
set -euo pipefail
# Exercise an APK whose entrypoint selects AndroidBackend::Gles before run().
# Use a real GLES3.1+ Android runtime. Existing validation remains enabled.
: "${ANDROID_SERIAL:?Set the owned runtime serial}"
: "${GPUI_SMOKE_PACKAGE:?Set the actual package built with Gles selected}"
: "${GPUI_SMOKE_ACTIVITY:?Set its fully qualified launcher activity}"
SDK="${ANDROID_HOME:?Set Android SDK}"
ADB="$SDK/platform-tools/adb"
"$ADB" -s "$ANDROID_SERIAL" logcat -c
"$ADB" -s "$ANDROID_SERIAL" shell am force-stop "$GPUI_SMOKE_PACKAGE"
"$ADB" -s "$ANDROID_SERIAL" shell am start -n "$GPUI_SMOKE_PACKAGE/$GPUI_SMOKE_ACTIVITY"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
for attempt in {1..60}; do
  "$ADB" -s "$ANDROID_SERIAL" logcat -d > "$TMP"
  if grep -Eq 'failed to open window|Error in create_window_surface|Rust panic' "$TMP"; then
    cat "$TMP"
    exit 1
  fi
  if grep -q 'NATIVE_INITIALIZED' "$TMP"; then
    echo 'PASS: GPUI rendered first frame with selected GLES backend'
    exit 0
  fi
  sleep 1
done
cat "$TMP"
echo 'FAIL: no GPUI first-frame evidence' >&2
exit 1
