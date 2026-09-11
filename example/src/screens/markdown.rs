//! GPUI Kit's Markdown TextView rendered by the native mobile platform.

use gpui_kit::component::{text::TextView, ActiveTheme};
use gpui_kit::{div, prelude::*, App};

const MARKDOWN: &str = include_str!("markdown.md");

pub fn render(cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .min_w_0()
        .p_3()
        .bg(cx.theme().background)
        .text_color(cx.theme().foreground)
        .child(
            TextView::markdown("kit-markdown-example", MARKDOWN)
                .selectable(true)
                .scrollable(false),
        )
}

// Serve the example image from the binary so screenshots also work offline.
// Markdown still exercises the normal URI image loader and image layout.
pub fn init(cx: &mut App) {
    cx.set_http_client(std::sync::Arc::new(DemoImageClient(cx.http_client())));
}

struct DemoImageClient(std::sync::Arc<dyn gpui::http_client::HttpClient>);

impl gpui::http_client::HttpClient for DemoImageClient {
    fn user_agent(&self) -> Option<&gpui::http_client::http::HeaderValue> {
        self.0.user_agent()
    }

    fn proxy(&self) -> Option<&gpui::http_client::Url> {
        self.0.proxy()
    }

    fn send(
        &self,
        request: gpui::http_client::Request<gpui::http_client::AsyncBody>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = gpui::Result<
                        gpui::http_client::Response<gpui::http_client::AsyncBody>,
                    >,
                > + Send,
        >,
    > {
        if request.uri().to_string() == "https://gpui-kit.com/og.png" {
            Box::pin(async {
                Ok(gpui::http_client::Response::builder()
                    .header("Content-Type", "image/png")
                    .body(
                        include_bytes!("../../assets/gpui-kit-og.png")
                            .to_vec()
                            .into(),
                    )?)
            })
        } else if request.uri().to_string() == "https://gpui.example/alpine-lake.jpg" {
            Box::pin(async {
                Ok(gpui::http_client::Response::builder()
                    .header("Content-Type", "image/jpeg")
                    .body(
                        include_bytes!("../../assets/alpine-lake.jpg")
                            .to_vec()
                            .into(),
                    )?)
            })
        } else {
            self.0.send(request)
        }
    }
}
