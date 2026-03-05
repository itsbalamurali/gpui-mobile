use objc::{class, msg_send, sel, sel_impl};
use objc::runtime::Object;

pub fn share_text(text: &str, _subject: Option<&str>) -> Result<(), String> {
    unsafe {
        // NSString *shareText = @"...";
        let ns_text: *mut Object = msg_send![
            class!(NSString),
            stringWithUTF8String: text.as_ptr() as *const std::ffi::c_char
        ];
        if ns_text.is_null() {
            // Fallback: use alloc + initWithBytes for non-null-terminated strings
            let ns_text: *mut Object = msg_send![class!(NSString), alloc];
            let ns_text: *mut Object = msg_send![ns_text,
                initWithBytes: text.as_ptr() as *const std::ffi::c_void
                length: text.len()
                encoding: 4u64  // NSUTF8StringEncoding
            ];
            if ns_text.is_null() {
                return Err("Failed to create NSString".into());
            }
            return present_share_sheet(ns_text);
        }
        present_share_sheet(ns_text)
    }
}

unsafe fn present_share_sheet(text: *mut Object) -> Result<(), String> {
    // NSArray *items = @[text];
    let items: *mut Object = msg_send![class!(NSArray), arrayWithObject: text];
    if items.is_null() {
        return Err("Failed to create NSArray".into());
    }

    // UIActivityViewController *vc = [[UIActivityViewController alloc]
    //     initWithActivityItems:items applicationActivities:nil];
    let vc: *mut Object = msg_send![class!(UIActivityViewController), alloc];
    let nil: *mut Object = std::ptr::null_mut();
    let vc: *mut Object = msg_send![vc,
        initWithActivityItems: items
        applicationActivities: nil
    ];
    if vc.is_null() {
        return Err("Failed to create UIActivityViewController".into());
    }

    // Get the root view controller to present from.
    let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
    let key_window: *mut Object = msg_send![app, keyWindow];
    if key_window.is_null() {
        return Err("No key window available".into());
    }
    let root_vc: *mut Object = msg_send![key_window, rootViewController];
    if root_vc.is_null() {
        return Err("No root view controller".into());
    }

    // [rootVC presentViewController:vc animated:YES completion:nil];
    let _: () = msg_send![root_vc,
        presentViewController: vc
        animated: true
        completion: nil
    ];

    Ok(())
}
