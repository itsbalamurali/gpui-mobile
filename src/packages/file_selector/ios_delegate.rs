//! Per-controller token; no callback ever targets a global/current sender.
use super::{completion, request};
use objc2::runtime::{AnyObject, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};
use std::ffi::c_void;

unsafe fn token_pointer(delegate: *mut AnyObject) -> *mut *mut c_void {
    let object = &*delegate;
    object
        .class()
        .instance_variable(c"gpui_request")
        .unwrap()
        .load_ptr(object)
}

unsafe fn export_pointer(delegate: *mut AnyObject) -> *mut *mut c_void {
    let object = &*delegate;
    object
        .class()
        .instance_variable(c"gpui_export")
        .unwrap()
        .load_ptr(object)
}

unsafe fn directory_pointer(delegate: *mut AnyObject) -> *mut *mut c_void {
    let object = &*delegate;
    object
        .class()
        .instance_variable(c"gpui_directory")
        .unwrap()
        .load_ptr(object)
}

/// The same delegate class serves import and export pickers. Export callbacks
/// must return the destination instead of copying the destination into app storage.
pub unsafe fn mark_export(delegate: *mut AnyObject) {
    *export_pointer(delegate) = 1usize as *mut c_void;
}

pub unsafe fn is_export(delegate: *mut AnyObject) -> bool {
    !(*export_pointer(delegate)).is_null()
}

/// Directory callbacks retain the original URL and start its security scope.
pub unsafe fn mark_directory(delegate: *mut AnyObject) {
    *directory_pointer(delegate) = 1usize as *mut c_void;
}

pub unsafe fn is_directory(delegate: *mut AnyObject) -> bool {
    !(*directory_pointer(delegate)).is_null()
}

pub unsafe fn token(delegate: *mut AnyObject) -> Option<request::Token> {
    let ptr = *token_pointer(delegate) as *mut request::Token;
    ptr.as_ref().cloned()
}
pub unsafe fn bind(
    picker: *mut AnyObject,
    delegate: *mut AnyObject,
    token: request::Token,
) -> Result<(), String> {
    if delegate.is_null() {
        let _: () = msg_send![picker, release];
        return Err("Cannot allocate picker delegate".into());
    }
    *token_pointer(delegate) = Box::into_raw(Box::new(token)).cast();
    // UIDocumentPickerDelegate owns result delivery; presentation dismissal can precede it.
    completion::set_picker_delegate(picker, delegate)?;
    Ok(())
}
pub unsafe fn register(builder: &mut ClassBuilder) {
    builder.add_ivar::<*mut c_void>(c"gpui_request");
    builder.add_ivar::<*mut c_void>(c"gpui_export");
    builder.add_ivar::<*mut c_void>(c"gpui_directory");
    builder.add_method(
        sel!(dealloc),
        dealloc as unsafe extern "C" fn(*mut AnyObject, Sel),
    );
}
unsafe extern "C" fn dealloc(this: *mut AnyObject, _: Sel) {
    let ptr = *token_pointer(this) as *mut request::Token;
    if !ptr.is_null() {
        *token_pointer(this) = std::ptr::null_mut();
        let token = Box::from_raw(ptr);
        if !token.is_dismissing() {
            token.finish(Err("Picker controller closed without a result".into()));
        }
    }
    let _: () = msg_send![super(this, class!(NSObject)), dealloc];
}

pub unsafe fn dismiss(
    picker: *mut AnyObject,
    token: request::Token,
    result: request::PickerResult,
    animated: bool,
    after_dismissal: Option<Box<dyn FnOnce()>>,
) {
    if !token.begin_dismissal() {
        token.finish(result);
        return;
    }
    // Retain the busy guard until dismissal completes. This block can be copied by UIKit.
    struct Delivery {
        token: request::Token,
        result: Option<request::PickerResult>,
        after_dismissal: Option<Box<dyn FnOnce()>>,
    }
    impl Drop for Delivery {
        fn drop(&mut self) {
            if let Some(result) = self.result.take() {
                self.token.finish_incomplete_dismissal(result);
            }
            if let Some(after_dismissal) = self.after_dismissal.take() {
                after_dismissal();
            }
        }
    }
    let delivery = std::sync::Mutex::new(Delivery {
        token,
        result: Some(result),
        after_dismissal,
    });
    let done = block2::RcBlock::new(move || {
        if let Ok(mut delivery) = delivery.lock() {
            if let Some(result) = delivery.result.take() {
                delivery.token.finish(result);
            }
            if let Some(after_dismissal) = delivery.after_dismissal.take() {
                after_dismissal();
            }
        }
    });
    let _: () = msg_send![picker,
        dismissViewControllerAnimated: animated,
        completion: &*done
    ];
}
