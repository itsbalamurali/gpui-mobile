//! Wait for native callbacks without blocking UIKit's main run loop.
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};
use std::sync::mpsc::{Receiver, TryRecvError};

pub fn wait<T>(receiver: &Receiver<T>) -> Result<T, String> {
    let main: Bool = unsafe { msg_send![class!(NSThread), isMainThread] };
    if !main.as_bool() {
        return receiver
            .recv()
            .map_err(|e| format!("Native callback disconnected: {e}"));
    }
    wait_cancellable(receiver, || true)
}

/// Cancellation is driven by a native terminal event, never an elapsed-time guess.
pub fn wait_cancellable<T>(
    receiver: &Receiver<T>,
    still_open: impl Fn() -> bool,
) -> Result<T, String> {
    let main: Bool = unsafe { msg_send![class!(NSThread), isMainThread] };
    if !main.as_bool() {
        loop {
            if !still_open() {
                return Err("Native operation closed during completion".into());
            }
            match receiver.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(value) => return Ok(value),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Native callback disconnected".into())
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (),
            }
        }
    }
    loop {
        if !still_open() {
            return Err("Native operation closed during completion".into());
        }
        match receiver.try_recv() {
            Ok(value) => return Ok(value),
            Err(TryRecvError::Disconnected) => return Err("Native callback disconnected".into()),
            Err(TryRecvError::Empty) => unsafe {
                objc2::rc::autoreleasepool(|_| {
                    let run_loop: *mut AnyObject = msg_send![class!(NSRunLoop), currentRunLoop];
                    let date: *mut AnyObject =
                        msg_send![class!(NSDate), dateWithTimeIntervalSinceNow: 0.01f64];
                    let _: () = msg_send![run_loop, runUntilDate: date];
                });
            },
        }
    }
}

pub unsafe fn error_message(error: *mut AnyObject) -> String {
    let description: *mut AnyObject = msg_send![error, localizedDescription];
    let utf8: *const std::ffi::c_char = msg_send![description, UTF8String];
    if utf8.is_null() {
        "Native operation failed".into()
    } else {
        std::ffi::CStr::from_ptr(utf8)
            .to_string_lossy()
            .into_owned()
    }
}

/// UIKit picker delegate properties are weak; retain until the picker is freed.
pub unsafe fn set_picker_delegate(
    picker: *mut AnyObject,
    delegate: *mut AnyObject,
) -> Result<(), String> {
    if delegate.is_null() {
        return Err("Failed to allocate picker delegate".into());
    }
    extern "C" {
        fn objc_setAssociatedObject(
            object: *mut AnyObject,
            key: *const u8,
            value: *mut AnyObject,
            policy: usize,
        );
    }
    static KEY: u8 = 0;
    objc_setAssociatedObject(picker, &KEY, delegate, 1);
    let _: () = msg_send![picker, setDelegate: delegate];
    let _: () = msg_send![delegate, release];
    Ok(())
}

pub fn private_import_directory() -> Result<std::path::PathBuf, String> {
    unsafe {
        extern "C" {
            fn NSSearchPathForDirectoriesInDomains(
                directory: usize,
                domain: usize,
                expand: Bool,
            ) -> *mut AnyObject;
        }
        let paths = NSSearchPathForDirectoriesInDomains(14, 1, Bool::YES);
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

pub fn import_file(source: &std::path::Path) -> Result<String, String> {
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
