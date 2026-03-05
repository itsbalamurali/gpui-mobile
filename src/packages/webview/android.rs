use super::{WebViewHandle, WebViewSettings};
use crate::android::jni::{self as jni_helpers, JniExt};
use jni::objects::JValue;

const HELPER_CLASS: &str = "dev.gpui.mobile.GpuiHelper";

pub fn load_url(url: &str, settings: &WebViewSettings) -> Result<WebViewHandle, String> {
    jni_helpers::with_env(|env| {
        let activity = jni_helpers::activity(env)?;
        let cls = jni_helpers::find_app_class(env, HELPER_CLASS)?;
        let jurl = env.new_string(url).e()?;

        let handle_id = env
            .call_static_method(
                &cls,
                jni::jni_str!("loadUrl"),
                jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;ZZZ)I"),
                &[
                    JValue::Object(&activity),
                    JValue::Object(&jurl),
                    JValue::Bool(settings.javascript_enabled),
                    JValue::Bool(settings.dom_storage_enabled),
                    JValue::Bool(settings.zoom_enabled),
                ],
            )
            .and_then(|v| v.i())
            .map_err(|e| {
                let _ = env.exception_clear();
                e.to_string()
            })?;

        std::mem::forget(activity);

        if handle_id < 0 {
            return Err("GpuiHelper.loadUrl failed".into());
        }
        Ok(WebViewHandle {
            ptr: handle_id as usize,
        })
    })
}

pub fn load_html(html: &str, settings: &WebViewSettings) -> Result<WebViewHandle, String> {
    jni_helpers::with_env(|env| {
        let activity = jni_helpers::activity(env)?;
        let cls = jni_helpers::find_app_class(env, HELPER_CLASS)?;
        let jhtml = env.new_string(html).e()?;

        let handle_id = env
            .call_static_method(
                &cls,
                jni::jni_str!("loadHtml"),
                jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;ZZZ)I"),
                &[
                    JValue::Object(&activity),
                    JValue::Object(&jhtml),
                    JValue::Bool(settings.javascript_enabled),
                    JValue::Bool(settings.dom_storage_enabled),
                    JValue::Bool(settings.zoom_enabled),
                ],
            )
            .and_then(|v| v.i())
            .map_err(|e| {
                let _ = env.exception_clear();
                e.to_string()
            })?;

        std::mem::forget(activity);

        if handle_id < 0 {
            return Err("GpuiHelper.loadHtml failed".into());
        }
        Ok(WebViewHandle {
            ptr: handle_id as usize,
        })
    })
}

pub fn evaluate_javascript(handle: &WebViewHandle, script: &str) -> Result<(), String> {
    if handle.ptr == 0 {
        return Err("No active WebView".into());
    }
    jni_helpers::with_env(|env| {
        let cls = jni_helpers::find_app_class(env, HELPER_CLASS)?;
        let jscript = env.new_string(script).e()?;
        env.call_static_method(
            &cls,
            jni::jni_str!("evaluateJavascript"),
            jni::jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&jscript)],
        )
        .map_err(|e| {
            let _ = env.exception_clear();
            e.to_string()
        })?;
        Ok(())
    })
}

pub fn dismiss(handle: WebViewHandle) -> Result<(), String> {
    if handle.ptr == 0 {
        return Ok(());
    }
    jni_helpers::with_env(|env| {
        let activity = jni_helpers::activity(env)?;
        let cls = jni_helpers::find_app_class(env, HELPER_CLASS)?;
        env.call_static_method(
            &cls,
            jni::jni_str!("dismissWebView"),
            jni::jni_sig!("(Landroid/app/Activity;)V"),
            &[JValue::Object(&activity)],
        )
        .map_err(|e| {
            let _ = env.exception_clear();
            e.to_string()
        })?;
        std::mem::forget(activity);
        Ok(())
    })
}
