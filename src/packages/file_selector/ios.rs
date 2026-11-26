use super::{OpenFileOptions, SaveFileOptions, SelectedFile, TypeGroup};
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};
use std::sync::Once;

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

fn ensure_background_thread() -> Result<(), String> {
    let main: Bool = unsafe { msg_send![class!(NSThread), isMainThread] };
    if main.as_bool() {
        return Err(
            "Call file_selector off the main thread; synchronous UIKit wait cannot block the main run loop"
                .into(),
        );
    }
    Ok(())
}

fn set_result_channel() -> Result<request::Request<'static>, String> {
    ensure_background_thread()?;
    REQUEST.begin()
}

fn set_export_result_channel() -> Result<request::Request<'static>, String> {
    ensure_background_thread()?;
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
                        imported.push(import_file(&path)?);
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

fn private_import_directory() -> Result<std::path::PathBuf, String> {
    unsafe {
        extern "C" {
            fn NSSearchPathForDirectoriesInDomains(
                directory: u64,
                domain_mask: u64,
                expand_tilde: bool,
            ) -> *mut AnyObject;
        }
        let paths = NSSearchPathForDirectoriesInDomains(14, 1, true);
        if paths.is_null() {
            return Err("Application Support directory unavailable".into());
        }
        let count: usize = msg_send![paths, count];
        if count == 0 {
            return Err("Application Support directory unavailable".into());
        }
        let value: *mut AnyObject = msg_send![paths, objectAtIndex: 0usize];
        let utf8: *const std::ffi::c_char = msg_send![value, UTF8String];
        if utf8.is_null() {
            return Err("Application Support path unavailable".into());
        }
        let path =
            std::path::PathBuf::from(std::ffi::CStr::from_ptr(utf8).to_string_lossy().as_ref())
                .join("gpui-picker-imports")
                .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("Create private import directory: {e}"))?;
        Ok(path)
    }
}

fn import_file(source: &std::path::Path) -> Result<String, String> {
    let name = source.file_name().ok_or("Selected file has no name")?;
    let directory = private_import_directory()?;
    let destination = directory.join(name);
    if let Err(error) = std::fs::copy(source, &destination) {
        let _ = std::fs::remove_file(&destination);
        let _ = std::fs::remove_dir(&directory);
        return Err(format!("Import selected file: {error}"));
    }
    Ok(destination.to_string_lossy().into_owned())
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

fn dispatch_to_main<F>(token: request::Token, f: F)
where
    F: FnOnce(request::Token) -> Result<(), String> + Send + 'static,
{
    struct Pending<F> {
        token: request::Token,
        f: F,
    }
    unsafe extern "C" fn run<F>(context: *mut std::ffi::c_void)
    where
        F: FnOnce(request::Token) -> Result<(), String> + Send + 'static,
    {
        let pending = Box::from_raw(context.cast::<Pending<F>>());
        if !pending.token.is_open() {
            return;
        }
        let token = pending.token.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            objc2::rc::autoreleasepool(|_| (pending.f)(pending.token))
        }))
        .unwrap_or_else(|_| Err("Document picker presentation panicked".into()));
        if let Err(error) = result {
            token.finish(Err(error));
        }
    }

    let pending = Box::into_raw(Box::new(Pending { token, f }));
    unsafe {
        extern "C" {
            fn dispatch_async_f(
                queue: *mut std::ffi::c_void,
                context: *mut std::ffi::c_void,
                work: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
            );
        }
        dispatch_async_f(
            crate::ios::dispatch_get_main_queue(),
            pending.cast(),
            Some(run::<F>),
        );
    }
}

unsafe fn present_and_release(picker: *mut AnyObject) -> Result<(), String> {
    let presented = present_picker(picker);
    let _: () = msg_send![picker, release];
    presented
}

unsafe fn build_and_present_open_picker(
    options: &OpenFileOptions,
    multiple: bool,
    token: request::Token,
) -> Result<(), String> {
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
        setAllowsMultipleSelection: Bool::from(multiple)
    ];

    let delegate_cls = delegate_class();
    let delegate: *mut AnyObject = msg_send![delegate_cls, alloc];
    let delegate: *mut AnyObject = msg_send![delegate, init];
    delegate::bind(picker, delegate, token)?;
    present_and_release(picker)
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
    let request = set_result_channel()?;
    let options = options.clone();
    dispatch_to_main(request.token.clone(), move |token| unsafe {
        build_and_present_open_picker(&options, false, token)
    });

    // Wait for result (blocks until user picks or cancels)
    match request
        .receiver
        .recv()
        .map_err(|_| "Document picker closed without a result".to_string())?
    {
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
    let request = set_result_channel()?;
    let options = options.clone();
    dispatch_to_main(request.token.clone(), move |token| unsafe {
        build_and_present_open_picker(&options, true, token)
    });

    match request
        .receiver
        .recv()
        .map_err(|_| "Document picker closed without a result".to_string())?
    {
        Ok(paths) => Ok(paths.iter().map(|p| url_to_selected_file(p)).collect()),
        Err(error) => Err(error),
    }
}

pub fn get_save_path(options: &SaveFileOptions) -> Result<Option<String>, String> {
    let request = set_export_result_channel()?;
    let suggested_name = options
        .suggested_name
        .as_deref()
        .unwrap_or("untitled")
        .to_string();
    dispatch_to_main(request.token.clone(), move |token| unsafe {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(&suggested_name);
        let _ = std::fs::write(&temp_file, b"");

        let ns_path = nsstring(&temp_file.to_string_lossy());
        let file_url: *mut AnyObject = msg_send![class!(NSURL), fileURLWithPath: ns_path];
        if file_url.is_null() {
            return Err("Failed to create export file URL".into());
        }
        let urls: *mut AnyObject = msg_send![class!(NSArray), arrayWithObject: file_url];

        let picker: *mut AnyObject = msg_send![class!(UIDocumentPickerViewController), alloc];
        let picker: *mut AnyObject =
            msg_send![picker, initForExportingURLs: urls, asCopy: Bool::from(true)];
        if picker.is_null() {
            return Err("Failed to create export document picker".into());
        }
        let _: () = msg_send![picker, setAllowsMultipleSelection: Bool::from(false)];

        let delegate_cls = delegate_class();
        let delegate: *mut AnyObject = msg_send![delegate_cls, alloc];
        let delegate: *mut AnyObject = msg_send![delegate, init];
        if delegate.is_null() {
            let _: () = msg_send![picker, release];
            return Err("Cannot allocate picker delegate".into());
        }
        delegate::mark_export(delegate);
        delegate::bind(picker, delegate, token)?;
        present_and_release(picker)
    });

    match request
        .receiver
        .recv()
        .map_err(|_| "Document picker closed without a result".to_string())?
    {
        Ok(paths) if paths.is_empty() => Ok(None),
        Ok(paths) => Ok(Some(paths[0].clone())),
        Err(error) => Err(error),
    }
}

pub fn get_directory_path(_initial_directory: Option<&str>) -> Result<Option<String>, String> {
    let request = set_export_result_channel()?;
    dispatch_to_main(request.token.clone(), move |token| unsafe {
        let folder = uttype_from_identifier("public.folder");
        if folder.is_null() {
            return Err("Failed to create the folder content type".into());
        }
        let content_types = nsarray_from_objects(&[folder]);

        let picker: *mut AnyObject = msg_send![class!(UIDocumentPickerViewController), alloc];
        let picker: *mut AnyObject =
            msg_send![picker, initForOpeningContentTypes: content_types, asCopy: false];
        if picker.is_null() {
            return Err("Failed to create directory document picker".into());
        }
        let _: () = msg_send![picker, setAllowsMultipleSelection: Bool::from(false)];

        let delegate_cls = delegate_class();
        let delegate: *mut AnyObject = msg_send![delegate_cls, alloc];
        let delegate: *mut AnyObject = msg_send![delegate, init];
        if delegate.is_null() {
            let _: () = msg_send![picker, release];
            return Err("Cannot allocate picker delegate".into());
        }
        delegate::mark_export(delegate);
        delegate::bind(picker, delegate, token)?;
        present_and_release(picker)
    });

    match request
        .receiver
        .recv()
        .map_err(|_| "Document picker closed without a result".to_string())?
    {
        Ok(paths) if paths.is_empty() => Ok(None),
        Ok(paths) => Ok(Some(paths[0].clone())),
        Err(error) => Err(error),
    }
}
