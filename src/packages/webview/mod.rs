//! In-app WebView for loading web content.
//!
//! Provides a cross-platform WebView API backed by:
//! - Android: `android.webkit.WebView` via JNI
//! - iOS: `WKWebView` via Objective-C
//!
//! Inspired by [flutter_inappwebview](https://pub.dev/packages/flutter_inappwebview)
//! and [gpui-component webview](https://github.com/longbridge/gpui-component).
//!
//! Feature-gated behind `webview`.

#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "android")]
mod android;

/// Configuration for creating a WebView.
#[derive(Debug, Clone)]
pub struct WebViewSettings {
    /// Enable JavaScript execution (default: true).
    pub javascript_enabled: bool,
    /// User-agent string override (None = platform default).
    pub user_agent: Option<String>,
    /// Allow zoom gestures (default: true).
    pub zoom_enabled: bool,
    /// Enable DOM storage / localStorage (default: true).
    pub dom_storage_enabled: bool,
}

impl Default for WebViewSettings {
    fn default() -> Self {
        Self {
            javascript_enabled: true,
            user_agent: None,
            zoom_enabled: true,
            dom_storage_enabled: true,
        }
    }
}

/// Load a URL in a platform-native WebView overlay.
///
/// On Android this creates an `android.webkit.WebView` and adds it to the
/// activity's content view. On iOS this creates a `WKWebView` and adds it
/// to the key window's root view.
///
/// Returns an opaque handle that can be used to control or dismiss the view.
pub fn load_url(url: &str, settings: &WebViewSettings) -> Result<WebViewHandle, String> {
    #[cfg(target_os = "ios")]
    { ios::load_url(url, settings) }
    #[cfg(target_os = "android")]
    { android::load_url(url, settings) }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    { let _ = (url, settings); Err("webview is only available on iOS and Android".into()) }
}

/// Load raw HTML content in a WebView.
pub fn load_html(html: &str, settings: &WebViewSettings) -> Result<WebViewHandle, String> {
    #[cfg(target_os = "ios")]
    { ios::load_html(html, settings) }
    #[cfg(target_os = "android")]
    { android::load_html(html, settings) }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    { let _ = (html, settings); Err("webview is only available on iOS and Android".into()) }
}

/// Evaluate JavaScript in an existing WebView.
pub fn evaluate_javascript(handle: &WebViewHandle, script: &str) -> Result<(), String> {
    #[cfg(target_os = "ios")]
    { ios::evaluate_javascript(handle, script) }
    #[cfg(target_os = "android")]
    { android::evaluate_javascript(handle, script) }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    { let _ = (handle, script); Err("webview is only available on iOS and Android".into()) }
}

/// Dismiss / destroy a WebView.
pub fn dismiss(handle: WebViewHandle) -> Result<(), String> {
    #[cfg(target_os = "ios")]
    { ios::dismiss(handle) }
    #[cfg(target_os = "android")]
    { android::dismiss(handle) }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    { let _ = handle; Err("webview is only available on iOS and Android".into()) }
}

/// Opaque handle to a native WebView instance.
///
/// Holds a platform-specific identifier for the view.
#[derive(Debug)]
pub struct WebViewHandle {
    /// Platform-specific pointer or ID.
    pub(crate) ptr: usize,
}
