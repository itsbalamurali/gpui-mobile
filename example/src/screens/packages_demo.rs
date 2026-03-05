//! Packages demo screen — showcases all 12 gpui-mobile utility packages.

use gpui::{div, prelude::*, px, rgb};

use super::{Router, BLUE, GREEN, LIGHT_CARD_BG, LIGHT_DIVIDER, LIGHT_SUBTEXT, LIGHT_TEXT, MAUVE, PEACH, SURFACE0, SURFACE1, TEAL, TEXT, YELLOW};

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
            info_card(card_bg).child(
                div()
                    .p_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .px_4()
                            .py_3()
                            .rounded_lg()
                            .bg(rgb(MAUVE))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xFFFFFF))
                                    .child("Open In-App Browser"),
                            )
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.navigate_to(super::Screen::WebViewBrowser);
                                    cx.notify();
                                }),
                            ),
                    ),
            )
        });

    // ── File Selector ────────────────────────────────────────────────────────
    root = root
        .child(section_header("File Selector", sub_text))
        .child({
            let last_path = router.last_picked_file.as_deref().unwrap_or("None");
            info_card(card_bg)
                .child(kv_row("Last Picked", last_path, BLUE, text_color, sub_text))
                .child(divider_line(divider_color))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .p_3()
                        .child(
                            action_button("Pick File", BLUE, cx.listener(|this, _, _, cx| {
                                let opts = gpui_mobile::packages::file_selector::OpenFileOptions::default();
                                match gpui_mobile::packages::file_selector::open_file(&opts) {
                                    Ok(Some(f)) => this.last_picked_file = Some(f.name),
                                    Ok(None) => this.last_picked_file = Some("Cancelled".into()),
                                    Err(e) => this.last_picked_file = Some(format!("Error: {e}")),
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            action_button("Pick Files", GREEN, cx.listener(|this, _, _, cx| {
                                let opts = gpui_mobile::packages::file_selector::OpenFileOptions::default();
                                match gpui_mobile::packages::file_selector::open_files(&opts) {
                                    Ok(files) => {
                                        this.last_picked_file = Some(format!("{} files", files.len()));
                                    }
                                    Err(e) => this.last_picked_file = Some(format!("Error: {e}")),
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            action_button("Pick Dir", TEAL, cx.listener(|this, _, _, cx| {
                                match gpui_mobile::packages::file_selector::get_directory_path(None) {
                                    Ok(Some(d)) => this.last_picked_file = Some(d),
                                    Ok(None) => this.last_picked_file = Some("Cancelled".into()),
                                    Err(e) => this.last_picked_file = Some(format!("Error: {e}")),
                                }
                                cx.notify();
                            })),
                        ),
                )
        });

    // ── Image Picker ─────────────────────────────────────────────────────────
    root = root
        .child(section_header("Image Picker", sub_text))
        .child({
            let last_image = router.last_picked_image.as_deref().unwrap_or("None");
            info_card(card_bg)
                .child(kv_row("Last Picked", last_image, MAUVE, text_color, sub_text))
                .child(divider_line(divider_color))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .p_3()
                        .child(
                            action_button("Gallery", MAUVE, cx.listener(|this, _, _, cx| {
                                let opts = gpui_mobile::packages::image_picker::ImagePickerOptions {
                                    source: gpui_mobile::packages::image_picker::ImageSource::Gallery,
                                    ..Default::default()
                                };
                                match gpui_mobile::packages::image_picker::pick_image(&opts) {
                                    Ok(Some(f)) => this.last_picked_image = Some(f.name),
                                    Ok(None) => this.last_picked_image = Some("Cancelled".into()),
                                    Err(e) => this.last_picked_image = Some(format!("Error: {e}")),
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            action_button("Camera", PEACH, cx.listener(|this, _, _, cx| {
                                let opts = gpui_mobile::packages::image_picker::ImagePickerOptions {
                                    source: gpui_mobile::packages::image_picker::ImageSource::Camera,
                                    ..Default::default()
                                };
                                match gpui_mobile::packages::image_picker::pick_image(&opts) {
                                    Ok(Some(f)) => this.last_picked_image = Some(f.name),
                                    Ok(None) => this.last_picked_image = Some("Cancelled".into()),
                                    Err(e) => this.last_picked_image = Some(format!("Error: {e}")),
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            action_button("Multi", YELLOW, cx.listener(|this, _, _, cx| {
                                match gpui_mobile::packages::image_picker::pick_multi_image(None, None, None) {
                                    Ok(files) => {
                                        this.last_picked_image = Some(format!("{} images", files.len()));
                                    }
                                    Err(e) => this.last_picked_image = Some(format!("Error: {e}")),
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            action_button("Video", TEAL, cx.listener(|this, _, _, cx| {
                                match gpui_mobile::packages::image_picker::pick_video(
                                    gpui_mobile::packages::image_picker::ImageSource::Gallery,
                                    gpui_mobile::packages::image_picker::CameraDevice::Rear,
                                ) {
                                    Ok(Some(f)) => this.last_picked_image = Some(f.name),
                                    Ok(None) => this.last_picked_image = Some("Cancelled".into()),
                                    Err(e) => this.last_picked_image = Some(format!("Error: {e}")),
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            action_button("Record", super::RED, cx.listener(|this, _, _, cx| {
                                match gpui_mobile::packages::image_picker::pick_video(
                                    gpui_mobile::packages::image_picker::ImageSource::Camera,
                                    gpui_mobile::packages::image_picker::CameraDevice::Rear,
                                ) {
                                    Ok(Some(f)) => this.last_picked_image = Some(f.name),
                                    Ok(None) => this.last_picked_image = Some("Cancelled".into()),
                                    Err(e) => this.last_picked_image = Some(format!("Error: {e}")),
                                }
                                cx.notify();
                            })),
                        ),
                )
        });

    // ── Camera ───────────────────────────────────────────────────────────────
    root = root
        .child(section_header("Camera", sub_text))
        .child({
            let camera_status = router.camera_status.as_deref().unwrap_or("Idle");
            let recording = router.camera_recording;
            let has_handle = router.camera_handle.is_some();

            // List cameras
            let cameras_label = match gpui_mobile::packages::camera::available_cameras() {
                Ok(cams) => {
                    let names: Vec<String> = cams
                        .iter()
                        .map(|c| format!("{:?} ({})", c.lens_direction, c.name))
                        .collect();
                    if names.is_empty() {
                        "No cameras found".to_string()
                    } else {
                        names.join(", ")
                    }
                }
                Err(e) => format!("Error: {e}"),
            };

            let mut card = info_card(card_bg)
                .child(kv_row("Cameras", &cameras_label, PEACH, text_color, sub_text))
                .child(divider_line(divider_color))
                .child(kv_row("Status", camera_status, PEACH, text_color, sub_text))
                .child(divider_line(divider_color));

            // Row 1: Open / Close / Switch
            card = card.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .p_3()
                    .child(
                        action_button(
                            if has_handle { "Preview" } else { "Open" },
                            if has_handle { GREEN } else { BLUE },
                            cx.listener(move |this, _, _, cx| {
                                if this.camera_handle.is_some() {
                                    // Toggle preview
                                    let handle = gpui_mobile::packages::camera::CameraHandle {
                                        id: this.camera_handle.unwrap(),
                                    };
                                    if this.camera_previewing {
                                        match gpui_mobile::packages::camera::stop_preview(&handle) {
                                            Ok(()) => {
                                                this.camera_previewing = false;
                                                this.camera_status = Some("Preview stopped".into());
                                            }
                                            Err(e) => this.camera_status = Some(format!("Error: {e}")),
                                        }
                                    } else {
                                        match gpui_mobile::packages::camera::start_preview(&handle) {
                                            Ok(()) => {
                                                this.camera_previewing = true;
                                                this.camera_status = Some("Preview active".into());
                                            }
                                            Err(e) => this.camera_status = Some(format!("Error: {e}")),
                                        }
                                    }
                                    std::mem::forget(handle);
                                } else {
                                    // Open back camera
                                    match gpui_mobile::packages::camera::available_cameras() {
                                        Ok(cams) => {
                                            let cam = cams.iter()
                                                .find(|c| c.lens_direction == gpui_mobile::packages::camera::CameraLensDirection::Back)
                                                .or(cams.first());
                                            if let Some(cam) = cam {
                                                match gpui_mobile::packages::camera::create_camera(
                                                    cam,
                                                    gpui_mobile::packages::camera::ResolutionPreset::High,
                                                    true,
                                                ) {
                                                    Ok(h) => {
                                                        this.camera_handle = Some(h.id);
                                                        this.camera_status = Some(format!("Opened: {}", cam.name));
                                                        std::mem::forget(h);
                                                    }
                                                    Err(e) => this.camera_status = Some(format!("Error: {e}")),
                                                }
                                            } else {
                                                this.camera_status = Some("No cameras available".into());
                                            }
                                        }
                                        Err(e) => this.camera_status = Some(format!("Error: {e}")),
                                    }
                                }
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        action_button("Switch", YELLOW, cx.listener(|this, _, _, cx| {
                            if let Some(id) = this.camera_handle {
                                let handle = gpui_mobile::packages::camera::CameraHandle { id };
                                match gpui_mobile::packages::camera::available_cameras() {
                                    Ok(cams) => {
                                        // Toggle between front and back
                                        let target_dir = if this.camera_status.as_deref()
                                            .map(|s| s.contains("Front"))
                                            .unwrap_or(false)
                                        {
                                            gpui_mobile::packages::camera::CameraLensDirection::Back
                                        } else {
                                            gpui_mobile::packages::camera::CameraLensDirection::Front
                                        };
                                        if let Some(cam) = cams.iter().find(|c| c.lens_direction == target_dir) {
                                            match gpui_mobile::packages::camera::set_camera(&handle, cam) {
                                                Ok(()) => this.camera_status = Some(format!("Switched to {:?}", cam.lens_direction)),
                                                Err(e) => this.camera_status = Some(format!("Error: {e}")),
                                            }
                                        }
                                    }
                                    Err(e) => this.camera_status = Some(format!("Error: {e}")),
                                }
                                std::mem::forget(handle);
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        action_button("Close", super::RED, cx.listener(|this, _, _, cx| {
                            if let Some(id) = this.camera_handle.take() {
                                let handle = gpui_mobile::packages::camera::CameraHandle { id };
                                let _ = gpui_mobile::packages::camera::dispose(handle);
                                this.camera_previewing = false;
                                this.camera_recording = false;
                                this.camera_status = Some("Closed".into());
                            }
                            cx.notify();
                        })),
                    ),
            );

            // Row 2: Take Photo / Record / Stop
            card = card.child(divider_line(divider_color)).child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .px_3()
                    .pb_3()
                    .child(
                        action_button("Photo", TEAL, cx.listener(|this, _, _, cx| {
                            if let Some(id) = this.camera_handle {
                                let handle = gpui_mobile::packages::camera::CameraHandle { id };
                                match gpui_mobile::packages::camera::take_picture(&handle) {
                                    Ok(img) => {
                                        this.camera_status = Some(format!(
                                            "Photo: {}x{}", img.width, img.height
                                        ));
                                    }
                                    Err(e) => this.camera_status = Some(format!("Error: {e}")),
                                }
                                std::mem::forget(handle);
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        action_button(
                            if recording { "Stop Rec" } else { "Record" },
                            if recording { super::RED } else { MAUVE },
                            cx.listener(move |this, _, _, cx| {
                                if let Some(id) = this.camera_handle {
                                    let handle = gpui_mobile::packages::camera::CameraHandle { id };
                                    if this.camera_recording {
                                        match gpui_mobile::packages::camera::stop_video_recording(&handle) {
                                            Ok(vid) => {
                                                this.camera_recording = false;
                                                this.camera_status = Some(format!("Video: {}", vid.path));
                                            }
                                            Err(e) => this.camera_status = Some(format!("Error: {e}")),
                                        }
                                    } else {
                                        match gpui_mobile::packages::camera::start_video_recording(&handle) {
                                            Ok(()) => {
                                                this.camera_recording = true;
                                                this.camera_status = Some("Recording...".into());
                                            }
                                            Err(e) => this.camera_status = Some(format!("Error: {e}")),
                                        }
                                    }
                                    std::mem::forget(handle);
                                }
                                cx.notify();
                            }),
                        ),
                    ),
            );

            // Row 3: Flash controls
            card = card.child(divider_line(divider_color)).child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .px_3()
                    .pb_3()
                    .child(action_button("Flash Off", SURFACE0, cx.listener(|this, _, _, cx| {
                        if let Some(id) = this.camera_handle {
                            let handle = gpui_mobile::packages::camera::CameraHandle { id };
                            let _ = gpui_mobile::packages::camera::set_flash_mode(
                                &handle, gpui_mobile::packages::camera::FlashMode::Off,
                            );
                            this.camera_status = Some("Flash: Off".into());
                            std::mem::forget(handle);
                        }
                        cx.notify();
                    })))
                    .child(action_button("Auto", BLUE, cx.listener(|this, _, _, cx| {
                        if let Some(id) = this.camera_handle {
                            let handle = gpui_mobile::packages::camera::CameraHandle { id };
                            let _ = gpui_mobile::packages::camera::set_flash_mode(
                                &handle, gpui_mobile::packages::camera::FlashMode::Auto,
                            );
                            this.camera_status = Some("Flash: Auto".into());
                            std::mem::forget(handle);
                        }
                        cx.notify();
                    })))
                    .child(action_button("Torch", YELLOW, cx.listener(|this, _, _, cx| {
                        if let Some(id) = this.camera_handle {
                            let handle = gpui_mobile::packages::camera::CameraHandle { id };
                            let _ = gpui_mobile::packages::camera::set_flash_mode(
                                &handle, gpui_mobile::packages::camera::FlashMode::Torch,
                            );
                            this.camera_status = Some("Flash: Torch".into());
                            std::mem::forget(handle);
                        }
                        cx.notify();
                    }))),
            );

            card
        });

    // ── Permission Handler ────────────────────────────────────────────────
    root = root
        .child(section_header("Permissions", sub_text))
        .child({
            let perm_status = router.perm_status.as_deref().unwrap_or("Tap to check");
            let mut card = info_card(card_bg)
                .child(kv_row("Status", perm_status, TEAL, text_color, sub_text))
                .child(divider_line(divider_color));

            // Row 1: Check permissions
            card = card.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .p_3()
                    .child(action_button("Camera", BLUE, cx.listener(|this, _, _, cx| {
                        match gpui_mobile::packages::permission_handler::check_permission(
                            gpui_mobile::packages::permission_handler::Permission::Camera,
                        ) {
                            Ok(s) => this.perm_status = Some(format!("Camera: {:?}", s)),
                            Err(e) => this.perm_status = Some(format!("Error: {e}")),
                        }
                        cx.notify();
                    })))
                    .child(action_button("Location", GREEN, cx.listener(|this, _, _, cx| {
                        match gpui_mobile::packages::permission_handler::check_permission(
                            gpui_mobile::packages::permission_handler::Permission::LocationWhenInUse,
                        ) {
                            Ok(s) => this.perm_status = Some(format!("Location: {:?}", s)),
                            Err(e) => this.perm_status = Some(format!("Error: {e}")),
                        }
                        cx.notify();
                    })))
                    .child(action_button("Photos", MAUVE, cx.listener(|this, _, _, cx| {
                        match gpui_mobile::packages::permission_handler::check_permission(
                            gpui_mobile::packages::permission_handler::Permission::Photos,
                        ) {
                            Ok(s) => this.perm_status = Some(format!("Photos: {:?}", s)),
                            Err(e) => this.perm_status = Some(format!("Error: {e}")),
                        }
                        cx.notify();
                    })))
                    .child(action_button("Notif", YELLOW, cx.listener(|this, _, _, cx| {
                        match gpui_mobile::packages::permission_handler::check_permission(
                            gpui_mobile::packages::permission_handler::Permission::Notification,
                        ) {
                            Ok(s) => this.perm_status = Some(format!("Notification: {:?}", s)),
                            Err(e) => this.perm_status = Some(format!("Error: {e}")),
                        }
                        cx.notify();
                    }))),
            );

            // Row 2: Request permissions + open settings
            card = card.child(divider_line(divider_color)).child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .px_3()
                    .pb_3()
                    .child(action_button("Request Cam", PEACH, cx.listener(|this, _, _, cx| {
                        match gpui_mobile::packages::permission_handler::request_permission(
                            gpui_mobile::packages::permission_handler::Permission::Camera,
                        ) {
                            Ok(s) => this.perm_status = Some(format!("Camera: {:?}", s)),
                            Err(e) => this.perm_status = Some(format!("Error: {e}")),
                        }
                        cx.notify();
                    })))
                    .child(action_button("Request Mic", TEAL, cx.listener(|this, _, _, cx| {
                        match gpui_mobile::packages::permission_handler::request_permission(
                            gpui_mobile::packages::permission_handler::Permission::Microphone,
                        ) {
                            Ok(s) => this.perm_status = Some(format!("Mic: {:?}", s)),
                            Err(e) => this.perm_status = Some(format!("Error: {e}")),
                        }
                        cx.notify();
                    })))
                    .child(action_button("Settings", SURFACE0, cx.listener(|this, _, _, cx| {
                        match gpui_mobile::packages::permission_handler::open_app_settings() {
                            Ok(true) => this.perm_status = Some("Settings opened".into()),
                            Ok(false) => this.perm_status = Some("Could not open settings".into()),
                            Err(e) => this.perm_status = Some(format!("Error: {e}")),
                        }
                        cx.notify();
                    }))),
            );

            card
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

fn action_button(
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
