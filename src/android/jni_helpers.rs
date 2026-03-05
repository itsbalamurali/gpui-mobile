//! Safe JNI helpers using the `jni` crate.
//!
//! Provides [`obtain_env`] and [`activity`] that replace the old raw
//! function-table wrappers with the safe `jni` crate API.

#![allow(unsafe_code)]

use jni::objects::{JObject, JString};
use jni::{JNIEnv, JavaVM};
use std::sync::OnceLock;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

/// Get or create the static `JavaVM` wrapper.
fn java_vm_safe() -> Result<&'static JavaVM, String> {
    if let Some(vm) = JAVA_VM.get() {
        return Ok(vm);
    }
    let ptr = super::jni::java_vm();
    if ptr.is_null() {
        return Err("JavaVM not available".into());
    }
    Ok(JAVA_VM.get_or_init(|| unsafe {
        JavaVM::from_raw(ptr as *mut jni::sys::JavaVM).expect("Invalid JavaVM pointer")
    }))
}

/// Attach the current thread to the JVM and return a `JNIEnv`.
///
/// The returned [`jni::AttachGuard`] auto-detaches the thread when dropped.
pub fn obtain_env() -> Result<jni::AttachGuard<'static>, String> {
    let vm = java_vm_safe()?;
    vm.attach_current_thread().map_err(|e| e.to_string())
}

/// Get the Activity as a [`JObject`].
///
/// `activity_as_ptr()` returns a JNI global reference from `android-activity`
/// that is valid for the lifetime of the app. We wrap it in a `JObject`
/// without taking ownership (JObject has no Drop).
pub fn activity() -> Result<JObject<'static>, String> {
    let ptr = super::jni::activity_as_ptr();
    if ptr.is_null() {
        return Err("Activity not available".into());
    }
    Ok(unsafe { JObject::from_raw(ptr as jni::sys::jobject) })
}

/// Convert a Java String (`JObject` wrapping a `java.lang.String`) to a Rust `String`.
///
/// Returns an empty string on null or error.
pub fn get_string(env: &mut JNIEnv<'_>, obj: &JObject<'_>) -> String {
    if obj.is_null() {
        return String::new();
    }
    let jstr = unsafe { JString::from_raw(obj.as_raw()) };
    let result = match env.get_string(&jstr) {
        Ok(s) => s.into(),
        Err(_) => {
            let _ = env.exception_clear();
            String::new()
        }
    };
    // Prevent JString from interfering with the raw jobject lifetime.
    // JString has no Drop, but we explicitly forget for clarity.
    std::mem::forget(jstr);
    result
}

/// Extension trait for converting `jni::errors::Result<T>` to `Result<T, String>`.
pub(crate) trait JniExt<T> {
    fn e(self) -> Result<T, String>;
}

impl<T> JniExt<T> for jni::errors::Result<T> {
    fn e(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
