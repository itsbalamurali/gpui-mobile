use super::HapticFeedback;
use crate::android::jni::{self as jni_helpers, JniExt};
use jni::objects::{JObject, JValue};

pub fn vibrate(duration_ms: u32) -> Result<(), String> {
    jni_helpers::with_env(|env| {
        let activity = jni_helpers::activity()?;

        let vibrator = get_vibrator_service(env, &activity)?;

        // Try VibrationEffect.createOneShot (API 26+)
        if let Ok(ve_cls) = env.find_class("android/os/VibrationEffect") {
            if let Ok(effect) = env.call_static_method(
                &ve_cls,
                "createOneShot",
                "(JI)Landroid/os/VibrationEffect;",
                &[JValue::Long(duration_ms as i64), JValue::Int(-1)], // DEFAULT_AMPLITUDE = -1
            )
            .and_then(|v| v.l())
            {
                if !effect.is_null() {
                    let _ = env.call_method(
                        &vibrator,
                        "vibrate",
                        "(Landroid/os/VibrationEffect;)V",
                        &[JValue::Object(&effect)],
                    );
                    let _ = env.exception_clear();
                    return Ok(());
                }
            }
            let _ = env.exception_clear();
        }

        // Fallback: vibrator.vibrate(long) for older APIs
        let _ = env.call_method(
            &vibrator,
            "vibrate",
            "(J)V",
            &[JValue::Long(duration_ms as i64)],
        );
        let _ = env.exception_clear();
        Ok(())
    })
}

pub fn haptic_feedback(feedback: HapticFeedback) -> Result<(), String> {
    // Map to Android HapticFeedbackConstants
    let constant: i32 = match feedback {
        HapticFeedback::Light => 1,     // VIRTUAL_KEY
        HapticFeedback::Medium => 1,    // VIRTUAL_KEY
        HapticFeedback::Heavy => 0,     // LONG_PRESS
        HapticFeedback::Selection => 3, // KEYBOARD_TAP
        HapticFeedback::Success => 1,   // VIRTUAL_KEY
        HapticFeedback::Warning => 0,   // LONG_PRESS
        HapticFeedback::Error => 0,     // LONG_PRESS
    };

    jni_helpers::with_env(|env| {
        let activity = jni_helpers::activity()?;

        // activity.getWindow().getDecorView().performHapticFeedback(constant)
        let window = env
            .call_method(&activity, "getWindow", "()Landroid/view/Window;", &[])
            .and_then(|v| v.l())
            .e()?;
        if window.is_null() {
            return Err("getWindow returned null".into());
        }

        let decor = env
            .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])
            .and_then(|v| v.l())
            .e()?;
        if decor.is_null() {
            return Err("getDecorView returned null".into());
        }

        let _ = env.call_method(
            &decor,
            "performHapticFeedback",
            "(I)Z",
            &[JValue::Int(constant)],
        );
        let _ = env.exception_clear();
        Ok(())
    })
}

pub fn can_vibrate() -> bool {
    jni_helpers::with_env(|env| {
        let activity = jni_helpers::activity()?;

        let vibrator = get_vibrator_service(env, &activity)?;

        let result = env.call_method(&vibrator, "hasVibrator", "()Z", &[])
            .and_then(|v| v.z())
            .unwrap_or(false);
        Ok(result)
    })
    .unwrap_or(false)
}

fn get_vibrator_service<'local>(
    env: &mut jni::Env<'local>,
    activity: &JObject<'_>,
) -> Result<JObject<'local>, String> {
    let service_name = env.new_string("vibrator").e()?;
    let vibrator = env
        .call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service_name)],
        )
        .and_then(|v| v.l())
        .e()?;
    if vibrator.is_null() {
        return Err("Vibrator service not available".into());
    }
    Ok(vibrator)
}
