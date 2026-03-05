use super::{WebViewHandle, WebViewSettings};
use crate::android::jni::{self as jni_helpers, JniExt};
use jni::objects::{JObject, JValue};

pub fn load_url(url: &str, settings: &WebViewSettings) -> Result<WebViewHandle, String> {
    let mut env = jni_helpers::obtain_env()?;
    let activity = jni_helpers::activity()?;

    // WebView webview = new WebView(activity);
    let webview = env
        .new_object(
            "android/webkit/WebView",
            "(Landroid/content/Context;)V",
            &[JValue::Object(&activity)],
        )
        .e()?;

    configure_webview(&mut env, &webview, settings)?;

    // webview.loadUrl(url)
    let jurl = env.new_string(url).e()?;
    let _ = env.call_method(
        &webview,
        "loadUrl",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&jurl)],
    );
    let _ = env.exception_clear();

    add_to_content_view(&mut env, &activity, &webview)?;

    // Store as a global ref so it survives past this JNI call
    let global = env.new_global_ref(&webview).e()?;
    let ptr = global.as_raw() as usize;
    std::mem::forget(global); // prevent drop — will be cleaned up in dismiss()
    Ok(WebViewHandle { ptr })
}

pub fn load_html(html: &str, settings: &WebViewSettings) -> Result<WebViewHandle, String> {
    let mut env = jni_helpers::obtain_env()?;
    let activity = jni_helpers::activity()?;

    let webview = env
        .new_object(
            "android/webkit/WebView",
            "(Landroid/content/Context;)V",
            &[JValue::Object(&activity)],
        )
        .e()?;

    configure_webview(&mut env, &webview, settings)?;

    // webview.loadData(html, "text/html", "UTF-8")
    let jhtml = env.new_string(html).e()?;
    let mime = env.new_string("text/html").e()?;
    let encoding = env.new_string("UTF-8").e()?;
    let _ = env.call_method(
        &webview,
        "loadData",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(&jhtml), JValue::Object(&mime), JValue::Object(&encoding)],
    );
    let _ = env.exception_clear();

    add_to_content_view(&mut env, &activity, &webview)?;

    let global = env.new_global_ref(&webview).e()?;
    let ptr = global.as_raw() as usize;
    std::mem::forget(global);
    Ok(WebViewHandle { ptr })
}

pub fn evaluate_javascript(handle: &WebViewHandle, script: &str) -> Result<(), String> {
    let mut env = jni_helpers::obtain_env()?;
    let webview = unsafe { JObject::from_raw(handle.ptr as jni::sys::jobject) };

    let jscript = env.new_string(script).e()?;
    let _ = env.call_method(
        &webview,
        "evaluateJavascript",
        "(Ljava/lang/String;Landroid/webkit/ValueCallback;)V",
        &[JValue::Object(&jscript), JValue::Object(&JObject::null())],
    );
    let _ = env.exception_clear();
    std::mem::forget(webview); // don't drop the borrowed ref
    Ok(())
}

pub fn dismiss(handle: WebViewHandle) -> Result<(), String> {
    let mut env = jni_helpers::obtain_env()?;
    let webview = unsafe { JObject::from_raw(handle.ptr as jni::sys::jobject) };

    // Get parent ViewGroup and remove the webview
    let parent = env
        .call_method(
            &webview,
            "getParent",
            "()Landroid/view/ViewParent;",
            &[],
        )
        .and_then(|v| v.l());

    if let Ok(parent) = parent {
        if !parent.is_null() {
            let _ = env.call_method(
                &parent,
                "removeView",
                "(Landroid/view/View;)V",
                &[JValue::Object(&webview)],
            );
            let _ = env.exception_clear();
        }
    }

    // webview.destroy()
    let _ = env.call_method(&webview, "destroy", "()V", &[]);
    let _ = env.exception_clear();

    // Delete the global ref via raw JNI (GlobalRef::from_raw is private in jni 0.21)
    unsafe {
        let raw_env = env.get_raw();
        (**raw_env).DeleteGlobalRef.unwrap()(raw_env, handle.ptr as jni::sys::jobject);
    }

    Ok(())
}

fn configure_webview(
    env: &mut jni::JNIEnv<'_>,
    webview: &JObject<'_>,
    settings: &WebViewSettings,
) -> Result<(), String> {
    // WebSettings ws = webview.getSettings();
    let ws = env
        .call_method(
            webview,
            "getSettings",
            "()Landroid/webkit/WebSettings;",
            &[],
        )
        .and_then(|v| v.l())
        .e()?;

    // ws.setJavaScriptEnabled(...)
    let _ = env.call_method(
        &ws,
        "setJavaScriptEnabled",
        "(Z)V",
        &[JValue::Bool(settings.javascript_enabled as u8)],
    );

    // ws.setDomStorageEnabled(...)
    let _ = env.call_method(
        &ws,
        "setDomStorageEnabled",
        "(Z)V",
        &[JValue::Bool(settings.dom_storage_enabled as u8)],
    );

    // ws.setSupportZoom(...)
    let _ = env.call_method(
        &ws,
        "setSupportZoom",
        "(Z)V",
        &[JValue::Bool(settings.zoom_enabled as u8)],
    );

    // User agent
    if let Some(ref ua) = settings.user_agent {
        if let Ok(jua) = env.new_string(ua) {
            let _ = env.call_method(
                &ws,
                "setUserAgentString",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&jua)],
            );
        }
    }

    let _ = env.exception_clear();
    Ok(())
}

fn add_to_content_view(
    env: &mut jni::JNIEnv<'_>,
    activity: &JObject<'_>,
    webview: &JObject<'_>,
) -> Result<(), String> {
    // FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT)
    let params = env
        .new_object(
            "android/widget/FrameLayout$LayoutParams",
            "(II)V",
            &[JValue::Int(-1), JValue::Int(-1)], // MATCH_PARENT = -1
        )
        .e()?;

    // activity.addContentView(webview, params)
    let _ = env.call_method(
        activity,
        "addContentView",
        "(Landroid/view/View;Landroid/view/ViewGroup$LayoutParams;)V",
        &[JValue::Object(webview), JValue::Object(&params)],
    );
    let _ = env.exception_clear();
    Ok(())
}
