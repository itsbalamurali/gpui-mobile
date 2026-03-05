//! Packages demo screen — showcases all 12 gpui-mobile utility packages.

use gpui::{div, prelude::*, px, rgb};

use super::{Router, BLUE, GREEN, LIGHT_CARD_BG, LIGHT_DIVIDER, LIGHT_SUBTEXT, LIGHT_TEXT, MAUVE, PEACH, RED, SURFACE0, SURFACE1, TEAL, TEXT, YELLOW};

/// Render the Packages demo screen.
pub fn render(router: &Router, cx: &mut gpui::Context<Router>) -> impl IntoElement {
    let dark_mode = router.dark_mode;
    let text_color = if dark_mode { TEXT } else { LIGHT_TEXT };
    let sub_text: u32 = if dark_mode { super::SUBTEXT } else { LIGHT_SUBTEXT };
    let card_bg = if dark_mode { SURFACE0 } else { LIGHT_CARD_BG };
    let divider_color = if dark_mode { SURFACE1 } else { LIGHT_DIVIDER };

    let mut root = div().flex().flex_col().flex_1().gap_4().px_4().py_6();

    // ── Device Info (native API, no JNI) ────────────────────────────────────
    root = root
        .child(section_header("Device Info", sub_text))
        .child({
            let info = gpui_mobile::packages::device_info::get_device_info();
            match info {
                Ok(di) => info_card(card_bg)
                    .child(kv_row("Model", &di.model, GREEN, text_color, sub_text))
                    .child(divider_line(divider_color))
                    .child(kv_row("Manufacturer", &di.manufacturer, GREEN, text_color, sub_text))
                    .child(divider_line(divider_color))
                    .child(kv_row("OS Version", &di.os_version, GREEN, text_color, sub_text))
                    .child(divider_line(divider_color))
                    .child(kv_row("Device Name", &di.device_name, GREEN, text_color, sub_text))
                    .child(divider_line(divider_color))
                    .child(kv_row(
                        "Physical Device",
                        if di.is_physical_device { "Yes" } else { "No" },
                        GREEN,
                        text_color,
                        sub_text,
                    ))
                    .into_any_element(),
                Err(e) => error_card(&e, card_bg, text_color).into_any_element(),
            }
        });

    // ── Path Provider (native API, no JNI) ──────────────────────────────────
    root = root
        .child(section_header("Path Provider", sub_text))
        .child({
            let tmp = gpui_mobile::packages::path_provider::temporary_directory();
            let docs = gpui_mobile::packages::path_provider::documents_directory();
            let cache = gpui_mobile::packages::path_provider::cache_directory();
            let support = gpui_mobile::packages::path_provider::support_directory();

            info_card(card_bg)
                .child(kv_row("Temp", &path_or_err(&tmp), MAUVE, text_color, sub_text))
                .child(divider_line(divider_color))
                .child(kv_row("Documents", &path_or_err(&docs), MAUVE, text_color, sub_text))
                .child(divider_line(divider_color))
                .child(kv_row("Cache", &path_or_err(&cache), MAUVE, text_color, sub_text))
                .child(divider_line(divider_color))
                .child(kv_row("Support", &path_or_err(&support), MAUVE, text_color, sub_text))
        });

    // ── Package Info (JNI) ──────────────────────────────────────────────────
    root = root
        .child(section_header("Package Info", sub_text))
        .child({
            let info = gpui_mobile::packages::package_info::get_package_info();
            match info {
                Ok(pi) => info_card(card_bg)
                    .child(kv_row("App Name", &pi.app_name, BLUE, text_color, sub_text))
                    .child(divider_line(divider_color))
                    .child(kv_row("Package", &pi.package_name, BLUE, text_color, sub_text))
                    .child(divider_line(divider_color))
                    .child(kv_row("Version", &pi.version, BLUE, text_color, sub_text))
                    .child(divider_line(divider_color))
                    .child(kv_row("Build", &pi.build_number, BLUE, text_color, sub_text))
                    .into_any_element(),
                Err(e) => error_card(&e, card_bg, text_color).into_any_element(),
            }
        });

    // ── Connectivity (JNI) ──────────────────────────────────────────────────
    root = root
        .child(section_header("Connectivity", sub_text))
        .child({
            let status = gpui_mobile::packages::connectivity::check_connectivity();
            let label = format!("{:?}", status);
            info_card(card_bg).child(kv_row("Status", &label, TEAL, text_color, sub_text))
        });

    // ── Network Info (JNI) ──────────────────────────────────────────────────
    root = root
        .child(section_header("Network Info", sub_text))
        .child({
            let info = gpui_mobile::packages::network_info::get_network_info();
            match info {
                Ok(ni) => info_card(card_bg)
                    .child(kv_row(
                        "WiFi Name",
                        ni.wifi_name.as_deref().unwrap_or("N/A"),
                        YELLOW,
                        text_color,
                        sub_text,
                    ))
                    .child(divider_line(divider_color))
                    .child(kv_row(
                        "WiFi BSSID",
                        ni.wifi_bssid.as_deref().unwrap_or("N/A"),
                        YELLOW,
                        text_color,
                        sub_text,
                    ))
                    .child(divider_line(divider_color))
                    .child(kv_row(
                        "WiFi IP",
                        ni.wifi_ip.as_deref().unwrap_or("N/A"),
                        YELLOW,
                        text_color,
                        sub_text,
                    ))
                    .into_any_element(),
                Err(e) => error_card(&e, card_bg, text_color).into_any_element(),
            }
        });

    // ── Shared Preferences (JNI) ────────────────────────────────────────────
    root = root
        .child(section_header("Shared Preferences", sub_text))
        .child({
            let prefs = gpui_mobile::packages::shared_preferences::SharedPreferences::instance();
            let key = "gpui_demo_counter";
            let current = prefs.get_int(key).unwrap_or(0);
            let _ = prefs.set_int(key, current + 1);
            info_card(card_bg)
                .child(kv_row("Demo Key", key, PEACH, text_color, sub_text))
                .child(divider_line(divider_color))
                .child(kv_row(
                    "Value (increments each visit)",
                    &current.to_string(),
                    PEACH,
                    text_color,
                    sub_text,
                ))
        });

    // ── Vibration (JNI) ─────────────────────────────────────────────────────
    root = root
        .child(section_header("Vibration", sub_text))
        .child({
            let can = gpui_mobile::packages::vibration::can_vibrate();
            let mut card = info_card(card_bg).child(kv_row(
                "Can Vibrate",
                if can { "Yes" } else { "No" },
                PEACH,
                text_color,
                sub_text,
            ));

            if can {
                card = card.child(divider_line(divider_color)).child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .p_3()
                        .child(haptic_button("Light", BLUE, cx.listener(|_this, _, _, cx| {
                            let _ = gpui_mobile::packages::vibration::haptic_feedback(
                                gpui_mobile::packages::vibration::HapticFeedback::Light,
                            );
                            cx.notify();
                        })))
                        .child(haptic_button("Medium", GREEN, cx.listener(|_this, _, _, cx| {
                            let _ = gpui_mobile::packages::vibration::haptic_feedback(
                                gpui_mobile::packages::vibration::HapticFeedback::Medium,
                            );
                            cx.notify();
                        })))
                        .child(haptic_button("Heavy", MAUVE, cx.listener(|_this, _, _, cx| {
                            let _ = gpui_mobile::packages::vibration::haptic_feedback(
                                gpui_mobile::packages::vibration::HapticFeedback::Heavy,
                            );
                            cx.notify();
                        })))
                        .child(haptic_button("Success", TEAL, cx.listener(|_this, _, _, cx| {
                            let _ = gpui_mobile::packages::vibration::haptic_feedback(
                                gpui_mobile::packages::vibration::HapticFeedback::Success,
                            );
                            cx.notify();
                        }))),
                );
            }
            card
        });

    // ── URL Launcher (JNI) ──────────────────────────────────────────────────
    root = root
        .child(section_header("URL Launcher", sub_text))
        .child({
            let can = gpui_mobile::packages::url_launcher::can_launch_url("https://zed.dev");
            info_card(card_bg)
                .child(kv_row(
                    "Can open https://zed.dev",
                    &format!("{:?}", can),
                    BLUE,
                    text_color,
                    sub_text,
                ))
                .child(divider_line(divider_color))
                .child(
                    div()
                        .p_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .px_4()
                                .py_2()
                                .rounded_lg()
                                .bg(rgb(BLUE))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(0x1e1e2e))
                                        .child("Open zed.dev"),
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        let _ = gpui_mobile::packages::url_launcher::launch_url(
                                            "https://zed.dev",
                                        );
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
        });

    // ── Battery ───────────────────────────────────────────────────────────────
    root = root
        .child(section_header("Battery", sub_text))
        .child({
            let bi = gpui_mobile::packages::battery::battery_info();
            info_card(card_bg)
                .child(kv_row("Level", &format!("{}%", bi.level), GREEN, text_color, sub_text))
                .child(divider_line(divider_color))
                .child(kv_row("State", &format!("{:?}", bi.state), GREEN, text_color, sub_text))
                .child(divider_line(divider_color))
                .child(kv_row(
                    "Battery Saver",
                    if bi.is_battery_save_mode { "On" } else { "Off" },
                    GREEN,
                    text_color,
                    sub_text,
                ))
        });

    // ── Sensors ───────────────────────────────────────────────────────────────
    root = root
        .child(section_header("Sensors", sub_text))
        .child({
            let avail = gpui_mobile::packages::sensors::available_sensors();
            let mut card = info_card(card_bg)
                .child(kv_row(
                    "Accelerometer",
                    if avail.accelerometer { "Available" } else { "N/A" },
                    YELLOW,
                    text_color,
                    sub_text,
                ))
                .child(divider_line(divider_color))
                .child(kv_row(
                    "Gyroscope",
                    if avail.gyroscope { "Available" } else { "N/A" },
                    YELLOW,
                    text_color,
                    sub_text,
                ))
                .child(divider_line(divider_color))
                .child(kv_row(
                    "Magnetometer",
                    if avail.magnetometer { "Available" } else { "N/A" },
                    YELLOW,
                    text_color,
                    sub_text,
                ))
                .child(divider_line(divider_color))
                .child(kv_row(
                    "Barometer",
                    if avail.barometer { "Available" } else { "N/A" },
                    YELLOW,
                    text_color,
                    sub_text,
                ));

            // Show live accelerometer reading if available
            if let Some(accel) = gpui_mobile::packages::sensors::accelerometer() {
                card = card
                    .child(divider_line(divider_color))
                    .child(kv_row(
                        "Accel (m/s²)",
                        &format!("x={:.1} y={:.1} z={:.1}", accel.x, accel.y, accel.z),
                        YELLOW,
                        text_color,
                        sub_text,
                    ));
            }
            card
        });

    // ── Share ─────────────────────────────────────────────────────────────────
    root = root
        .child(section_header("Share", sub_text))
        .child({
            info_card(card_bg)
                .child(
                    div()
                        .p_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .px_4()
                                .py_2()
                                .rounded_lg()
                                .bg(rgb(TEAL))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(0x1e1e2e))
                                        .child("Share \"Hello from GPUI!\""),
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        let _ = gpui_mobile::packages::share::share_text(
                                            "Hello from GPUI!",
                                            Some("GPUI Demo"),
                                        );
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
        });

    // ── WebView ───────────────────────────────────────────────────────────────
    root = root
        .child(section_header("WebView", sub_text))
        .child({
            info_card(card_bg)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .p_3()
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .px_4()
                                .py_2()
                                .rounded_lg()
                                .bg(rgb(RED))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0xFFFFFF))
                                        .child("Load HTML"),
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        let settings = gpui_mobile::packages::webview::WebViewSettings::default();
                                        let html = "<html><body style='background:#121318;color:white;display:flex;align-items:center;justify-content:center;height:100vh;font-family:system-ui'><h1>Hello from GPUI WebView!</h1></body></html>";
                                        match gpui_mobile::packages::webview::load_html(html, &settings) {
                                            Ok(handle) => {
                                                log::info!("WebView loaded successfully");
                                            }
                                            Err(e) => {
                                                log::error!("WebView error: {e}");
                                            }
                                        }
                                        cx.notify();
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .px_4()
                                .py_2()
                                .rounded_lg()
                                .bg(rgb(MAUVE))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0xFFFFFF))
                                        .child("Open Google"),
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|_this, _, _, cx| {
                                        let settings = gpui_mobile::packages::webview::WebViewSettings::default();
                                        match gpui_mobile::packages::webview::load_url("https://google.com", &settings) {
                                            Ok(handle) => {
                                                log::info!("WebView loaded URL successfully");
                                            }
                                            Err(e) => {
                                                log::error!("WebView URL error: {e}");
                                            }
                                        }
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
        });

    root
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn path_or_err(r: &Result<std::path::PathBuf, String>) -> String {
    match r {
        Ok(p) => p.display().to_string(),
        Err(e) => format!("Error: {e}"),
    }
}

fn section_header(title: &str, color: u32) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(rgb(color))
        .px_1()
        .child(title.to_string().to_uppercase())
}

fn info_card(bg: u32) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .rounded_xl()
        .bg(rgb(bg))
        .overflow_hidden()
}

fn divider_line(color: u32) -> impl IntoElement {
    div().w_full().h(px(1.0)).bg(rgb(color)).mx_3()
}

fn error_card(msg: &str, bg: u32, text_color: u32) -> impl IntoElement {
    info_card(bg).child(
        div()
            .p_4()
            .text_sm()
            .text_color(rgb(text_color))
            .child(format!("Error: {msg}")),
    )
}

fn kv_row(label: &str, value: &str, accent: u32, text_color: u32, sub_text: u32) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_4()
        .py_3()
        .child(div().size_2().rounded_full().bg(rgb(accent)))
        .child(
            div()
                .text_xs()
                .text_color(rgb(sub_text))
                .min_w(px(80.0))
                .child(label.to_string()),
        )
        .child(
            div()
                .flex_1()
                .text_sm()
                .text_color(rgb(text_color))
                .child(value.to_string()),
        )
}

fn haptic_button(
    label: &str,
    color: u32,
    handler: impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .px_2()
        .py_2()
        .rounded_lg()
        .bg(rgb(color))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x1e1e2e))
                .child(label.to_string()),
        )
        .on_mouse_down(gpui::MouseButton::Left, handler)
}
