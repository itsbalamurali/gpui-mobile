//! APK asset loading shared by NativeActivity and host-driven Activity entry points.
use anyhow::{anyhow, Result};
use ndk::asset::AssetManager;
use std::{ffi::CString, ptr::NonNull};

/// Read an APK asset while keeping its Java AssetManager alive for the whole read.
/// `path` is relative to the APK assets directory. Missing assets are errors.
pub(crate) fn load_asset_bytes(path: &str) -> Result<Vec<u8>> {
    let path = CString::new(path)?;
    if let Some(app) = super::jni::android_app() {
        return read_asset(&app.asset_manager(), &path);
    }
    super::jni::with_env(|env| {
        let activity = super::jni::activity(env)?;
        let java_manager = env
            .call_method(
                &activity,
                jni::jni_str!("getAssets"),
                jni::jni_sig!("()Landroid/content/res/AssetManager;"),
                &[],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        // The local JNI reference keeps the Java manager alive until read_asset
        // has copied the data. No native pointer escapes this attached frame.
        let pointer = unsafe {
            ndk_sys::AAssetManager_fromJava(env.get_raw().cast(), java_manager.as_raw().cast())
        };
        let pointer = NonNull::new(pointer).ok_or("Activity has no AssetManager")?;
        let manager = unsafe { AssetManager::from_ptr(pointer) };
        read_asset(&manager, &path).map_err(|error| error.to_string())
    })
    .map_err(|error| anyhow!(error))
}

fn read_asset(manager: &AssetManager, path: &std::ffi::CStr) -> Result<Vec<u8>> {
    let mut asset = manager
        .open(path)
        .ok_or_else(|| anyhow!("APK asset not found: {}", path.to_string_lossy()))?;
    Ok(asset.buffer()?.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_embedded_nul_before_using_the_vm() {
        assert!(load_asset_bytes("fonts/invalid\0.ttf").is_err());
    }
    #[test]
    fn missing_entry_point_is_a_recoverable_error() {
        assert!(load_asset_bytes("fonts/NotoColorEmoji.ttf").is_err());
    }
}
