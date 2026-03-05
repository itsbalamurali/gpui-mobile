use super::BatteryState;
use crate::android::jni as jni_helpers;
use jni::objects::JValue;

/// Android BatteryManager.EXTRA_* constants.
const BATTERY_STATUS_CHARGING: i32 = 2;
const BATTERY_STATUS_DISCHARGING: i32 = 3;
const BATTERY_STATUS_FULL: i32 = 5;
const BATTERY_STATUS_NOT_CHARGING: i32 = 4;

pub fn battery_level() -> i32 {
    let (level, scale, _) = match read_battery_sticky() {
        Some(v) => v,
        None => return -1,
    };
    if scale > 0 {
        (level * 100) / scale
    } else {
        -1
    }
}

pub fn battery_state() -> BatteryState {
    let (_, _, status) = match read_battery_sticky() {
        Some(v) => v,
        None => return BatteryState::Unknown,
    };
    match status {
        BATTERY_STATUS_CHARGING => BatteryState::Charging,
        BATTERY_STATUS_DISCHARGING | BATTERY_STATUS_NOT_CHARGING => BatteryState::Discharging,
        BATTERY_STATUS_FULL => BatteryState::Full,
        _ => BatteryState::Unknown,
    }
}

pub fn is_battery_save_mode() -> bool {
    let mut env = match jni_helpers::obtain_env() {
        Ok(e) => e,
        Err(_) => return false,
    };
    let activity = match jni_helpers::activity() {
        Ok(a) => a,
        Err(_) => return false,
    };

    // PowerManager pm = (PowerManager) context.getSystemService("power");
    let service_name = match env.new_string("power") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let pm = match env
        .call_method(
            &activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service_name)],
        )
        .and_then(|v| v.l())
    {
        Ok(o) if !o.is_null() => o,
        _ => {
            let _ = env.exception_clear();
            return false;
        }
    };

    // pm.isPowerSaveMode()
    match env
        .call_method(&pm, "isPowerSaveMode", "()Z", &[])
        .and_then(|v| v.z())
    {
        Ok(v) => v,
        Err(_) => {
            let _ = env.exception_clear();
            false
        }
    }
}

/// Read battery info from the sticky ACTION_BATTERY_CHANGED broadcast.
///
/// Returns `(level, scale, status)` or None on failure.
fn read_battery_sticky() -> Option<(i32, i32, i32)> {
    let mut env = jni_helpers::obtain_env().ok()?;
    let activity = jni_helpers::activity().ok()?;

    // IntentFilter filter = new IntentFilter(Intent.ACTION_BATTERY_CHANGED);
    let action = env.new_string("android.intent.action.BATTERY_CHANGED").ok()?;
    let filter = env
        .new_object(
            "android/content/IntentFilter",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&action)],
        )
        .ok()?;

    // Intent batteryStatus = context.registerReceiver(null, filter);
    let battery_intent = env
        .call_method(
            &activity,
            "registerReceiver",
            "(Landroid/content/BroadcastReceiver;Landroid/content/IntentFilter;)Landroid/content/Intent;",
            &[JValue::Object(&jni::objects::JObject::null()), JValue::Object(&filter)],
        )
        .and_then(|v| v.l())
        .ok()?;
    if battery_intent.is_null() {
        return None;
    }

    // int level = intent.getIntExtra("level", -1);
    let key_level = env.new_string("level").ok()?;
    let level = env
        .call_method(
            &battery_intent,
            "getIntExtra",
            "(Ljava/lang/String;I)I",
            &[JValue::Object(&key_level), JValue::Int(-1)],
        )
        .and_then(|v| v.i())
        .ok()?;

    // int scale = intent.getIntExtra("scale", -1);
    let key_scale = env.new_string("scale").ok()?;
    let scale = env
        .call_method(
            &battery_intent,
            "getIntExtra",
            "(Ljava/lang/String;I)I",
            &[JValue::Object(&key_scale), JValue::Int(-1)],
        )
        .and_then(|v| v.i())
        .ok()?;

    // int status = intent.getIntExtra("status", -1);
    let key_status = env.new_string("status").ok()?;
    let status = env
        .call_method(
            &battery_intent,
            "getIntExtra",
            "(Ljava/lang/String;I)I",
            &[JValue::Object(&key_status), JValue::Int(-1)],
        )
        .and_then(|v| v.i())
        .ok()?;

    Some((level, scale, status))
}
