use super::{WebViewHandle, WebViewSettings};
use objc::{class, msg_send, sel, sel_impl};
use objc::runtime::Object;

pub fn load_url(url: &str, settings: &WebViewSettings) -> Result<WebViewHandle, String> {
    unsafe {
        let webview = create_webview(settings)?;

        // NSURL *nsurl = [NSURL URLWithString:@"..."];
        let ns_url_str: *mut Object = msg_send![class!(NSString), alloc];
        let ns_url_str: *mut Object = msg_send![ns_url_str,
            initWithBytes: url.as_ptr() as *const std::ffi::c_void
            length: url.len()
            encoding: 4u64  // NSUTF8StringEncoding
        ];
        let nsurl: *mut Object = msg_send![class!(NSURL), URLWithString: ns_url_str];
        if nsurl.is_null() {
            return Err(format!("Invalid URL: {url}"));
        }

        // NSURLRequest *request = [NSURLRequest requestWithURL:nsurl];
        let request: *mut Object = msg_send![class!(NSURLRequest), requestWithURL: nsurl];

        // [webview loadRequest:request];
        let _: *mut Object = msg_send![webview, loadRequest: request];

        add_to_window(webview)?;
        Ok(WebViewHandle { ptr: webview as usize })
    }
}

pub fn load_html(html: &str, settings: &WebViewSettings) -> Result<WebViewHandle, String> {
    unsafe {
        let webview = create_webview(settings)?;

        let ns_html: *mut Object = msg_send![class!(NSString), alloc];
        let ns_html: *mut Object = msg_send![ns_html,
            initWithBytes: html.as_ptr() as *const std::ffi::c_void
            length: html.len()
            encoding: 4u64
        ];

        let base_url: *mut Object = std::ptr::null_mut();
        let _: *mut Object = msg_send![webview, loadHTMLString: ns_html baseURL: base_url];

        add_to_window(webview)?;
        Ok(WebViewHandle { ptr: webview as usize })
    }
}

pub fn evaluate_javascript(handle: &WebViewHandle, script: &str) -> Result<(), String> {
    unsafe {
        let webview = handle.ptr as *mut Object;
        let ns_script: *mut Object = msg_send![class!(NSString), alloc];
        let ns_script: *mut Object = msg_send![ns_script,
            initWithBytes: script.as_ptr() as *const std::ffi::c_void
            length: script.len()
            encoding: 4u64
        ];
        let nil: *mut Object = std::ptr::null_mut();
        let _: () = msg_send![webview, evaluateJavaScript: ns_script completionHandler: nil];
        Ok(())
    }
}

pub fn dismiss(handle: WebViewHandle) -> Result<(), String> {
    unsafe {
        let webview = handle.ptr as *mut Object;
        let _: () = msg_send![webview, removeFromSuperview];
        // WKWebView doesn't need explicit dealloc — ARC handles it after removeFromSuperview
        // if nothing else retains it.
        Ok(())
    }
}

unsafe fn create_webview(settings: &WebViewSettings) -> Result<*mut Object, String> {
    // WKWebViewConfiguration *config = [[WKWebViewConfiguration alloc] init];
    let config: *mut Object = msg_send![class!(WKWebViewConfiguration), alloc];
    let config: *mut Object = msg_send![config, init];
    if config.is_null() {
        return Err("Failed to create WKWebViewConfiguration".into());
    }

    // config.preferences.javaScriptEnabled = YES/NO;
    let prefs: *mut Object = msg_send![config, preferences];
    if !prefs.is_null() {
        let _: () = msg_send![prefs, setJavaScriptEnabled: settings.javascript_enabled];
    }

    // Get the screen bounds for the frame
    let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
    let bounds: CGRect = msg_send![screen, bounds];

    // WKWebView *webview = [[WKWebView alloc] initWithFrame:bounds configuration:config];
    let webview: *mut Object = msg_send![class!(WKWebView), alloc];
    let webview: *mut Object = msg_send![webview, initWithFrame: bounds configuration: config];
    if webview.is_null() {
        return Err("Failed to create WKWebView".into());
    }

    // Set user agent if specified
    if let Some(ref ua) = settings.user_agent {
        let ns_ua: *mut Object = msg_send![class!(NSString), alloc];
        let ns_ua: *mut Object = msg_send![ns_ua,
            initWithBytes: ua.as_ptr() as *const std::ffi::c_void
            length: ua.len()
            encoding: 4u64
        ];
        let _: () = msg_send![webview, setCustomUserAgent: ns_ua];
    }

    Ok(webview)
}

unsafe fn add_to_window(webview: *mut Object) -> Result<(), String> {
    let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
    let key_window: *mut Object = msg_send![app, keyWindow];
    if key_window.is_null() {
        return Err("No key window available".into());
    }
    let _: () = msg_send![key_window, addSubview: webview];
    Ok(())
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}
