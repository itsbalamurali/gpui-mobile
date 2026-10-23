//! Android Storage Access Framework bridge returning readable local files.

use super::jni::{self as jni_helpers, JniExt};
use anyhow::{anyhow, Result};
use futures::channel::oneshot;
use gpui::PathPromptOptions;
use jni::objects::{JObject, JObjectArray, JValue};
use std::path::PathBuf;

pub(super) fn prompt(
    options: PathPromptOptions,
) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
    let (sender, receiver) = oneshot::channel();
    // Waiting for an Activity result on GPUI's native event thread deadlocks
    // Android's pause/resume handshakes. The Java helper waits only on this worker.
    std::thread::spawn(move || {
        let result = if !options.files || options.directories {
            Err(anyhow!(
                "Android file prompts support files only; directory URIs are not filesystem paths"
            ))
        } else {
            pick_files(options.multiple).map_err(|error| anyhow!(error))
        };
        let _ = sender.send(result);
    });
    receiver
}

fn pick_files(multiple: bool) -> std::result::Result<Option<Vec<PathBuf>>, String> {
    jni_helpers::with_env(|env| {
        let activity = jni_helpers::activity(env)?;
        let class = jni_helpers::find_app_class(env, "dev.gpui.mobile.GpuiPathPicker")?;
        let result = env
            .call_static_method(
                &class,
                jni::jni_str!("openFiles"),
                jni::jni_sig!("(Landroid/app/Activity;Z)[Ljava/lang/String;"),
                &[JValue::Object(&activity), JValue::Bool(multiple)],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if result.is_null() {
            return Ok(None);
        }
        let array = unsafe { JObjectArray::<JObject>::from_raw(env, result.as_raw()) };
        let mut paths = Vec::with_capacity(array.len(env).e()?);
        for index in 0..array.len(env).e()? {
            let path: JObject = array.get_element(env, index).e()?;
            paths.push(PathBuf::from(jni_helpers::get_string(env, &path)));
        }
        Ok((!paths.is_empty()).then_some(paths))
    })
}
