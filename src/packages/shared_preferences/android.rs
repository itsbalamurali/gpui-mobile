use crate::android::jni_helpers::{self, get_string, JniExt};
use jni::objects::{JObject, JValue};

pub struct AndroidSharedPreferences;

impl AndroidSharedPreferences {
    pub fn new() -> Self {
        Self
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        let mut env = jni_helpers::obtain_env().ok()?;
        let prefs = get_default_prefs(&mut env)?;

        let jkey = env.new_string(key).ok()?;
        let result = env
            .call_method(
                &prefs,
                "getString",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&jkey), JValue::Object(&JObject::null())],
            )
            .and_then(|v| v.l())
            .ok()?;

        if result.is_null() {
            None
        } else {
            Some(get_string(&mut env, &result))
        }
    }

    pub fn set_string(&self, key: &str, value: &str) -> Result<(), String> {
        with_editor(|env, editor| {
            let jkey = env.new_string(key).e()?;
            let jval = env.new_string(value).e()?;
            let _ = env.call_method(
                editor,
                "putString",
                "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
                &[JValue::Object(&jkey), JValue::Object(&jval)],
            );
            Ok(())
        })
    }

    pub fn get_int(&self, key: &str) -> Option<i64> {
        let mut env = jni_helpers::obtain_env().ok()?;
        let prefs = get_default_prefs(&mut env)?;

        if !self.contains_key_jni(&mut env, &prefs, key) {
            return None;
        }
        let jkey = env.new_string(key).ok()?;
        env.call_method(
            &prefs,
            "getLong",
            "(Ljava/lang/String;J)J",
            &[JValue::Object(&jkey), JValue::Long(0)],
        )
        .and_then(|v| v.j())
        .ok()
    }

    pub fn set_int(&self, key: &str, value: i64) -> Result<(), String> {
        with_editor(|env, editor| {
            let jkey = env.new_string(key).e()?;
            let _ = env.call_method(
                editor,
                "putLong",
                "(Ljava/lang/String;J)Landroid/content/SharedPreferences$Editor;",
                &[JValue::Object(&jkey), JValue::Long(value)],
            );
            Ok(())
        })
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        let mut env = jni_helpers::obtain_env().ok()?;
        let prefs = get_default_prefs(&mut env)?;

        if !self.contains_key_jni(&mut env, &prefs, key) {
            return None;
        }
        let jkey = env.new_string(key).ok()?;
        env.call_method(
            &prefs,
            "getBoolean",
            "(Ljava/lang/String;Z)Z",
            &[JValue::Object(&jkey), JValue::Bool(0)],
        )
        .and_then(|v| v.z())
        .ok()
    }

    pub fn set_bool(&self, key: &str, value: bool) -> Result<(), String> {
        with_editor(|env, editor| {
            let jkey = env.new_string(key).e()?;
            let _ = env.call_method(
                editor,
                "putBoolean",
                "(Ljava/lang/String;Z)Landroid/content/SharedPreferences$Editor;",
                &[JValue::Object(&jkey), JValue::Bool(value as u8)],
            );
            Ok(())
        })
    }

    pub fn remove(&self, key: &str) -> Result<(), String> {
        with_editor(|env, editor| {
            let jkey = env.new_string(key).e()?;
            let _ = env.call_method(
                editor,
                "remove",
                "(Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
                &[JValue::Object(&jkey)],
            );
            Ok(())
        })
    }

    pub fn clear(&self) -> Result<(), String> {
        with_editor(|env, editor| {
            let _ = env.call_method(
                editor,
                "clear",
                "()Landroid/content/SharedPreferences$Editor;",
                &[],
            );
            Ok(())
        })
    }

    pub fn contains_key(&self, key: &str) -> bool {
        let mut env = match jni_helpers::obtain_env() {
            Ok(e) => e,
            Err(_) => return false,
        };
        let prefs = match get_default_prefs(&mut env) {
            Some(p) => p,
            None => return false,
        };
        self.contains_key_jni(&mut env, &prefs, key)
    }

    fn contains_key_jni(
        &self,
        env: &mut jni::JNIEnv<'_>,
        prefs: &JObject<'_>,
        key: &str,
    ) -> bool {
        let jkey = match env.new_string(key) {
            Ok(k) => k,
            Err(_) => return false,
        };
        env.call_method(
            prefs,
            "contains",
            "(Ljava/lang/String;)Z",
            &[JValue::Object(&jkey)],
        )
        .and_then(|v| v.z())
        .unwrap_or(false)
    }
}

/// Get default SharedPreferences via PreferenceManager.
fn get_default_prefs<'local>(
    env: &mut jni::JNIEnv<'local>,
) -> Option<JObject<'local>> {
    let activity = jni_helpers::activity().ok()?;
    let prefs = env
        .call_static_method(
            "android/preference/PreferenceManager",
            "getDefaultSharedPreferences",
            "(Landroid/content/Context;)Landroid/content/SharedPreferences;",
            &[JValue::Object(&activity)],
        )
        .and_then(|v| v.l())
        .ok()?;
    if prefs.is_null() { None } else { Some(prefs) }
}

/// Get an editor, run the callback, then commit.
fn with_editor(
    f: impl FnOnce(&mut jni::JNIEnv<'_>, &JObject<'_>) -> Result<(), String>,
) -> Result<(), String> {
    let mut env = jni_helpers::obtain_env()?;
    let prefs = get_default_prefs(&mut env)
        .ok_or_else(|| "Failed to get SharedPreferences".to_string())?;

    let editor = env
        .call_method(
            &prefs,
            "edit",
            "()Landroid/content/SharedPreferences$Editor;",
            &[],
        )
        .and_then(|v| v.l())
        .e()?;
    if editor.is_null() {
        return Err("edit() returned null".into());
    }

    f(&mut env, &editor)?;

    // Commit
    let _ = env.call_method(&editor, "commit", "()Z", &[]);
    Ok(())
}
