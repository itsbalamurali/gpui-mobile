use super::{OpenFileOptions, SaveFileOptions, SelectedFile, TypeGroup};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel, BOOL, YES};
use objc::{class, msg_send, sel, sel_impl};
use std::sync::{mpsc, Mutex, Once};

#[link(name = "UIKit", kind = "framework")]
extern "C" {}

#[link(name = "UniformTypeIdentifiers", kind = "framework")]
extern "C" {}

// ── Result channel ──────────────────────────────────────────────────────────

static RESULT_TX: Mutex<Option<mpsc::Sender<Vec<String>>>> = Mutex::new(None);

fn set_result_channel() -> mpsc::Receiver<Vec<String>> {
    let (tx, rx) = mpsc::channel();
    *RESULT_TX.lock().unwrap() = Some(tx);
    rx
}

fn send_result(paths: Vec<String>) {
    if let Some(tx) = RESULT_TX.lock().unwrap().take() {
        let _ = tx.send(paths);
    }
}

// ── ObjC delegate class ─────────────────────────────────────────────────────

static REGISTER_DELEGATE: Once = Once::new();
static mut DELEGATE_CLASS: *const Class = std::ptr::null();

fn delegate_class() -> &'static Class {
    REGISTER_DELEGATE.call_once(|| {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("GpuiDocumentPickerDelegate", superclass).unwrap();

        unsafe {
            // documentPicker:didPickDocumentsAtURLs:
            decl.add_method(
                sel!(documentPicker:didPickDocumentsAtURLs:),
                did_pick_documents as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
            );
            // documentPickerWasCancelled:
            decl.add_method(
                sel!(documentPickerWasCancelled:),
                did_cancel as extern "C" fn(&Object, Sel, *mut Object),
            );
        }

        unsafe { DELEGATE_CLASS = decl.register() };
    });
    unsafe { &*DELEGATE_CLASS }
}

extern "C" fn did_pick_documents(
    _this: &Object,
    _sel: Sel,
    _controller: *mut Object,
    urls: *mut Object,
) {
    unsafe {
        let count: usize = msg_send![urls, count];
        let mut paths = Vec::with_capacity(count);
        for i in 0..count {
            let url: *mut Object = msg_send![urls, objectAtIndex: i];
            let abs_string: *mut Object = msg_send![url, absoluteString];
            if !abs_string.is_null() {
                let cstr: *const std::ffi::c_char = msg_send![abs_string, UTF8String];
                if !cstr.is_null() {
                    paths.push(
                        std::ffi::CStr::from_ptr(cstr)
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
        // Dismiss the picker
        let _: () = msg_send![_controller, dismissViewControllerAnimated: YES completion: std::ptr::null::<Object>()];
        send_result(paths);
    }
}

extern "C" fn did_cancel(_this: &Object, _sel: Sel, controller: *mut Object) {
    unsafe {
        let _: () = msg_send![controller, dismissViewControllerAnimated: YES completion: std::ptr::null::<Object>()];
    }
    send_result(vec![]);
}

// ── UTType helpers ──────────────────────────────────────────────────────────

/// Convert a TypeGroup into an NSArray of UTType objects.
unsafe fn type_group_to_uttypes(group: &TypeGroup) -> *mut Object {
    let mut types: Vec<*mut Object> = Vec::new();

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

unsafe fn uttype_from_identifier(ident: &str) -> *mut Object {
    let ns_ident = nsstring(ident);
    msg_send![class!(UTType), typeWithIdentifier: ns_ident]
}

unsafe fn uttype_from_extension(ext: &str) -> *mut Object {
    let ns_ext = nsstring(ext);
    msg_send![class!(UTType), typeWithFilenameExtension: ns_ext]
}

unsafe fn uttype_from_mime(mime: &str) -> *mut Object {
    let ns_mime = nsstring(mime);
    msg_send![class!(UTType), typeWithMIMEType: ns_mime]
}

unsafe fn nsstring(s: &str) -> *mut Object {
    let ns: *mut Object = msg_send![class!(NSString), alloc];
    msg_send![ns,
        initWithBytes: s.as_ptr() as *const std::ffi::c_void
        length: s.len()
        encoding: 4u64 // NSUTF8StringEncoding
    ]
}

unsafe fn nsarray_from_objects(objects: &[*mut Object]) -> *mut Object {
    msg_send![class!(NSArray),
        arrayWithObjects: objects.as_ptr()
        count: objects.len()
    ]
}

// ── Presentation helper ─────────────────────────────────────────────────────

unsafe fn present_picker(picker: *mut Object) -> Result<(), String> {
    let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
    let key_window: *mut Object = msg_send![app, keyWindow];
    if key_window.is_null() {
        return Err("No key window available".into());
    }
    let root_vc: *mut Object = msg_send![key_window, rootViewController];
    if root_vc.is_null() {
        return Err("No root view controller".into());
    }
    let _: () = msg_send![root_vc,
        presentViewController: picker
        animated: YES
        completion: std::ptr::null::<Object>()
    ];
    Ok(())
}

fn url_to_selected_file(url_string: &str) -> SelectedFile {
    // Extract filename from URL path
    let name = url_string
        .rsplit('/')
        .next()
        .unwrap_or(url_string)
        .to_string();
    // Decode percent-encoded path
    let path = url_string
        .strip_prefix("file://")
        .unwrap_or(url_string)
        .to_string();
    SelectedFile { path, name }
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn open_file(options: &OpenFileOptions) -> Result<Option<SelectedFile>, String> {
    let rx = set_result_channel();
    unsafe {
        let content_types = if options.accept_type_groups.is_empty() {
            let ut = uttype_from_identifier("public.item");
            nsarray_from_objects(&[ut])
        } else {
            // Merge all type groups into one array
            let mut all_types: Vec<*mut Object> = Vec::new();
            for group in &options.accept_type_groups {
                
                let arr = type_group_to_uttypes(group);
                let count: usize = msg_send![arr, count];
                for i in 0..count {
                    let obj: *mut Object = msg_send![arr, objectAtIndex: i];
                    all_types.push(obj);
                }
            }
            nsarray_from_objects(&all_types)
        };

        // UIDocumentPickerViewController *picker =
        //   [[UIDocumentPickerViewController alloc] initForOpeningContentTypes:types];
        let picker: *mut Object = msg_send![class!(UIDocumentPickerViewController), alloc];
        let picker: *mut Object = msg_send![picker, initForOpeningContentTypes: content_types];
        if picker.is_null() {
            return Err("Failed to create UIDocumentPickerViewController".into());
        }

        let _: () = msg_send![picker, setAllowsMultipleSelection: false as BOOL];

        // Set delegate
        let delegate_cls = delegate_class();
        let delegate: *mut Object = msg_send![delegate_cls, alloc];
        let delegate: *mut Object = msg_send![delegate, init];
        let _: () = msg_send![picker, setDelegate: delegate];

        present_picker(picker)?;
    }

    // Wait for result (blocks until user picks or cancels)
    match rx.recv() {
        Ok(paths) if paths.is_empty() => Ok(None),
        Ok(paths) => Ok(Some(url_to_selected_file(&paths[0]))),
        Err(_) => Ok(None),
    }
}

pub fn open_files(options: &OpenFileOptions) -> Result<Vec<SelectedFile>, String> {
    let rx = set_result_channel();
    unsafe {
        let content_types = if options.accept_type_groups.is_empty() {
            let ut = uttype_from_identifier("public.item");
            nsarray_from_objects(&[ut])
        } else {
            let mut all_types: Vec<*mut Object> = Vec::new();
            for group in &options.accept_type_groups {
                let arr = type_group_to_uttypes(group);
                let count: usize = msg_send![arr, count];
                for i in 0..count {
                    let obj: *mut Object = msg_send![arr, objectAtIndex: i];
                    all_types.push(obj);
                }
            }
            nsarray_from_objects(&all_types)
        };

        let picker: *mut Object = msg_send![class!(UIDocumentPickerViewController), alloc];
        let picker: *mut Object = msg_send![picker, initForOpeningContentTypes: content_types];
        if picker.is_null() {
            return Err("Failed to create UIDocumentPickerViewController".into());
        }

        let _: () = msg_send![picker, setAllowsMultipleSelection: YES];

        let delegate_cls = delegate_class();
        let delegate: *mut Object = msg_send![delegate_cls, alloc];
        let delegate: *mut Object = msg_send![delegate, init];
        let _: () = msg_send![picker, setDelegate: delegate];

        present_picker(picker)?;
    }

    match rx.recv() {
        Ok(paths) => Ok(paths.iter().map(|p| url_to_selected_file(p)).collect()),
        Err(_) => Ok(vec![]),
    }
}

pub fn get_save_path(options: &SaveFileOptions) -> Result<Option<String>, String> {
    // iOS doesn't have a native "save as" dialog like desktop.
    // We use UIDocumentPickerViewController in export mode.
    // For now, create a temp file and let the user pick where to export it.
    let rx = set_result_channel();
    unsafe {
        let content_types = if options.accept_type_groups.is_empty() {
            let ut = uttype_from_identifier("public.data");
            nsarray_from_objects(&[ut])
        } else {
            let mut all_types: Vec<*mut Object> = Vec::new();
            for group in &options.accept_type_groups {
                let arr = type_group_to_uttypes(group);
                let count: usize = msg_send![arr, count];
                for i in 0..count {
                    let obj: *mut Object = msg_send![arr, objectAtIndex: i];
                    all_types.push(obj);
                }
            }
            nsarray_from_objects(&all_types)
        };

        // Use "move to" mode which lets user pick a save location
        let picker: *mut Object = msg_send![class!(UIDocumentPickerViewController), alloc];
        let picker: *mut Object = msg_send![picker, initForOpeningContentTypes: content_types];
        if picker.is_null() {
            return Err("Failed to create UIDocumentPickerViewController".into());
        }

        let delegate_cls = delegate_class();
        let delegate: *mut Object = msg_send![delegate_cls, alloc];
        let delegate: *mut Object = msg_send![delegate, init];
        let _: () = msg_send![picker, setDelegate: delegate];

        present_picker(picker)?;
    }

    match rx.recv() {
        Ok(paths) if paths.is_empty() => Ok(None),
        Ok(paths) => {
            let path = paths[0]
                .strip_prefix("file://")
                .unwrap_or(&paths[0])
                .to_string();
            Ok(Some(path))
        }
        Err(_) => Ok(None),
    }
}

pub fn get_directory_path(initial_directory: Option<&str>) -> Result<Option<String>, String> {
    let rx = set_result_channel();
    unsafe {
        // Use UTType.folder for directory picking
        let folder_type = uttype_from_identifier("public.folder");
        let content_types = nsarray_from_objects(&[folder_type]);

        let picker: *mut Object = msg_send![class!(UIDocumentPickerViewController), alloc];
        let picker: *mut Object = msg_send![picker, initForOpeningContentTypes: content_types];
        if picker.is_null() {
            return Err("Failed to create UIDocumentPickerViewController".into());
        }

        // Set initial directory if provided
        if let Some(dir) = initial_directory {
            let ns_dir = nsstring(dir);
            let dir_url: *mut Object =
                msg_send![class!(NSURL), fileURLWithPath: ns_dir isDirectory: YES];
            if !dir_url.is_null() {
                let _: () = msg_send![picker, setDirectoryURL: dir_url];
            }
        }

        let delegate_cls = delegate_class();
        let delegate: *mut Object = msg_send![delegate_cls, alloc];
        let delegate: *mut Object = msg_send![delegate, init];
        let _: () = msg_send![picker, setDelegate: delegate];

        present_picker(picker)?;
    }

    match rx.recv() {
        Ok(paths) if paths.is_empty() => Ok(None),
        Ok(paths) => {
            let path = paths[0]
                .strip_prefix("file://")
                .unwrap_or(&paths[0])
                .to_string();
            Ok(Some(path))
        }
        Err(_) => Ok(None),
    }
}
