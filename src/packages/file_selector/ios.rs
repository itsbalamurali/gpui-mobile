use super::{
    OpenFileOptions, SaveFileOptions, SecurityScopedDirectory, SelectedFile, TempExport, TypeGroup,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};
use std::cell::RefCell;
use std::sync::mpsc;
use std::sync::Once;

#[path = "../permission_handler/ios_completion.rs"]
mod completion;
#[path = "ios_delegate.rs"]
mod delegate;
#[path = "request.rs"]
mod request;

#[link(name = "UIKit", kind = "framework")]
extern "C" {}

#[link(name = "UniformTypeIdentifiers", kind = "framework")]
extern "C" {}

// ── Result channel ──────────────────────────────────────────────────────────

static REQUEST: request::Slot = request::Slot::new();

thread_local! {
    // The document picker delegate and its completion run on the main thread.
    static DIRECTORY_RESULT: RefCell<Option<mpsc::Sender<SecurityScopedDirectory>>> =
        const { RefCell::new(None) };
}

struct DirectoryMailbox {
    receiver: mpsc::Receiver<SecurityScopedDirectory>,
}

impl DirectoryMailbox {
    fn install() -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        DIRECTORY_RESULT.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_some() {
                return Err("A directory request is already active".into());
            }
            *slot = Some(sender);
            Ok(Self { receiver })
        })
    }

    fn take(&self) -> Result<Option<SecurityScopedDirectory>, String> {
        match self.receiver.try_recv() {
            Ok(directory) => Ok(Some(directory)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Directory result channel disconnected".into())
            }
        }
    }
}

impl Drop for DirectoryMailbox {
    fn drop(&mut self) {
        DIRECTORY_RESULT.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

fn set_result_channel() -> Result<request::Request<'static>, String> {
    let main: objc2::runtime::Bool = unsafe { msg_send![class!(NSThread), isMainThread] };
    if !main.as_bool() {
        return Err("Call the iOS document picker on the main thread".into());
    }
    REQUEST.begin()
}

fn set_export_result_channel() -> Result<request::Request<'static>, String> {
    let main: objc2::runtime::Bool = unsafe { msg_send![class!(NSThread), isMainThread] };
    if !main.as_bool() {
        return Err("Call the iOS document picker on the main thread".into());
    }
    REQUEST.begin_export()
}

// ── ObjC delegate class ─────────────────────────────────────────────────────

static REGISTER_DELEGATE: Once = Once::new();
static mut DELEGATE_CLASS: *const AnyClass = std::ptr::null();

fn delegate_class() -> &'static AnyClass {
    REGISTER_DELEGATE.call_once(|| {
        let superclass = class!(NSObject);
        let mut decl = ClassBuilder::new(c"GpuiDocumentPickerDelegate", superclass).unwrap();

        unsafe {
            delegate::register(&mut decl);
            // documentPicker:didPickDocumentsAtURLs:
            decl.add_method(
                sel!(documentPicker:didPickDocumentsAtURLs:),
                did_pick_documents
                    as unsafe extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject),
            );
            // documentPickerWasCancelled:
            decl.add_method(
                sel!(documentPickerWasCancelled:),
                did_cancel as unsafe extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
        }

        unsafe { DELEGATE_CLASS = decl.register() as *const AnyClass };
    });
    unsafe { &*DELEGATE_CLASS }
}

unsafe extern "C" fn did_pick_documents(
    this: *mut AnyObject,
    _: Sel,
    controller: *mut AnyObject,
    urls: *mut AnyObject,
) {
    let Some(token) = delegate::token(this) else {
        return;
    };
    if !token.claim() {
        return;
    }
    if delegate::is_directory(this) {
        let result = match deliver_directory(urls) {
            Ok(()) => Ok(vec![]),
            Err(error) => Err(error),
        };
        delegate::dismiss(controller, token, result, true, None);
        return;
    }
    if delegate::is_export(this) {
        let paths = match document_paths(urls) {
            Ok(paths) => paths,
            Err(error) => {
                delegate::dismiss(controller, token, Err(error), true, None);
                return;
            }
        };
        delegate::dismiss(controller, token, Ok(paths), true, None);
        return;
    }
    let selected = (|| -> Result<Vec<(std::path::PathBuf, ScopedUrl)>, String> {
        let count: usize = msg_send![urls, count];
        if count == 0 {
            return Err("Document picker returned no file URL".into());
        }
        let mut selected = Vec::with_capacity(count);
        for index in 0..count {
            let url: *mut AnyObject = msg_send![urls, objectAtIndex: index];
            selected.push(ScopedUrl::new(url)?);
        }
        Ok(selected)
    })();
    let selected = match selected {
        Ok(selected) => selected,
        Err(error) => {
            delegate::dismiss(controller, token, Err(error), true, None);
            return;
        }
    };

    if !token.begin_dismissal() {
        token.finish(Err(
            "Document picker closed before the import could start".into()
        ));
        return;
    }
    let _: () = msg_send![controller,
        dismissViewControllerAnimated: false,
        completion: std::ptr::null::<AnyObject>()
    ];

    let worker_token = token.clone();
    let worker = std::thread::Builder::new()
        .name("gpui-document-import".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                objc2::rc::autoreleasepool(|_| {
                    let mut imported = request::Imports::new();
                    for (path, scope) in selected {
                        imported.push(completion::import_file(&path)?);
                        drop(scope);
                    }
                    Ok(imported.take())
                })
            }))
            .unwrap_or_else(|_| Err("Document import worker panicked".into()));
            worker_token.finish(result);
        });
    if let Err(error) = worker {
        token.finish(Err(format!("Start document import worker: {error}")));
    }
}

struct ScopedUrl {
    url: objc2::rc::Retained<AnyObject>,
    opened: bool,
}

// NSURL is immutable and thread-safe. The worker owns this exact URL only to
// balance startAccessingSecurityScopedResource after the copy finishes.
unsafe impl Send for ScopedUrl {}

impl ScopedUrl {
    unsafe fn new(url: *mut AnyObject) -> Result<(std::path::PathBuf, Self), String> {
        let url = objc2::rc::Retained::retain(url)
            .ok_or_else(|| "Document picker returned an unreadable file URL".to_string())?;
        let is_file: Bool = msg_send![&*url, isFileURL];
        if !is_file.as_bool() {
            return Err("Document picker returned a non-file URL".into());
        }
        let value: *mut AnyObject = msg_send![&*url, path];
        if value.is_null() {
            return Err("Document picker returned an unreadable file path".into());
        }
        let bytes: *const std::ffi::c_char = msg_send![value, UTF8String];
        if bytes.is_null() {
            return Err("Document picker returned an unreadable file path".into());
        }
        let path = std::ffi::CStr::from_ptr(bytes).to_string_lossy();
        let opened: Bool = msg_send![&*url, startAccessingSecurityScopedResource];
        Ok((
            std::path::PathBuf::from(path.as_ref()),
            Self {
                url,
                opened: opened.as_bool(),
            },
        ))
    }
}

impl Drop for ScopedUrl {
    fn drop(&mut self) {
        if self.opened {
            let _: () = unsafe { msg_send![&*self.url, stopAccessingSecurityScopedResource] };
        }
    }
}

unsafe fn document_paths(urls: *mut AnyObject) -> Result<Vec<String>, String> {
    let count: usize = msg_send![urls, count];
    if count == 0 {
        return Err("Document picker returned no file URL".into());
    }
    let mut paths = Vec::with_capacity(count);
    for index in 0..count {
        let url: *mut AnyObject = msg_send![urls, objectAtIndex: index];
        paths.push(url_path(url)?);
    }
    Ok(paths)
}

unsafe fn url_path(url: *const AnyObject) -> Result<String, String> {
    if url.is_null() {
        return Err("Document picker returned an unreadable file URL".into());
    }
    let value: *mut AnyObject = msg_send![url, path];
    if value.is_null() {
        return Err("Document picker returned an unreadable file URL".into());
    }
    let bytes: *const std::ffi::c_char = msg_send![value, UTF8String];
    if bytes.is_null() {
        return Err("Document picker returned an unreadable file path".into());
    }
    Ok(std::ffi::CStr::from_ptr(bytes)
        .to_string_lossy()
        .into_owned())
}

/// Start access for the one selected folder and hand ownership to the waiting
/// request. Every failure path drops the handle, which stops a successful start.
unsafe fn deliver_directory(urls: *mut AnyObject) -> Result<(), String> {
    let count: usize = msg_send![urls, count];
    if count != 1 {
        return Err("Directory picker must return exactly one folder".into());
    }
    let url: *mut AnyObject = msg_send![urls, objectAtIndex: 0usize];
    let url = Retained::retain(url).ok_or("Directory picker returned no folder URL")?;
    let started: bool = msg_send![&*url, startAccessingSecurityScopedResource];
    if !started {
        return Err("iOS did not grant access to the selected directory".into());
    }
    let path = match url_path(Retained::as_ptr(&url)) {
        Ok(path) => std::path::PathBuf::from(path),
        Err(error) => {
            let _: () = msg_send![&*url, stopAccessingSecurityScopedResource];
            return Err(error);
        }
    };
    if let Err(error) = super::validate_directory_path(&path) {
        let _: () = msg_send![&*url, stopAccessingSecurityScopedResource];
        return Err(error);
    }
    let directory = SecurityScopedDirectory::new(path, url);
    match DIRECTORY_RESULT.with(|slot| slot.borrow_mut().take()) {
        Some(sender) => sender
            .send(directory)
            .map_err(|_| "Directory request was cancelled before delivery".to_string()),
        None => Err("No directory request is waiting".into()),
    }
}

unsafe extern "C" fn did_cancel(this: *mut AnyObject, _: Sel, controller: *mut AnyObject) {
    let Some(token) = delegate::token(this) else {
        return;
    };
    if token.claim() {
        delegate::dismiss(controller, token, Ok(vec![]), true, None);
    }
}

// ── UTType helpers ──────────────────────────────────────────────────────────

/// Convert a TypeGroup into an NSArray of UTType objects.
unsafe fn type_group_to_uttypes(group: &TypeGroup) -> *mut AnyObject {
    let mut types: Vec<*mut AnyObject> = Vec::new();

    // Prefer explicit UTIs
    for uti in &group.utis {
        let ut = uttype_from_identifier(uti);
        if !ut.is_null() {
            types.push(ut);
        }
    }

    // Fall back to extensions
    if types.is_empty() {
        for ext in &group.extensions {
            let ut = uttype_from_extension(ext);
            if !ut.is_null() {
                types.push(ut);
            }
        }
    }

    // Fall back to MIME types
    if types.is_empty() {
        for mime in &group.mime_types {
            let ut = uttype_from_mime(mime);
            if !ut.is_null() {
                types.push(ut);
            }
        }
    }

    if types.is_empty() {
        // Accept all file types
        let ut = uttype_from_identifier("public.item");
        if !ut.is_null() {
            types.push(ut);
        }
    }

    nsarray_from_objects(&types)
}

unsafe fn uttype_from_identifier(ident: &str) -> *mut AnyObject {
    let ns_ident = nsstring(ident);
    msg_send![class!(UTType), typeWithIdentifier: ns_ident]
}

unsafe fn uttype_from_extension(ext: &str) -> *mut AnyObject {
    let ns_ext = nsstring(ext);
    msg_send![class!(UTType), typeWithFilenameExtension: ns_ext]
}

unsafe fn uttype_from_mime(mime: &str) -> *mut AnyObject {
    let ns_mime = nsstring(mime);
    msg_send![class!(UTType), typeWithMIMEType: ns_mime]
}

use crate::ios::util::nsstring;

unsafe fn nsarray_from_objects(objects: &[*mut AnyObject]) -> *mut AnyObject {
    msg_send![class!(NSArray),
        arrayWithObjects: objects.as_ptr(),
        count: objects.len()
    ]
}

// ── Presentation helper ─────────────────────────────────────────────────────

unsafe fn present_picker(picker: *mut AnyObject) -> Result<(), String> {
    let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
    let key_window: *mut AnyObject = msg_send![app, keyWindow];
    if key_window.is_null() {
        return Err("No key window available".into());
    }
    let root_vc: *mut AnyObject = msg_send![key_window, rootViewController];
    if root_vc.is_null() {
        return Err("No root view controller".into());
    }
    let presented: *mut AnyObject = msg_send![root_vc, presentedViewController];
    if !presented.is_null() {
        return Err("Another view controller is already presented".into());
    }
    let _: () = msg_send![root_vc,
        presentViewController: picker,
        animated: true,
        completion: std::ptr::null::<AnyObject>()
    ];
    Ok(())
}

struct PendingPresentation {
    picker: *mut AnyObject,
    token: request::Token,
}

unsafe extern "C" fn present_picker_callback(context: *mut std::ffi::c_void) {
    let pending = Box::from_raw(context.cast::<PendingPresentation>());
    if !pending.token.is_open() {
        let _: () = msg_send![pending.picker, release];
        return;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        objc2::rc::autoreleasepool(|_| present_picker(pending.picker))
    }))
    .unwrap_or_else(|_| Err("Document picker presentation panicked".into()));
    let _: () = msg_send![pending.picker, release];
    if let Err(error) = result {
        pending.token.finish(Err(error));
    }
}

unsafe fn present_picker_async(picker: *mut AnyObject, token: request::Token) {
    let pending = Box::into_raw(Box::new(PendingPresentation { picker, token }));
    crate::ios::dispatch_async_f(
        crate::ios::dispatch_get_main_queue(),
        pending.cast(),
        Some(present_picker_callback),
    );
}

struct PreparedPicker {
    request: request::Request<'static>,
    picker: *mut AnyObject,
}

fn prepare_picker(options: &OpenFileOptions, multiple: bool) -> Result<PreparedPicker, String> {
    let request = set_result_channel()?;
    unsafe {
        let content_types = if options.accept_type_groups.is_empty() {
            let ut = uttype_from_identifier("public.item");
            nsarray_from_objects(&[ut])
        } else {
            let mut all_types: Vec<*mut AnyObject> = Vec::new();
            for group in &options.accept_type_groups {
                let arr = type_group_to_uttypes(group);
                let count: usize = msg_send![arr, count];
                for index in 0..count {
                    all_types.push(msg_send![arr, objectAtIndex: index]);
                }
            }
            nsarray_from_objects(&all_types)
        };

        let picker: *mut AnyObject = msg_send![class!(UIDocumentPickerViewController), alloc];
        let picker: *mut AnyObject =
            msg_send![picker, initForOpeningContentTypes: content_types, asCopy: false];
        if picker.is_null() {
            return Err("Failed to create UIDocumentPickerViewController".into());
        }
        let _: () = msg_send![picker,
            setAllowsMultipleSelection: objc2::runtime::Bool::from(multiple)
        ];

        let delegate_cls = delegate_class();
        let delegate: *mut AnyObject = msg_send![delegate_cls, alloc];
        let delegate: *mut AnyObject = msg_send![delegate, init];
        delegate::bind(picker, delegate, request.token.clone())?;
        Ok(PreparedPicker { request, picker })
    }
}

struct PickerController(*mut AnyObject);

// The retained UIKit controller is moved with the session, but it is only
// messaged or released in a main-queue callback.
unsafe impl Send for PickerController {}

impl Drop for PickerController {
    fn drop(&mut self) {
        unsafe {
            crate::ios::dispatch_async_f(
                crate::ios::dispatch_get_main_queue(),
                self.0.cast(),
                Some(release_picker_callback),
            );
        }
    }
}

unsafe extern "C" fn release_picker_callback(context: *mut std::ffi::c_void) {
    let picker = context.cast::<AnyObject>();
    let _: () = msg_send![picker, release];
}

pub(super) struct PromptSession {
    request: request::Request<'static>,
    multiple: bool,
    controller: PickerController,
}

impl PromptSession {
    pub(super) fn cancel(self, error: String) {
        let token = self.request.token.clone();
        if token.claim() && token.begin_dismissal() {
            let picker = self.controller.0;
            unsafe {
                delegate::dismiss(
                    picker,
                    token,
                    Err(error),
                    false,
                    Some(Box::new(move || drop(self))),
                )
            };
        } else {
            token.finish(Err(error));
        }
    }

    fn cancel_async(self, error: String) {
        let pending = Box::new(PendingCancellation {
            session: self,
            error,
        });
        unsafe {
            crate::ios::dispatch_async_f(
                crate::ios::dispatch_get_main_queue(),
                Box::into_raw(pending).cast(),
                Some(cancel_prompt_callback),
            );
        }
    }

    pub(super) fn wait_cancellable(self, cancelled: impl Fn() -> bool) -> PromptWaitResult {
        let result = wait_for_picker(&self.request.receiver, &cancelled);
        match result {
            Ok(result) => PromptWaitResult::Complete(
                result.and_then(|paths| picker_paths(self.multiple, paths)),
            ),
            Err(_error) if cancelled() => {
                self.cancel_async("Document picker request was dropped".into());
                PromptWaitResult::Cancelled
            }
            Err(error) => PromptWaitResult::Complete(Err(error)),
        }
    }
}

fn wait_for_picker(
    receiver: &std::sync::mpsc::Receiver<request::PickerResult>,
    cancelled: impl Fn() -> bool,
) -> Result<request::PickerResult, String> {
    completion::wait_cancellable(receiver, || !cancelled())
}

pub(super) enum PromptWaitResult {
    Complete(Result<Vec<std::path::PathBuf>, String>),
    Cancelled,
}

struct PendingCancellation {
    session: PromptSession,
    error: String,
}

unsafe extern "C" fn cancel_prompt_callback(context: *mut std::ffi::c_void) {
    let pending = Box::from_raw(context.cast::<PendingCancellation>());
    pending.session.cancel(pending.error);
}

fn picker_paths(multiple: bool, paths: Vec<String>) -> Result<Vec<std::path::PathBuf>, String> {
    if !multiple && paths.len() > 1 {
        request::discard(&paths);
        return Err("Single document picker returned multiple files".into());
    }
    let mut selected = Vec::with_capacity(paths.len());
    for path in &paths {
        let path = std::path::PathBuf::from(path.as_str());
        if !path.is_absolute() || !path.is_file() {
            request::discard(&paths);
            return Err("Picker did not return an imported filesystem file".into());
        }
        selected.push(path);
    }
    Ok(selected)
}

pub(super) fn begin_prompt(
    options: &OpenFileOptions,
    multiple: bool,
) -> Result<PromptSession, String> {
    let PreparedPicker { request, picker } = prepare_picker(options, multiple)?;
    if let Err(error) = unsafe { present_picker(picker) } {
        let _: () = unsafe { msg_send![picker, release] };
        return Err(error);
    }
    Ok(PromptSession {
        request,
        multiple,
        controller: PickerController(picker),
    })
}

pub(super) fn discard_paths(paths: &[std::path::PathBuf]) {
    let paths: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    request::discard(&paths);
}

fn url_to_selected_file(url_string: &str) -> SelectedFile {
    // Extract filename from URL path
    let name = url_string
        .rsplit('/')
        .next()
        .unwrap_or(url_string)
        .to_string();
    // NSURL.path has already decoded the file URL in the native callback.
    let path = url_string.to_string();
    SelectedFile { path, name }
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn open_file(options: &OpenFileOptions) -> Result<Option<SelectedFile>, String> {
    let PreparedPicker { request, picker } = prepare_picker(options, false)?;
    unsafe {
        present_picker_async(picker, request.token.clone());
    }

    // Wait for result (blocks until user picks or cancels)
    match completion::wait(&request.receiver)? {
        Ok(paths) if paths.is_empty() => Ok(None),
        Ok(paths) if paths.len() != 1 => {
            request::discard(&paths);
            Err("Single document picker returned multiple files".into())
        }
        Ok(paths) => Ok(Some(url_to_selected_file(&paths[0]))),
        Err(error) => Err(error),
    }
}

pub fn open_files(options: &OpenFileOptions) -> Result<Vec<SelectedFile>, String> {
    let PreparedPicker { request, picker } = prepare_picker(options, true)?;
    unsafe {
        present_picker_async(picker, request.token.clone());
    }

    match completion::wait(&request.receiver)? {
        Ok(paths) => Ok(paths.iter().map(|p| url_to_selected_file(p)).collect()),
        Err(error) => Err(error),
    }
}

pub fn get_save_path(_options: &SaveFileOptions) -> Result<Option<String>, String> {
    Err("iOS cannot return a writable save path without exporting file contents; this path-only API is unsupported".into())
}

pub fn export_file(
    options: &SaveFileOptions,
    contents: &[u8],
) -> Result<Option<SelectedFile>, String> {
    let rx = set_export_result_channel()?;
    let source = TempExport::new(options.suggested_name.as_deref(), contents)?;
    unsafe {
        let ns_path = crate::ios::util::nsstring(&source.path().to_string_lossy());
        let file_url: *mut AnyObject = msg_send![class!(NSURL), fileURLWithPath: ns_path];
        if file_url.is_null() {
            return Err("Failed to create export file URL".into());
        }
        let urls: *mut AnyObject = msg_send![class!(NSArray), arrayWithObject: file_url];

        let picker: *mut AnyObject = msg_send![class!(UIDocumentPickerViewController), alloc];
        let picker: *mut AnyObject = msg_send![picker,
            initForExportingURLs: urls,
            asCopy: objc2::runtime::Bool::from(true)
        ];
        if picker.is_null() {
            return Err("Failed to create export document picker".into());
        }
        let _: () = msg_send![picker,
            setAllowsMultipleSelection: objc2::runtime::Bool::from(false)];

        let delegate_cls = delegate_class();
        let delegate: *mut AnyObject = msg_send![delegate_cls, alloc];
        let delegate: *mut AnyObject = msg_send![delegate, init];
        if delegate.is_null() {
            let _: () = msg_send![picker, release];
            return Err("Cannot allocate picker delegate".into());
        }
        delegate::mark_export(delegate);
        delegate::bind(picker, delegate, rx.token.clone())?;

        let presented = present_picker(picker);
        let _: () = msg_send![picker, release];
        presented?;
    }

    match completion::wait(&rx.receiver)? {
        Ok(paths) if paths.is_empty() => Ok(None),
        Ok(paths) if paths.len() != 1 => {
            Err("Export document picker returned multiple destinations".into())
        }
        Ok(paths) => Ok(Some(url_to_selected_file(&paths[0]))),
        Err(error) => Err(error),
    }
}

pub fn open_directory() -> Result<Option<SecurityScopedDirectory>, String> {
    let rx = set_result_channel()?;
    let directory = DirectoryMailbox::install()?;
    unsafe {
        let folder = uttype_from_identifier("public.folder");
        if folder.is_null() {
            return Err("Failed to create the folder content type".into());
        }
        let content_types = nsarray_from_objects(&[folder]);

        let picker: *mut AnyObject = msg_send![class!(UIDocumentPickerViewController), alloc];
        // Original URLs are security-scoped; an imported copy would not be a directory grant.
        let picker: *mut AnyObject = msg_send![
            picker,
            initForOpeningContentTypes: content_types,
            asCopy: objc2::runtime::Bool::from(false)
        ];
        if picker.is_null() {
            return Err("Failed to create directory document picker".into());
        }
        let _: () = msg_send![picker,
            setAllowsMultipleSelection: objc2::runtime::Bool::from(false)];

        let delegate_cls = delegate_class();
        let delegate: *mut AnyObject = msg_send![delegate_cls, alloc];
        let delegate: *mut AnyObject = msg_send![delegate, init];
        if delegate.is_null() {
            let _: () = msg_send![picker, release];
            return Err("Cannot allocate picker delegate".into());
        }
        delegate::mark_directory(delegate);
        delegate::bind(picker, delegate, rx.token.clone())?;

        let presented = present_picker(picker);
        let _: () = msg_send![picker, release];
        presented?;
    }

    match completion::wait(&rx.receiver)? {
        Ok(_) => directory.take(),
        Err(error) => Err(error),
    }
}

pub fn get_directory_path(_initial_directory: Option<&str>) -> Result<Option<String>, String> {
    Err("iOS directory access requires a security-scoped lifetime which this path-only API cannot represent".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    #[test]
    fn live_receiver_keeps_picker_wait_open() {
        let (sender, receiver) = mpsc::channel();
        let provider = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            sender.send(Ok(vec![])).unwrap();
        });

        let result = wait_for_picker(&receiver, || false);
        provider.join().unwrap();

        assert_eq!(result, Ok(Ok(vec![])));
    }

    #[test]
    fn cancellation_ends_picker_wait() {
        let (_sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let waiter_cancelled = cancelled.clone();
        let (result_sender, result_receiver) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            result_sender
                .send(wait_for_picker(&receiver, || {
                    waiter_cancelled.load(Ordering::Acquire)
                }))
                .unwrap();
        });

        std::thread::sleep(Duration::from_millis(30));
        cancelled.store(true, Ordering::Release);
        let result = result_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("cancellation must end the picker wait");
        waiter.join().unwrap();

        assert_eq!(
            result,
            Err("Native operation closed during completion".into())
        );
    }
}
