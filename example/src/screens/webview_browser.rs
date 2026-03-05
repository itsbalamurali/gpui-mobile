//! In-app browser screen with back, reload, stop, and close controls.
//!
//! The WebView is a native platform overlay. GPUI renders a toolbar with
//! back/reload/close buttons above the WebView area. The native WebView
//! is offset below the GPUI toolbar using `WebViewSettings::top_offset`.

use gpui::{div, prelude::*, px, rgb};

use super::{Router, BLUE, GREEN, LIGHT_CARD_BG, LIGHT_TEXT, RED, SURFACE0, SURFACE1, TEXT, YELLOW};

/// Approximate height of the TopAppBar rendered by the Router (in logical pt).
const APP_BAR_HEIGHT: f32 = 56.0;

pub fn render(router: &Router, cx: &mut gpui::Context<Router>) -> impl IntoElement {
    let dark = router.dark_mode;
    let text_color = if dark { TEXT } else { LIGHT_TEXT };
    let card_bg = if dark { SURFACE0 } else { LIGHT_CARD_BG };
    let toolbar_bg = if dark { SURFACE1 } else { 0xDADAE0 };
    let has_webview = router.webview_handle.is_some();
    let url_display = router.webview_url.clone();

    if has_webview {
        // ── Active WebView: show compact toolbar with back/reload/close ──
        div()
            .flex()
            .flex_col()
            .flex_1()
            // Browser toolbar
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_2()
                    .bg(rgb(toolbar_bg))
                    // Back
                    .child(toolbar_btn(
                        "←",
                        text_color,
                        cx.listener(|this, _, _, cx| {
                            if let Some(ptr) = this.webview_handle {
                                let h = gpui_mobile::packages::webview::WebViewHandle { ptr };
                                let _ = gpui_mobile::packages::webview::go_back(&h);
                                std::mem::forget(h);
                            }
                            cx.notify();
                        }),
                    ))
                    // Reload
                    .child(toolbar_btn(
                        "↻",
                        text_color,
                        cx.listener(|this, _, _, cx| {
                            if let Some(ptr) = this.webview_handle {
                                let h = gpui_mobile::packages::webview::WebViewHandle { ptr };
                                let _ = gpui_mobile::packages::webview::reload(&h);
                                std::mem::forget(h);
                            }
                            cx.notify();
                        }),
                    ))
                    // URL display
                    .child(
                        div()
                            .flex_1()
                            .px_3()
                            .py_1()
                            .mx_1()
                            .rounded_lg()
                            .bg(rgb(card_bg))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(text_color))
                                    .child(url_display),
                            ),
                    )
                    // Close (X)
                    .child(toolbar_btn(
                        "X",
                        RED,
                        cx.listener(|this, _, _, cx| {
                            if let Some(ptr) = this.webview_handle.take() {
                                let h = gpui_mobile::packages::webview::WebViewHandle { ptr };
                                let _ = gpui_mobile::packages::webview::dismiss(h);
                            }
                            cx.notify();
                        }),
                    )),
            )
            // The rest of the screen is behind the native WebView overlay
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x666666))
                            .child("WebView overlay active"),
                    ),
            )
            .into_any_element()
    } else {
        // ── No WebView: show link picker ────────────────────────────────
        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap_3()
            .px_3()
            .py_4()
            // Quick links
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(open_url_btn("Google", "https://google.com", BLUE, router, cx))
                    .child(open_url_btn("GitHub", "https://github.com", 0x333333, router, cx))
                    .child(open_url_btn("Zed.dev", "https://zed.dev", GREEN, router, cx)),
            )
            // Load HTML demo
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_4()
                    .py_3()
                    .rounded_xl()
                    .bg(rgb(0xFA7B17))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xFFFFFF))
                            .child("Load Custom HTML"),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            let top_offset = this.safe_area.top + APP_BAR_HEIGHT;
                            let mut settings = gpui_mobile::packages::webview::WebViewSettings::default();
                            settings.top_offset = top_offset;
                            let html = r#"<html><body style="background:#1e1f25;color:#e2e2e9;display:flex;align-items:center;justify-content:center;height:100vh;font-family:system-ui;flex-direction:column"><h1>GPUI WebView</h1><p>Custom HTML loaded successfully</p><button onclick="document.body.style.background='#4285F4'" style="padding:12px 24px;font-size:16px;border:none;border-radius:8px;background:#34A853;color:white;margin-top:16px">Change Color</button></body></html>"#;
                            match gpui_mobile::packages::webview::load_html(html, &settings) {
                                Ok(handle) => {
                                    this.webview_url = "about:blank".into();
                                    this.webview_handle = Some(handle.ptr);
                                    std::mem::forget(handle);
                                }
                                Err(e) => log::error!("WebView HTML error: {e}"),
                            }
                            cx.notify();
                        }),
                    ),
            )
            // Status
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .py_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x666666))
                            .child("Tap a link above to open the in-app browser"),
                    ),
            )
            .into_any_element()
    }
}

fn toolbar_btn(
    label: &str,
    color: u32,
    handler: impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(36.0))
        .rounded_lg()
        .child(
            div()
                .text_base()
                .text_color(rgb(color))
                .child(label.to_string()),
        )
        .on_mouse_down(gpui::MouseButton::Left, handler)
}

fn open_url_btn(
    label: &str,
    url: &'static str,
    color: u32,
    router: &Router,
    cx: &mut gpui::Context<Router>,
) -> impl IntoElement {
    let top_offset = router.safe_area.top + APP_BAR_HEIGHT;
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .px_2()
        .py_3()
        .rounded_xl()
        .bg(rgb(color))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xFFFFFF))
                .child(label.to_string()),
        )
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                // Dismiss existing
                if let Some(ptr) = this.webview_handle.take() {
                    let h = gpui_mobile::packages::webview::WebViewHandle { ptr };
                    let _ = gpui_mobile::packages::webview::dismiss(h);
                }
                let mut settings = gpui_mobile::packages::webview::WebViewSettings::default();
                settings.top_offset = top_offset;
                match gpui_mobile::packages::webview::load_url(url, &settings) {
                    Ok(handle) => {
                        this.webview_url = url.into();
                        this.webview_handle = Some(handle.ptr);
                        std::mem::forget(handle);
                        log::info!("WebView: loaded {url}");
                    }
                    Err(e) => log::error!("WebView URL error: {e}"),
                }
                cx.notify();
            }),
        )
}
