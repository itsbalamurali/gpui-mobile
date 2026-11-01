use super::*;

// ── main event loop ───────────────────────────────────────────────────────────

/// The event loop that processes `android-activity` events and drives the
/// platform.
///
/// Called from `android_main` after the platform is initialised.
/// Runs until the platform requests quit or the activity is destroyed.
pub fn run_event_loop(app: &AndroidApp) {
    log::info!("run_event_loop: entering main loop");

    // Track whether the on_init_window callback has already been invoked.
    // We do NOT invoke it inside handle_main_event (which runs inside
    // poll_events) because the callback can be heavy (shader compilation,
    // GPUI Application setup).  Running it there blocks the event loop and
    // prevents the system's FocusEvent from being consumed, triggering an
    // ANR after 10 seconds.
    //
    // Instead we check each loop iteration: if a primary window exists and
    // the callback is still pending, invoke it *after* poll_events has
    // returned so focus/input events have already been drained.
    INIT_WINDOW_DONE.store(false, Ordering::Relaxed);
    // Vsync pacing lives on this thread's looper.
    super::super::frame_source::install();
    let mut iteration: u64 = 0;
    let mut last_heartbeat = std::time::Instant::now();
    let mut app_is_active = false;

    loop {
        iteration += 1;

        // Log a heartbeat every 5 seconds so we can tell if the loop is alive.
        let now = std::time::Instant::now();
        if now.duration_since(last_heartbeat) >= Duration::from_secs(5) {
            log::info!(
                "run_event_loop: heartbeat — iteration={}, init_done={}, active={}",
                iteration,
                INIT_WINDOW_DONE.load(Ordering::Relaxed),
                app_is_active,
            );
            last_heartbeat = now;
        }

        // Check if quit was requested.
        if let Some(platform) = platform() {
            if platform.should_quit() {
                log::info!("run_event_loop: platform requested quit");
                dispose_all_platform_views();
                break;
            }
            platform.tick();
        }

        // ── Poll for events ──
        //
        // Blocks until something reaches the looper: a lifecycle command,
        // input, a main-thread task, or the vsync callback that
        // `frame_source` posted for a wanted frame. The timeout only covers
        // the delayed dispatcher tasks `platform.tick()` has to release and
        // the clock fallback without a choreographer.
        let timeout = super::super::frame_source::poll_timeout(
            platform().and_then(|platform| platform.next_delayed_due()),
            INIT_WINDOW_DONE.load(Ordering::Relaxed) && app_is_active,
        );
        app.poll_events(Some(timeout), |event| match event {
            PollEvent::Main(main_event) => {
                handle_main_event(app, main_event);
            }
            PollEvent::Wake => {}
            PollEvent::Timeout => {}
            _ => {}
        });

        // ── Deferred lifecycle processing ──
        //
        // Between each handler, call poll_events again to drain any
        // events the Java UI thread may have queued while we were
        // processing.  This keeps the condvar wait short and prevents
        // the InputDispatcher ANR (10s timeout on MotionEvent delivery).
        //
        // Helper closure to drain events quickly:
        let drain_events = |app: &AndroidApp| {
            app.poll_events(Some(Duration::ZERO), |event| {
                if let PollEvent::Main(main_event) = event {
                    handle_main_event(app, main_event);
                }
            });
        };

        // 1. TerminateWindow — unconfigure surface, release native window
        if TERM_WINDOW_PENDING.swap(false, Ordering::Relaxed) {
            log::info!("deferred: TerminateWindow (iter={})", iteration);
            INIT_WINDOW_DONE.store(false, Ordering::Relaxed);
            *LAST_CHROME_STYLE.lock().unwrap() = None;
            if let Some(platform) = platform() {
                if let Some(win) = platform.primary_window() {
                    win.term_window();
                }
            }
            // Drain immediately — Java thread may be blocked on InitWindow condvar.
            drain_events(app);
        }

        // 2. InitWindow — replace surface on existing renderer, or create new
        if INIT_WINDOW_PENDING.swap(false, Ordering::Relaxed) {
            log::info!("deferred: InitWindow (iter={})", iteration);
            if let Some(platform) = platform() {
                if let Some(native_window) = app.native_window() {
                    let width = native_window.width();
                    let height = native_window.height();
                    log::info!("InitWindow: {}×{}", width, height);

                    platform.update_primary_display(&native_window, &app.asset_manager());

                    let scale_factor = platform
                        .primary_display()
                        .map(|d| d.scale_factor())
                        .unwrap_or(1.0);

                    if let Some(existing) = platform.primary_window() {
                        let gpu_ctx = platform.gpu_context();
                        match existing.init_window(native_window, gpu_ctx) {
                            Ok(()) => {
                                log::info!("InitWindow: reinitialised existing window");
                            }
                            Err(e) => {
                                log::error!("failed to reinit window surface: {e:#}");
                            }
                        }

                        // Trigger GPUI resize so layout adapts to new dimensions.
                        existing.handle_resize();

                        let cr = app.content_rect();
                        existing.update_safe_area_from_content_rect(
                            cr.left, cr.top, cr.right, cr.bottom,
                        );

                        INIT_WINDOW_DONE.store(true, Ordering::Relaxed);
                    } else {
                        match platform.open_window(native_window, scale_factor, false) {
                            Ok(win) => {
                                log::info!(
                                    "window opened — id={:#x} scale={:.1}",
                                    win.id(),
                                    scale_factor
                                );

                                let cr = app.content_rect();
                                win.update_safe_area_from_content_rect(
                                    cr.left, cr.top, cr.right, cr.bottom,
                                );
                            }
                            Err(e) => {
                                log::error!("failed to open window: {e:#}");
                            }
                        }
                    }
                }
            }
            // Drain — Java thread may have queued ConfigChanged/WindowResized.
            drain_events(app);
        }

        // 3. WindowResized
        if WINDOW_RESIZED_PENDING.swap(false, Ordering::Relaxed) {
            log::debug!("deferred: WindowResized");
            if let Some(platform) = platform() {
                if let Some(win) = platform.primary_window() {
                    win.handle_resize();
                    let cr = app.content_rect();
                    win.update_safe_area_from_content_rect(cr.left, cr.top, cr.right, cr.bottom);
                }
            }
        }

        // 4. ConfigChanged
        if CONFIG_CHANGED_PENDING.swap(false, Ordering::Relaxed) {
            log::debug!("deferred: ConfigChanged");
            if let Some(platform) = platform() {
                platform.notify_keyboard_layout_change();
                let is_dark = query_night_mode_via_jni();
                if let Some(win) = platform.primary_window() {
                    let appearance = if is_dark {
                        crate::android::window::WindowAppearance::Dark
                    } else {
                        crate::android::window::WindowAppearance::Light
                    };
                    win.set_appearance(appearance);
                }
            }
        }

        // 5. Pause / background
        if PAUSE_PENDING.swap(false, Ordering::Relaxed) {
            log::info!("deferred: Pause (iter={})", iteration);
            if let Some(platform) = platform() {
                platform.did_enter_background();
                if let Some(win) = platform.primary_window() {
                    win.set_active(false);
                }
            }
            // Pause platform views
            pause_platform_views();
        }

        // 6. Resume / foreground
        if RESUME_PENDING.swap(false, Ordering::Relaxed) {
            log::info!("deferred: Resume (iter={})", iteration);
            if let Some(platform) = platform() {
                platform.did_become_active();
                if let Some(win) = platform.primary_window() {
                    win.set_active(true);
                }
            }
            // Resume platform views
            resume_platform_views();
        }

        // Track active/focused state.
        if let Some(platform) = platform() {
            if let Some(win) = platform.primary_window() {
                let is_active = win.is_active();
                if is_active != app_is_active {
                    log::info!(
                        "run_event_loop: active {} -> {} (iter={})",
                        app_is_active,
                        is_active,
                        iteration,
                    );
                    app_is_active = is_active;
                }
            }
        }

        // Process input events.
        process_input_events(app);

        // Deferred initialisation callbacks (runs once).
        if !INIT_WINDOW_DONE.load(Ordering::Relaxed) {
            if let Some(platform) = platform() {
                if platform.primary_window().is_some() {
                    if let Some(finish_cb) = platform.take_finish_launching_callback() {
                        log::info!("invoking finish_launching callback (iter={})", iteration);
                        finish_cb();
                    }

                    if let Some(init_cb) = platform.take_on_init_window_callback() {
                        let win = platform.primary_window().unwrap();
                        log::info!("invoking on_init_window callback (iter={})", iteration);
                        init_cb(win);
                    }

                    INIT_WINDOW_DONE.store(true, Ordering::Relaxed);
                    NATIVE_INITIALIZED.store(true, Ordering::Release);
                    log::info!("NATIVE_INITIALIZED = true");

                    // Render first frame immediately.
                    platform.flush_main_thread_tasks();
                    if let Some(win) = platform.primary_window() {
                        win.request_frame();
                    }
                }
            }
        }

        // ── Render ──
        if let Some(platform) = platform() {
            if INIT_WINDOW_DONE.load(Ordering::Relaxed) && app_is_active {
                platform.flush_main_thread_tasks();
                // Software-keyboard text bypasses GPUI's invalidator; the
                // frame callback turns it into a forced render.
                if crate::TEXT_INPUT_DIRTY.load(Ordering::Acquire) {
                    super::super::frame_source::schedule_frame();
                }
                if super::super::frame_source::take_frame() {
                    if let Some(win) = platform.primary_window() {
                        win.request_frame();
                    }

                    // Drain lifecycle events that arrived during rendering
                    // (e.g. rotation triggers TerminateWindow while we were
                    // in get_current_texture / present).
                    drain_events(app);
                    process_input_events(app);
                }
            }
        }
    }

    log::info!("run_event_loop: exiting main loop");
}

/// Pause all platform views when the app goes to background.
fn pause_platform_views() {
    let _ = with_env(|env| {
        if let Ok(helper_class) = find_app_class(env, "dev.gpui.mobile.GpuiPlatformView") {
            let _ = env.call_static_method(
                &helper_class,
                jni::jni_str!("pauseAll"),
                jni::jni_sig!("()V"),
                &[],
            );
            env.exception_clear();
        }
        Ok(())
    });
}

/// Resume all platform views when the app returns to foreground.
fn resume_platform_views() {
    let _ = with_env(|env| {
        if let Ok(helper_class) = find_app_class(env, "dev.gpui.mobile.GpuiPlatformView") {
            let _ = env.call_static_method(
                &helper_class,
                jni::jni_str!("resumeAll"),
                jni::jni_sig!("()V"),
                &[],
            );
            env.exception_clear();
        }
        Ok(())
    });
}

/// Dispose all platform views during app shutdown.
fn dispose_all_platform_views() {
    let _ = with_env(|env| {
        if let Ok(helper_class) = find_app_class(env, "dev.gpui.mobile.GpuiPlatformView") {
            let _ = env.call_static_method(
                &helper_class,
                jni::jni_str!("disposeAll"),
                jni::jni_sig!("()V"),
                &[],
            );
            env.exception_clear();
        }
        Ok(())
    });
}

/// Handle a single `MainEvent` from `android-activity`.
fn handle_main_event(_app: &AndroidApp, event: MainEvent<'_>) {
    match event {
        MainEvent::InitWindow { .. } => {
            log::info!("MainEvent::InitWindow");
            // Defer to after poll_events to avoid deadlock with state lock.
            INIT_WINDOW_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::TerminateWindow { .. } => {
            log::info!("MainEvent::TerminateWindow");
            // Defer to after poll_events to avoid deadlock with state lock.
            TERM_WINDOW_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::WindowResized { .. } => {
            log::debug!("MainEvent::WindowResized");
            // Defer to after poll_events to avoid deadlock with state lock.
            WINDOW_RESIZED_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::GainedFocus => {
            log::info!("MainEvent::GainedFocus");
            RESUME_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::LostFocus => {
            log::info!("MainEvent::LostFocus");
            PAUSE_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::Resume { .. } => {
            log::info!("MainEvent::Resume");
            RESUME_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::Pause => {
            log::info!("MainEvent::Pause");
            // set_active uses AtomicBool so it never blocks.
            PAUSE_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::ConfigChanged { .. } => {
            log::debug!("MainEvent::ConfigChanged");
            CONFIG_CHANGED_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::Start => {
            log::info!("MainEvent::Start");
        }

        MainEvent::Stop => {
            log::info!("MainEvent::Stop");
        }

        MainEvent::SaveState { .. } => {
            log::info!("MainEvent::SaveState");
        }

        MainEvent::LowMemory => {
            log::warn!("MainEvent::LowMemory — consider releasing cached resources");
        }

        MainEvent::Destroy => {
            log::info!("MainEvent::Destroy");

            if let Some(platform) = platform() {
                platform.quit();
            }
        }

        MainEvent::InsetsChanged { .. } => {
            log::debug!("MainEvent::InsetsChanged");
            WINDOW_RESIZED_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::ContentRectChanged { .. } => {
            log::debug!("MainEvent::ContentRectChanged");
            WINDOW_RESIZED_PENDING.store(true, Ordering::Relaxed);
        }

        _ => {
            log::trace!("MainEvent: other variant");
        }
    }
}

// ── main loop helper (compat with existing code) ──────────────────────────────

/// Run one iteration of the event loop.
///
/// This is a compatibility wrapper for code that uses a manual poll loop.
/// Prefer `run_event_loop` for the standard event loop.
///
/// `timeout_ms` — how long to block waiting for events (milliseconds).
/// Pass `0` for non-blocking, `-1` to block indefinitely.
///
/// Returns `true` if the application should exit.
pub fn poll_events(timeout_ms: i32) -> bool {
    if let Some(platform) = platform() {
        if platform.should_quit() {
            return true;
        }
        platform.tick();
    }

    let app = match android_app() {
        Some(app) => app,
        None => return false,
    };

    let timeout = if timeout_ms < 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms as u64))
    };

    app.poll_events(timeout, |event| match event {
        PollEvent::Main(main_event) => {
            handle_main_event(&app, main_event);
        }
        PollEvent::Wake => {}
        _ => {}
    });

    process_input_events(&app);

    // Drive the GPUI rendering pipeline (same as run_event_loop).
    if let Some(platform) = platform() {
        platform.flush_main_thread_tasks();
        if let Some(win) = platform.primary_window() {
            win.request_frame();
        }
    }

    false
}
