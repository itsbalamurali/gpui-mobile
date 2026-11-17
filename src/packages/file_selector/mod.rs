//! File selector for picking files and directories.
//!
//! Provides a cross-platform file picker API backed by:
//! - iOS: `UIDocumentPickerViewController` via Objective-C
//! - Android: Storage Access Framework via JNI
//!
//! Inspired by [file_selector](https://pub.dev/packages/file_selector).
//!
//! Feature-gated behind `file_selector`.
//!
//! Synchronous iOS picker calls must run on the main thread and service its run
//! loop until completion. The internal GPUI bridge starts the picker on a main
//! queue turn and completes asynchronously. Overlapping presentations return an
//! error. Android calls must run off the UI thread. iOS file selections are
//! imported copies.

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "ios")]
mod ios;

/// A group of file type filters.
///
/// At least one of `extensions`, `mime_types`, or `utis` should be non-empty.
#[derive(Debug, Clone, Default)]
pub struct TypeGroup {
    /// Human-readable label for this group (e.g. "Images").
    pub label: String,
    /// File extensions without the dot (e.g. `["jpg", "png"]`).
    pub extensions: Vec<String>,
    /// MIME types (e.g. `["image/jpeg", "image/png"]`).
    pub mime_types: Vec<String>,
    /// iOS Uniform Type Identifiers (e.g. `["public.jpeg", "public.png"]`).
    /// If empty, `extensions` and `mime_types` are used to infer UTIs.
    pub utis: Vec<String>,
}

/// Options for opening a file.
#[derive(Debug, Clone, Default)]
pub struct OpenFileOptions {
    /// Accepted file type groups. Empty means all files.
    pub accept_type_groups: Vec<TypeGroup>,
    /// Initial directory to open the picker in (platform hint, may be ignored).
    pub initial_directory: Option<String>,
}

/// Options for getting a save path.
#[derive(Debug, Clone, Default)]
pub struct SaveFileOptions {
    /// Accepted file type groups.
    pub accept_type_groups: Vec<TypeGroup>,
    /// Initial directory (platform hint).
    pub initial_directory: Option<String>,
    /// Suggested file name.
    pub suggested_name: Option<String>,
}

#[cfg(any(target_os = "ios", test))]
pub(crate) struct TempExport {
    directory: std::path::PathBuf,
    path: std::path::PathBuf,
}

#[cfg(any(target_os = "ios", test))]
impl TempExport {
    pub(crate) fn new(suggested_name: Option<&str>, contents: &[u8]) -> Result<Self, String> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NONCE: AtomicU64 = AtomicU64::new(0);

        let root = std::env::temp_dir();
        for _ in 0..16 {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|time| time.as_nanos())
                .unwrap_or(0);
            let directory = root.join(format!(
                "gpui-export-{}-{stamp}-{}",
                std::process::id(),
                NONCE.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&directory) {
                Ok(()) => {
                    let path = directory.join(safe_export_name(suggested_name));
                    if let Err(error) = std::fs::write(&path, contents) {
                        let _ = std::fs::remove_file(&path);
                        let _ = std::fs::remove_dir(&directory);
                        return Err(format!("Write export contents: {error}"));
                    }
                    return Ok(Self { directory, path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("Create export directory: {error}")),
            }
        }
        Err("Could not allocate a unique export directory".into())
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(any(target_os = "ios", test))]
impl Drop for TempExport {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

#[cfg(any(target_os = "ios", test))]
fn safe_export_name(suggested_name: Option<&str>) -> String {
    let name: String = suggested_name
        .unwrap_or("untitled")
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if name.is_empty() || name == "." || name == ".." {
        "untitled".into()
    } else {
        name
    }
}

/// A file selected by the user.
#[derive(Debug, Clone)]
pub struct SelectedFile {
    /// Absolute path or content URI of the file.
    pub path: String,
    /// Display name of the file.
    pub name: String,
}

/// A directory selected through the iOS document picker.
///
/// The picker returns a security-scoped URL. This handle starts access before
/// it is returned and stops access when the handle is dropped. Keep the handle
/// alive for every read or write below `path()`. Cloning the path
/// does not extend access, and this API never creates a persistent bookmark.
#[cfg(target_os = "ios")]
pub struct SecurityScopedDirectory {
    path: std::path::PathBuf,
    url: objc2::rc::Retained<objc2::runtime::AnyObject>,
}

#[cfg(target_os = "ios")]
impl SecurityScopedDirectory {
    pub(crate) fn new(
        path: std::path::PathBuf,
        url: objc2::rc::Retained<objc2::runtime::AnyObject>,
    ) -> Self {
        Self { path, url }
    }

    /// The selected directory while this handle keeps security scope open.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(target_os = "ios")]
impl Drop for SecurityScopedDirectory {
    fn drop(&mut self) {
        unsafe {
            let _: () = objc2::msg_send![&*self.url, stopAccessingSecurityScopedResource];
        }
    }
}

/// Open a file picker to select a single file.
///
/// Returns `Ok(None)` if the user cancelled.
pub fn open_file(options: &OpenFileOptions) -> Result<Option<SelectedFile>, String> {
    #[cfg(target_os = "ios")]
    {
        ios::open_file(options)
    }
    #[cfg(target_os = "android")]
    {
        android::open_file(options)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let _ = options;
        Err("file_selector is only available on iOS and Android".into())
    }
}

/// Open a file picker to select multiple files.
///
/// Returns an empty Vec if the user cancelled.
pub fn open_files(options: &OpenFileOptions) -> Result<Vec<SelectedFile>, String> {
    #[cfg(target_os = "ios")]
    {
        ios::open_files(options)
    }
    #[cfg(target_os = "android")]
    {
        android::open_files(options)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let _ = options;
        Err("file_selector is only available on iOS and Android".into())
    }
}

/// Open a save dialog to get a file path from the user.
///
/// Returns `Ok(None)` if the user cancelled.
/// Android returns a content URI, which must be written via ContentResolver.
/// iOS returns an error: native export requires existing file contents, which
/// this path-only API does not accept.
pub fn get_save_path(options: &SaveFileOptions) -> Result<Option<String>, String> {
    #[cfg(target_os = "ios")]
    {
        ios::get_save_path(options)
    }
    #[cfg(target_os = "android")]
    {
        android::get_save_path(options)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let _ = options;
        Err("file_selector is only available on iOS and Android".into())
    }
}

/// Export existing contents through the native save picker.
///
/// Returns `Ok(None)` if the user cancelled. The contents are written before
/// this function returns; the temporary source is removed on cancellation or
/// error. Android returns a content URI with the picker's persisted write grant.
/// iOS returns the chosen destination path for display only; the destination is
/// outside the app sandbox, and this API does not retain security-scoped access
/// after the picker closes, so later reads or writes are not guaranteed.
pub fn export_file(
    options: &SaveFileOptions,
    contents: &[u8],
) -> Result<Option<SelectedFile>, String> {
    #[cfg(target_os = "ios")]
    {
        ios::export_file(options, contents)
    }
    #[cfg(target_os = "android")]
    {
        android::export_file(options, contents)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let _ = (options, contents);
        Err("file_selector is only available on iOS and Android".into())
    }
}

/// Open a directory picker.
///
/// Returns `Ok(None)` if the user cancelled. Android returns a content URI.
/// iOS returns an error because a bare path cannot carry security-scoped access;
/// use `open_directory` and keep its handle alive during I/O.
pub fn get_directory_path(initial_directory: Option<&str>) -> Result<Option<String>, String> {
    #[cfg(target_os = "ios")]
    {
        ios::get_directory_path(initial_directory)
    }
    #[cfg(target_os = "android")]
    {
        android::get_directory_path(initial_directory)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let _ = initial_directory;
        Err("file_selector is only available on iOS and Android".into())
    }
}

/// Open the iOS directory picker and return a lifetime-bounded access handle.
///
/// Access starts only after the picker returns one folder URL. Cancellation
/// returns `Ok(None)`, and dropping the handle stops the security scope. The
/// returned path is valid only while the handle is alive and is not persisted.
#[cfg(target_os = "ios")]
pub fn open_directory() -> Result<Option<SecurityScopedDirectory>, String> {
    ios::open_directory()
}

/// Reject paths that cannot be the selected directory. Call this after starting
/// security-scoped access so provider-backed folders can be checked.
#[cfg(any(target_os = "ios", test))]
pub(crate) fn validate_directory_path(path: &std::path::Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Directory picker returned a relative path".into());
    }
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err("Directory picker returned a file".into()),
        Err(error) => Err(format!("Selected directory is not accessible: {error}")),
    }
}

/// Internal GPUI adapter: return usable filesystem paths, never SAF URIs.
pub(crate) fn prompt_paths(
    options: gpui::PathPromptOptions,
) -> Result<Option<Vec<std::path::PathBuf>>, String> {
    if !options.files || options.directories {
        return Err("Mobile GPUI path prompts support files only; directory grants are not filesystem paths".into());
    }
    let files = if options.multiple {
        open_files(&OpenFileOptions::default())?
    } else {
        open_file(&OpenFileOptions::default())?
            .into_iter()
            .collect()
    };
    if files.is_empty() {
        return Ok(None);
    }
    let mut paths = Vec::with_capacity(files.len());
    for file in files {
        let path = std::path::PathBuf::from(file.path);
        if !path.is_absolute() || !path.is_file() {
            return Err("Picker did not return an imported filesystem file".into());
        }
        paths.push(path);
    }
    Ok(Some(paths))
}

#[cfg(any(target_os = "ios", test))]
fn discard_import_paths(paths: &[std::path::PathBuf]) {
    #[cfg(target_os = "ios")]
    ios::discard_paths(paths);
    #[cfg(not(target_os = "ios"))]
    for path in paths {
        let _ = std::fs::remove_file(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

#[cfg(any(target_os = "ios", test))]
type PromptSender =
    futures::channel::oneshot::Sender<gpui::Result<Option<Vec<std::path::PathBuf>>>>;

#[cfg(target_os = "ios")]
struct QueuedPathPrompt {
    multiple: bool,
    sender: PromptSender,
}

#[cfg(any(target_os = "ios", test))]
fn finish_async_prompt(
    sender: &std::sync::Arc<std::sync::Mutex<Option<PromptSender>>>,
    result: Result<Option<Vec<std::path::PathBuf>>, String>,
) {
    let sender = sender.lock().ok().and_then(|mut sender| sender.take());
    if let Some(sender) = sender {
        let result = result.map_err(anyhow::Error::msg);
        if let Err(result) = sender.send(result) {
            if let Ok(Some(paths)) = result {
                discard_import_paths(&paths);
            }
        }
    } else {
        if let Ok(Some(paths)) = result {
            discard_import_paths(&paths);
        }
    }
}

#[cfg(any(target_os = "ios", test))]
fn prompt_receiver_closed<T>(
    sender: &std::sync::Arc<std::sync::Mutex<Option<futures::channel::oneshot::Sender<T>>>>,
) -> bool {
    sender
        .lock()
        .ok()
        .and_then(|sender| sender.as_ref().map(|sender| sender.is_canceled()))
        .unwrap_or(true)
}

#[cfg(target_os = "ios")]
unsafe extern "C" fn start_async_prompt(context: *mut std::ffi::c_void) {
    let queued = Box::from_raw(context.cast::<QueuedPathPrompt>());
    let QueuedPathPrompt { multiple, sender } = *queued;
    if sender.is_canceled() {
        return;
    }
    let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
    let callback_sender = sender.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let session = match ios::begin_prompt(&OpenFileOptions::default(), multiple) {
            Ok(session) => session,
            Err(error) => {
                finish_async_prompt(&callback_sender, Err(error));
                return;
            }
        };
        if prompt_receiver_closed(&callback_sender) {
            session.cancel("Document picker request was dropped".into());
            return;
        }
        let session_owner = std::sync::Arc::new(std::sync::Mutex::new(Some(session)));
        let worker_session = session_owner.clone();
        let worker_sender = callback_sender.clone();
        let worker = std::thread::Builder::new()
            .name("gpui-document-picker".into())
            .spawn(move || {
                let session = worker_session
                    .lock()
                    .ok()
                    .and_then(|mut session| session.take());
                let Some(session) = session else {
                    finish_async_prompt(
                        &worker_sender,
                        Err("Document picker session ownership was lost".into()),
                    );
                    return;
                };
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        match session.wait_cancellable(|| prompt_receiver_closed(&worker_sender)) {
                            ios::PromptWaitResult::Complete(result) => {
                                Some(result.map(|paths| (!paths.is_empty()).then_some(paths)))
                            }
                            ios::PromptWaitResult::Cancelled => None,
                        }
                    }))
                    .unwrap_or_else(|_| Some(Err("Document picker worker panicked".into())));
                if let Some(result) = result {
                    finish_async_prompt(&worker_sender, result);
                }
            });
        if let Err(error) = worker {
            if let Some(session) = session_owner
                .lock()
                .ok()
                .and_then(|mut session| session.take())
            {
                session.cancel(format!("Start document picker worker: {error}"));
            }
            finish_async_prompt(
                &callback_sender,
                Err(format!("Start document picker worker: {error}")),
            );
        }
    }));
    if outcome.is_err() {
        finish_async_prompt(&sender, Err("Document picker startup panicked".into()));
    }
}

/// Start an iOS picker from a normal main-run-loop turn and complete the GPUI
/// receiver directly. The synchronous public API remains available for callers
/// that require it.
#[cfg(target_os = "ios")]
pub(crate) fn prompt_paths_nonblocking(options: gpui::PathPromptOptions, sender: PromptSender) {
    if !options.files || options.directories {
        let _ = sender.send(Err(anyhow::Error::msg(
            "Mobile GPUI path prompts support files only; directory grants are not filesystem paths",
        )));
        return;
    }
    let queued = Box::new(QueuedPathPrompt {
        multiple: options.multiple,
        sender,
    });
    unsafe {
        crate::ios::dispatch_async_f(
            crate::ios::dispatch_get_main_queue(),
            Box::into_raw(queued).cast(),
            Some(start_async_prompt),
        );
    }
}

#[cfg(target_os = "ios")]
pub(crate) fn discard_paths(paths: &[std::path::PathBuf]) {
    discard_import_paths(paths);
}

pub(crate) fn prompt_new_path(
    _directory: &std::path::Path,
    _suggested_name: Option<&str>,
) -> Result<Option<std::path::PathBuf>, String> {
    Err("Mobile save dialogs require export contents or a ContentResolver URI; GPUI's filesystem PathBuf cannot safely represent that contract".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_names_cannot_escape_the_temporary_directory() {
        assert_eq!(
            safe_export_name(Some("report/2026\\q1.txt")),
            "report_2026_q1.txt"
        );
        assert_eq!(safe_export_name(Some("a\0b.txt")), "a_b.txt");
        assert_eq!(safe_export_name(Some(".")), "untitled");
        assert_eq!(safe_export_name(Some("..")), "untitled");
        assert_eq!(safe_export_name(None), "untitled");
    }

    #[test]
    fn temporary_export_is_removed_after_success_or_cancellation() {
        let export = TempExport::new(Some("report.txt"), b"existing contents").unwrap();
        let directory = export.path().parent().unwrap().to_path_buf();
        assert_eq!(std::fs::read(export.path()).unwrap(), b"existing contents");
        drop(export);
        assert!(!directory.exists());
    }

    #[test]
    fn directory_results_are_never_bare_paths() {
        assert!(get_directory_path(None).is_err());
        let options = gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        };
        assert!(prompt_paths(options).is_err());
    }

    #[test]
    fn directory_validation_requires_an_existing_absolute_directory() {
        static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "gpui-directory-validation-{}-{}",
            std::process::id(),
            NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("file.txt");
        std::fs::write(&file, b"not a directory").unwrap();

        assert!(validate_directory_path(&root).is_ok());
        assert!(validate_directory_path(&file).is_err());
        assert!(validate_directory_path(std::path::Path::new("relative")).is_err());
        assert!(validate_directory_path(&root.join("missing")).is_err());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dropping_unconsumed_prompt_result_removes_imports() {
        let root = std::env::temp_dir().join(format!(
            "gpui-async-prompt-drop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("import.txt");
        std::fs::write(&file, "owned").unwrap();

        let (sender, receiver) =
            futures::channel::oneshot::channel::<gpui::Result<Option<Vec<std::path::PathBuf>>>>();
        drop(receiver);
        let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
        finish_async_prompt(&sender, Ok(Some(vec![file])));
        assert!(!root.exists());
    }

    #[test]
    fn receiver_drop_is_visible_before_picker_start() {
        let (sender, receiver) = futures::channel::oneshot::channel::<()>();
        let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
        assert!(!prompt_receiver_closed(&sender));
        drop(receiver);
        assert!(prompt_receiver_closed(&sender));
    }

    #[test]
    fn live_prompt_receiver_receives_the_import() {
        let root = std::env::temp_dir().join(format!(
            "gpui-direct-prompt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("import.txt");
        std::fs::write(&file, "owned").unwrap();

        let (sender, mut receiver) =
            futures::channel::oneshot::channel::<gpui::Result<Option<Vec<std::path::PathBuf>>>>();
        let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
        finish_async_prompt(&sender, Ok(Some(vec![file.clone()])));

        assert_eq!(
            receiver.try_recv().unwrap().unwrap().unwrap(),
            Some(vec![file])
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
