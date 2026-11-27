//! Grimlite Ri Android runtime (`cdylib`, `android-activity` GameActivity + wgpu + egui).
//!
//! Turu / Mugetsu parity mobile architecture:
//! - **Two-Level Frame-Paced Loop**: Matches `turu`'s `run_loop_paced` + `Screen::step` — ticks
//!   `Player` ONLY when `Instant::now() >= next_frame` (default 24 FPS = 41.67ms budget), while
//!   socket packets, HTTP chunks, FlashCalls, and touch/key input are processed in the inner loop
//!   without premature ticks or vsync-stealing UI presents.
//! - **Opaque Hardware `GpuBlit` (`BlendState::REPLACE`)**: Draws the game texture directly into
//!   the surface viewport with zero alpha-blending and zero `pow(2.4)` gamma shader overhead,
//!   followed by `egui` rendering only the left rail, top navbar, and open tool windows.
//! - **1 GB SWF `HttpCache` + `DiskStorageBackend` + Mobile Memory Cap**: Caches `Game.swf`, maps,
//!   monsters, and gear on disk (`cache/gamefiles`), persists AQW SharedObjects, and enables
//!   `ruffle_render_wgpu::set_memory_pressure(true)` (64 MB texture pool cap) + periodic shape
//!   eviction so mobile GPUs never thrash system swap RAM.

#[cfg(target_os = "android")]
mod runtime {
    use android_activity::input::{
        ImeOptions, InputEvent, InputType, KeyAction, KeyMapChar, Keycode, MotionAction,
        TextInputAction, TextInputState,
    };
    use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
    use egui::{Context, Pos2, RawInput, Rect};
    use grimlite_bot::Bot;
    use grimlite_egui::{
        GrimliteUi, NAV_BAR_HEIGHT, TAB_RAIL_WIDTH, TabBarAction, TabDescriptor, TabLayout,
    };
    use grimlite_frontend::capture::CaptureNavigator;
    use grimlite_frontend::external::GrimliteExternalInterface;
    use grimlite_frontend::gpu::{GpuBlit, GpuTextureTarget, TabPlayerGuard};
    use grimlite_frontend::ruffle_host::{FlashCallRequest, RuffleFlashHost, answer_flash_call};
    use ruffle_core::backend::navigator::{OwnedFuture, SocketMode};
    use ruffle_core::backend::ui::{
        DialogResultFuture, FileFilter, FontDefinition, FullscreenError, LanguageIdentifier,
        MouseCursor, MultiDialogResultFuture, NullUiBackend, UiBackend,
    };
    use ruffle_core::events::{
        KeyDescriptor, KeyLocation, LogicalKey, MouseButton, NamedKey, PhysicalKey, PlayerEvent,
        TextControlCode,
    };
    use ruffle_core::font::FontQuery;
    use ruffle_core::{FloatDuration, LoadBehavior, Player, PlayerBuilder};
    use ruffle_frontend_utils::backends::navigator::{
        ExternalNavigatorBackend, FutureSpawner, NavigatorInterface,
    };
    use ruffle_frontend_utils::backends::storage::DiskStorageBackend;
    use ruffle_frontend_utils::content::{ContentDescriptor, PlayingContent};
    use ruffle_render::backend::ViewportDimensions;
    use ruffle_render::quality::StageQuality;
    use ruffle_render_wgpu::backend::{
        WgpuRenderBackend, create_wgpu_instance, request_adapter_and_device,
    };
    use ruffle_render_wgpu::descriptors::Descriptors;
    use std::collections::HashSet;
    use std::fs::File;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};
    use url::Url;

    /// Mugetsu 2.0 / Ruffle-AQW speed-up switches enabled before any worker threads start.
    const DEFAULT_ON_SWITCHES: [&str; 8] = [
        "RUFFLE_LEAN_SCRIPTS",
        "RUFFLE_LEAN_FRAMES",
        "RUFFLE_LEAN_RENDER",
        "RUFFLE_PAGE_BLENDS",
        "RUFFLE_SHAPE_UPLOADS",
        "RUFFLE_PICK_BOUNDS",
        "RUFFLE_SHARP_SNAPSHOTS",
        "RUFFLE_EARLY_SCRIPTS",
    ];

    /// Offscreen SWF render height cap (~660p, sharper than AQW's native 550p stage while saving
    /// ~60% fragment fillrate on Mali/Adreno GPUs).
    const MAX_GAME_RENDER_HEIGHT: u32 = 660;
    /// Longest gap between drawn pictures when skipping behind frames (`turu`'s `MAX_DRAW_GAP`).
    const MAX_DRAW_GAP: Duration = Duration::from_millis(250);
    /// Minimum interval between standalone UI-only draws (`turu`'s `UI_FRAME` = 33ms / 30Hz).
    const UI_FRAME: Duration = Duration::from_millis(33);
    /// Unused vector shape eviction cadence (`turu`'s `SWEEP_EVERY` = 60s, 5 min unused).
    const SHAPE_SWEEP_EVERY: Duration = Duration::from_secs(60);

    #[derive(Clone, Copy, Debug)]
    enum TouchKind {
        Down,
        Move,
        Up,
    }

    enum EngineMsg {
        InitWindow,
        ResizeWindow,
        DetachWindow(mpsc::SyncSender<()>),
        Paused(bool),
        Destroy,
        Touch {
            kind: TouchKind,
            px: f32,
            py: f32,
        },
        TextDelta {
            backspaces: usize,
            inserted: String,
        },
        KeyBackspace,
        KeyEnter,
        KeyChar(char),
        Task(u64, async_task::Runnable),
        FlashCall(u64, FlashCallRequest),
    }

    struct AndroidTab {
        id: u64,
        bot: Arc<Bot>,
        player: Arc<Mutex<Player>>,
        segments: Mutex<Vec<wgpu::CommandEncoder>>,
        ui: GrimliteUi,
        bg_accum_ms: f64,
    }

    struct LocalExecutor {
        tab_id: u64,
        tx: mpsc::Sender<EngineMsg>,
    }

    impl<E: std::error::Error + 'static> FutureSpawner<E> for LocalExecutor {
        fn spawn(&self, future: OwnedFuture<(), E>) {
            let future = async {
                if let Err(e) = future.await {
                    log::error!("Android navigator future error: {e}");
                }
            };
            let tab_id = self.tab_id;
            let tx = self.tx.clone();
            let schedule = move |task| {
                let _ = tx.send(EngineMsg::Task(tab_id, task));
            };
            let (runnable, task) = async_task::spawn_local(future, schedule);
            task.detach();
            runnable.schedule();
        }
    }

    #[derive(Clone, Default)]
    struct NoDialogs;

    impl NavigatorInterface for NoDialogs {
        fn navigate_to_website(&self, _url: Url) {}

        async fn open_file(&self, path: &Path) -> std::io::Result<File> {
            File::open(path)
        }

        async fn confirm_socket(&self, _host: &str, _port: u16) -> bool {
            true
        }
    }

    fn ensure_host_swf(data_dir: &Path) -> PathBuf {
        let _ = std::fs::create_dir_all(data_dir);
        let path = data_dir.join("grimlite.swf");
        let up_to_date = std::fs::read(&path).is_ok_and(|b| b == grimlite_bot::HOST_SWF);
        if !up_to_date {
            let _ = std::fs::write(&path, grimlite_bot::HOST_SWF);
        }
        path
    }

    fn show_ime(app: &AndroidApp, reset_state_flag: &AtomicBool) {
        reset_state_flag.store(true, Ordering::Release);
        app.set_ime_editor_info(
            InputType::TYPE_NULL,
            TextInputAction::None,
            ImeOptions::IMG_FLAG_NO_EXTRACT_UI | ImeOptions::IME_FLAG_NO_FULLSCREEN,
        );
        app.set_text_input_state(TextInputState::default());
        app.show_soft_input(false);
    }

    fn hide_ime(app: &AndroidApp, reset_state_flag: &AtomicBool) {
        reset_state_flag.store(true, Ordering::Release);
        app.hide_soft_input(false);
    }

    struct AndroidUiBackend {
        inner: NullUiBackend,
        app: AndroidApp,
        kb_open: Arc<AtomicBool>,
        ime_reset_flag: Arc<AtomicBool>,
    }

    impl AndroidUiBackend {
        fn new(
            app: AndroidApp,
            kb_open: Arc<AtomicBool>,
            ime_reset_flag: Arc<AtomicBool>,
        ) -> Self {
            Self {
                inner: NullUiBackend::new(),
                app,
                kb_open,
                ime_reset_flag,
            }
        }
    }

    impl UiBackend for AndroidUiBackend {
        fn mouse_visible(&self) -> bool {
            self.inner.mouse_visible()
        }
        fn set_mouse_visible(&mut self, visible: bool) {
            self.inner.set_mouse_visible(visible);
        }
        fn set_mouse_cursor(&mut self, cursor: MouseCursor) {
            self.inner.set_mouse_cursor(cursor);
        }
        fn clipboard_content(&mut self) -> String {
            self.inner.clipboard_content()
        }
        fn set_clipboard_content(&mut self, content: String) {
            self.inner.set_clipboard_content(content);
        }
        fn set_fullscreen(&mut self, is_full: bool) -> Result<(), FullscreenError> {
            self.inner.set_fullscreen(is_full)
        }
        fn display_root_movie_download_failed_message(
            &self,
            invalid_swf: bool,
            fetch_error: String,
        ) {
            self.inner
                .display_root_movie_download_failed_message(invalid_swf, fetch_error);
        }
        fn message(&self, message: &str) {
            self.inner.message(message);
        }
        fn open_virtual_keyboard(&self) {
            if !self.kb_open.swap(true, Ordering::AcqRel) {
                show_ime(&self.app, &self.ime_reset_flag);
            }
        }
        fn close_virtual_keyboard(&self) {
            if self.kb_open.swap(false, Ordering::AcqRel) {
                hide_ime(&self.app, &self.ime_reset_flag);
            }
        }
        fn supports_touch_events(&self) -> bool {
            true
        }
        fn language(&self) -> LanguageIdentifier {
            self.inner.language()
        }
        fn display_unsupported_video(&self, url: Url) {
            self.inner.display_unsupported_video(url);
        }
        fn load_device_font(
            &self,
            query: &FontQuery,
            register: &mut dyn FnMut(FontDefinition),
        ) {
            self.inner.load_device_font(query, register);
        }
        fn sort_device_fonts(
            &self,
            query: &FontQuery,
            register: &mut dyn FnMut(FontDefinition),
        ) -> Vec<FontQuery> {
            self.inner.sort_device_fonts(query, register)
        }
        fn display_file_open_dialog(&mut self, filters: Vec<FileFilter>) -> Option<DialogResultFuture> {
            self.inner.display_file_open_dialog(filters)
        }
        fn display_file_open_dialog_multiple(
            &mut self,
            filters: Vec<FileFilter>,
        ) -> Option<MultiDialogResultFuture> {
            self.inner.display_file_open_dialog_multiple(filters)
        }
        fn display_file_save_dialog(
            &mut self,
            file_name: String,
            title: String,
        ) -> Option<DialogResultFuture> {
            self.inner.display_file_save_dialog(file_name, title)
        }
        fn close_file_dialog(&mut self) {
            self.inner.close_file_dialog();
        }
    }

    /// Computes physical viewport rect `(left_px, top_px, raw_w, raw_h)` and scaled offscreen
    /// render dimensions `(drawn_w, drawn_h, render_scale)`.
    fn compute_game_layout(
        win_w: u32,
        win_h: u32,
        ppp: f32,
        flash_kb_bar: bool,
    ) -> (f32, f32, u32, u32, u32, u32, f64) {
        let left_px = (TAB_RAIL_WIDTH * ppp).round();
        let top_logical = if flash_kb_bar {
            NAV_BAR_HEIGHT + 34.0
        } else {
            NAV_BAR_HEIGHT
        };
        let top_px = (top_logical * ppp).round();
        let raw_w = win_w.saturating_sub(left_px as u32).max(1);
        let raw_h = win_h.saturating_sub(top_px as u32).max(1);
        let render_scale = if raw_h > MAX_GAME_RENDER_HEIGHT {
            f64::from(MAX_GAME_RENDER_HEIGHT) / f64::from(raw_h)
        } else {
            1.0
        };
        let drawn_w = ((f64::from(raw_w) * render_scale).round() as u32).max(1);
        let drawn_h = ((f64::from(raw_h) * render_scale).round() as u32).max(1);
        (left_px, top_px, raw_w, raw_h, drawn_w, drawn_h, render_scale)
    }

    /// Reads the live text and password flag directly from Ruffle's currently focused `EditText`.
    fn inspect_focused_flash_field(player: &mut Player) -> Option<(String, bool)> {
        player.mutate_with_update_context(|context| {
            let field = context.focus_tracker.get_as_edit_text()?;
            if !field.is_editable() {
                return None;
            }
            Some((
                field.text().to_utf8_lossy().into_owned(),
                field.is_password(),
            ))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_tab(
        app: &AndroidApp,
        data_dir: &Path,
        kb_open: Arc<AtomicBool>,
        ime_reset_flag: Arc<AtomicBool>,
        tab_id: u64,
        descriptors: Arc<Descriptors>,
        blit: Arc<GpuBlit>,
        swf_url: &Url,
        engine_tx: mpsc::Sender<EngineMsg>,
        win_w: u32,
        win_h: u32,
        ppp: f32,
    ) -> AndroidTab {
        let bot = Bot::new();
        let (_, _, _, _, drawn_w, drawn_h, render_scale) =
            compute_game_layout(win_w, win_h, ppp, false);
        let target = GpuTextureTarget::new(blit, &descriptors.device, drawn_w, drawn_h);
        let renderer = WgpuRenderBackend::new(descriptors, target)
            .expect("Android WgpuRenderBackend<GpuTextureTarget>");

        let content = PlayingContent::DirectFile(ContentDescriptor {
            url: swf_url.clone(),
            root_content_path: None,
        });
        let raw_nav = ExternalNavigatorBackend::new(
            swf_url.clone(),
            None,
            None,
            LocalExecutor {
                tab_id,
                tx: engine_tx.clone(),
            },
            None,
            false,
            HashSet::new(),
            SocketMode::Allow,
            Rc::new(content),
            NoDialogs,
        )
        .with_http_cache(grimlite_frontend::shared_http_cache(
            &data_dir.join("cache").join("gamefiles"),
        ));
        let navigator = CaptureNavigator::new(raw_nav, Some(bot.capture.clone()));

        let shared_objects_dir = data_dir.join("SharedObjects").join(format!("tab_{tab_id}"));
        let player = PlayerBuilder::new()
            .with_renderer(renderer)
            .with_navigator(navigator)
            .with_storage(Box::new(DiskStorageBackend::new(shared_objects_dir)))
            .with_ui(AndroidUiBackend::new(app.clone(), kb_open, ime_reset_flag))
            .with_external_interface(Box::new(GrimliteExternalInterface::new(bot.clone())))
            .with_letterbox(ruffle_core::config::Letterbox::On)
            .with_quality(StageQuality::High)
            .with_max_execution_duration(Duration::MAX)
            .with_load_behavior(LoadBehavior::Streaming)
            .with_autoplay(true)
            .build();

        let flash_tx = engine_tx;
        let host = Arc::new(RuffleFlashHost::new(
            std::thread::current().id(),
            move |req| {
                flash_tx
                    .send(EngineMsg::FlashCall(tab_id, req))
                    .map_err(|_| "android engine closed".to_string())
            },
        ));
        bot.set_host(host);

        let segments = Mutex::new(Vec::new());
        {
            let mut p = TabPlayerGuard::lock(&player, &segments);
            p.set_viewport_dimensions(ViewportDimensions {
                width: drawn_w,
                height: drawn_h,
                scale_factor: f64::from(ppp) * render_scale,
            });
            p.fetch_root_movie(swf_url.to_string(), vec![], Box::new(|_| {}));
        }

        let ui = GrimliteUi::new(bot.clone(), None, None);
        AndroidTab {
            id: tab_id,
            bot,
            player,
            segments,
            ui,
            bg_accum_ms: 0.0,
        }
    }

    fn resize_tab_viewport(tab: &mut AndroidTab, win_w: u32, win_h: u32, ppp: f32) {
        let (_, _, _, _, drawn_w, drawn_h, render_scale) =
            compute_game_layout(win_w, win_h, ppp, false);
        let mut p = TabPlayerGuard::lock(&tab.player, &tab.segments);
        p.set_viewport_dimensions(ViewportDimensions {
            width: drawn_w,
            height: drawn_h,
            scale_factor: f64::from(ppp) * render_scale,
        });
    }

    fn init_android_tls(app: &AndroidApp) {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let activity_raw = app.activity_as_ptr() as jni::sys::jobject;
        let res: Result<(), Box<dyn std::error::Error>> = vm.attach_current_thread(|env| {
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&activity_raw) }?;
            let context = env.new_local_ref(&*activity)?;
            rustls_platform_verifier::android::init_with_env(env, context)?;
            Ok(())
        });
        if let Err(e) = res {
            log::error!("Failed to initialize Android TLS verifier: {e}");
        }
    }

    fn make_raw_surface<'a>(
        instance: &wgpu::Instance,
        native_win: &ndk::native_window::NativeWindow,
    ) -> Result<wgpu::Surface<'a>, wgpu::CreateSurfaceError> {
        let raw_window_handle = wgpu::rwh::RawWindowHandle::AndroidNdk(
            wgpu::rwh::AndroidNdkWindowHandle::new(native_win.ptr().cast()),
        );
        let raw_display_handle =
            wgpu::rwh::RawDisplayHandle::Android(wgpu::rwh::AndroidDisplayHandle::new());
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(raw_display_handle),
            raw_window_handle,
        };
        unsafe { instance.create_surface_unsafe(target) }
    }

    fn create_mobile_gpu(
        native_win: &ndk::native_window::NativeWindow,
    ) -> Option<(
        Arc<Descriptors>,
        Arc<GpuBlit>,
        wgpu::RenderPipeline,
        wgpu::Surface<'static>,
        wgpu::SurfaceConfiguration,
    )> {
        let w = native_win.width().max(1) as u32;
        let h = native_win.height().max(1) as u32;
        // Prefer OpenGL ES first (matches Turu's `Graphics::Auto`), falling back cleanly to Vulkan.
        for backend in [wgpu::Backends::GL, wgpu::Backends::VULKAN] {
            let instance = create_wgpu_instance(backend, wgpu::BackendOptions::default(), None);
            let Ok(surf) = make_raw_surface(&instance, native_win) else {
                continue;
            };
            let Ok((adapter, device, queue)) =
                futures::executor::block_on(request_adapter_and_device(
                    backend,
                    &instance,
                    Some(&surf),
                    wgpu::PowerPreference::HighPerformance,
                ))
            else {
                continue;
            };
            let info = adapter.get_info();
            log::info!(
                "Graphics initialized: {} on {} ({:?})",
                info.backend.to_str(),
                info.name,
                info.device_type
            );
            let caps = surf.get_capabilities(&adapter);
            let format = [
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Bgra8Unorm,
            ]
            .into_iter()
            .find(|f| caps.formats.contains(f))
            .or_else(|| caps.formats.first().copied())
            .unwrap_or(wgpu::TextureFormat::Rgba8Unorm);
            let cfg = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                color_space: wgpu::SurfaceColorSpace::Auto,
                width: w,
                height: h,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: caps
                    .alpha_modes
                    .first()
                    .copied()
                    .unwrap_or(wgpu::CompositeAlphaMode::Auto),
                view_formats: vec![],
            };
            surf.configure(&device, &cfg);
            let blit = Arc::new(GpuBlit::new(&device));
            let blit_pipeline = blit.create_pipeline(&device, format);
            let desc = Arc::new(Descriptors::new(instance, adapter, device, queue));
            return Some((desc, blit, blit_pipeline, surf, cfg));
        }
        log::error!("Failed to initialize both OpenGL ES and Vulkan backends on Android");
        None
    }

    fn send_flash_backspace(p: &mut Player) {
        let key = KeyDescriptor {
            physical_key: PhysicalKey::Backspace,
            logical_key: LogicalKey::Named(NamedKey::Backspace),
            key_location: KeyLocation::Standard,
        };
        p.handle_event(PlayerEvent::KeyDown { key });
        p.handle_event(PlayerEvent::TextControl {
            code: TextControlCode::Backspace,
        });
        p.handle_event(PlayerEvent::KeyUp { key });
    }

    fn send_flash_enter(p: &mut Player) {
        let key = KeyDescriptor {
            physical_key: PhysicalKey::Enter,
            logical_key: LogicalKey::Named(NamedKey::Enter),
            key_location: KeyLocation::Standard,
        };
        p.handle_event(PlayerEvent::KeyDown { key });
        p.handle_event(PlayerEvent::TextControl {
            code: TextControlCode::Enter,
        });
        p.handle_event(PlayerEvent::KeyUp { key });
    }

    fn push_egui_touch(events: &mut Vec<egui::Event>, kind: TouchKind, pos: Pos2) {
        let phase = match kind {
            TouchKind::Down => egui::TouchPhase::Start,
            TouchKind::Move => egui::TouchPhase::Move,
            TouchKind::Up => egui::TouchPhase::End,
        };
        events.push(egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(0),
            phase,
            pos,
            force: None,
        });
        match kind {
            TouchKind::Down => {
                events.push(egui::Event::PointerMoved(pos));
                events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            TouchKind::Move => {
                events.push(egui::Event::PointerMoved(pos));
            }
            TouchKind::Up => {
                events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
                events.push(egui::Event::PointerGone);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_screen(
        app: &AndroidApp,
        data_dir: &Path,
        swf_url: &Url,
        engine_tx: &mpsc::Sender<EngineMsg>,
        kb_open: &Arc<AtomicBool>,
        ime_reset_flag: &Arc<AtomicBool>,
        egui_ctx: &Context,
        scale_factor: f32,
        started: Instant,
        desc: &Arc<Descriptors>,
        blit: &Arc<GpuBlit>,
        blit_pipeline: &wgpu::RenderPipeline,
        surf: &wgpu::Surface<'static>,
        cfg: &wgpu::SurfaceConfiguration,
        renderer: &mut egui_wgpu::Renderer,
        tabs: &mut Vec<AndroidTab>,
        active_idx: &mut usize,
        next_tab_id: &mut u64,
        egui_ime_active: &mut bool,
        kb_draft: &mut String,
        kb_masked: &mut bool,
        kb_manual_mask: &mut Option<bool>,
        pending_egui_events: &mut Vec<egui::Event>,
        ui_due: &mut Option<Instant>,
        needs_game_render: &mut bool,
    ) {
        let mut raw_input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(
                    cfg.width as f32 / scale_factor,
                    cfg.height as f32 / scale_factor,
                ),
            )),
            time: Some(started.elapsed().as_secs_f64()),
            events: std::mem::take(pending_egui_events),
            max_texture_side: Some(desc.limits.max_texture_dimension_2d as usize),
            ..Default::default()
        };
        if let Some(vp) = raw_input.viewports.get_mut(&egui::ViewportId::ROOT) {
            vp.native_pixels_per_point = Some(scale_factor);
        }

        let mut tab_action = TabBarAction::None;
        let active_id = tabs.get(*active_idx).map(|t| t.id).unwrap_or(0);
        let descriptors_ui: Vec<TabDescriptor> = tabs
            .iter()
            .map(|t| {
                let snap = t.bot.player_snapshot();
                let label = if snap.logged_in && !snap.username.trim().is_empty() {
                    snap.username
                } else {
                    format!("Account #{}", t.id)
                };
                TabDescriptor {
                    id: t.id,
                    label,
                    logged_in: snap.logged_in,
                    bot_running: t.bot.runner.is_running()
                        || t.bot.auto_attack.is_running()
                        || t.bot.maid.is_running(),
                }
            })
            .collect();

        let mut kb_action_enter = false;
        let mut kb_action_close = false;
        let flash_kb_visible = kb_open.load(Ordering::Acquire) && !*egui_ime_active;

        // Note: NO `CentralPanel` `ui.image` here — the game texture is blitted directly onto the
        // surface via `GpuBlit` (`BlendState::REPLACE`) underneath egui's chrome & windows!
        let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
            tab_action =
                grimlite_egui::show_tabs(ui, TabLayout::LeftRail, &descriptors_ui, active_id);
            if let Some(tab) = tabs.get_mut(*active_idx) {
                tab.ui.show(egui_ctx, ui);

                if flash_kb_visible {
                    egui::Panel::top("grimlite_android_kb_bar")
                        .exact_size(34.0)
                        .frame(
                            egui::Frame::new()
                                .fill(grimlite_egui::theme::Palette::BG_SURFACE)
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    grimlite_egui::theme::Palette::BORDER_FOCUS,
                                )),
                        )
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("Typing:")
                                        .small()
                                        .strong()
                                        .color(grimlite_egui::theme::Palette::BORDER_FOCUS),
                                );
                                let preview = if *kb_masked && !kb_draft.is_empty() {
                                    "•".repeat(kb_draft.chars().count())
                                } else if kb_draft.is_empty() {
                                    "Type on keyboard...".to_string()
                                } else {
                                    kb_draft.clone()
                                };
                                let preview_color = if kb_draft.is_empty() {
                                    grimlite_egui::theme::Palette::TEXT_MUTED
                                } else {
                                    egui::Color32::WHITE
                                };
                                egui::Frame::new()
                                    .fill(grimlite_egui::theme::Palette::BG_INPUT)
                                    .corner_radius(egui::CornerRadius::same(4))
                                    .inner_margin(egui::Margin::symmetric(8, 3))
                                    .show(ui, |ui| {
                                        ui.set_min_width(
                                            (ui.available_width() - 165.0).max(120.0),
                                        );
                                        ui.label(
                                            egui::RichText::new(preview)
                                                .monospace()
                                                .color(preview_color),
                                        );
                                    });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("Done").clicked() {
                                            kb_action_close = true;
                                        }
                                        if grimlite_egui::theme::primary_button(
                                            ui,
                                            "Enter",
                                            [48.0, 22.0],
                                        )
                                        .clicked()
                                        {
                                            kb_action_enter = true;
                                        }
                                        if ui
                                            .selectable_label(*kb_masked, "•••")
                                            .on_hover_text("Hide/show typed text")
                                            .clicked()
                                        {
                                            *kb_masked = !*kb_masked;
                                            *kb_manual_mask = Some(*kb_masked);
                                        }
                                    },
                                );
                            });
                        });
                }
            }
        });

        *ui_due = full_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .and_then(|vp| Instant::now().checked_add(vp.repaint_delay));

        if kb_action_enter {
            if let Some(tab) = tabs.get(*active_idx)
                && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
            {
                send_flash_enter(&mut p);
                *needs_game_render = true;
            }
            kb_draft.clear();
            kb_open.store(false, Ordering::Release);
            hide_ime(app, ime_reset_flag);
        } else if kb_action_close {
            kb_draft.clear();
            kb_open.store(false, Ordering::Release);
            hide_ime(app, ime_reset_flag);
        }

        let wants_egui_ime = full_output.platform_output.ime.is_some();
        if wants_egui_ime != *egui_ime_active {
            *egui_ime_active = wants_egui_ime;
            if wants_egui_ime {
                show_ime(app, ime_reset_flag);
            } else if !kb_open.load(Ordering::Acquire) {
                hide_ime(app, ime_reset_flag);
            }
        }

        match tab_action {
            TabBarAction::NewTab => {
                let tab = spawn_tab(
                    app,
                    data_dir,
                    kb_open.clone(),
                    ime_reset_flag.clone(),
                    *next_tab_id,
                    desc.clone(),
                    blit.clone(),
                    swf_url,
                    engine_tx.clone(),
                    cfg.width,
                    cfg.height,
                    scale_factor,
                );
                *next_tab_id += 1;
                tabs.push(tab);
                *active_idx = tabs.len() - 1;
                *needs_game_render = true;
            }
            TabBarAction::Switch(id) => {
                if let Some(idx) = tabs.iter().position(|t| t.id == id) {
                    *active_idx = idx;
                    *needs_game_render = true;
                }
            }
            TabBarAction::Close(id) if tabs.len() > 1 => {
                if let Some(idx) = tabs.iter().position(|t| t.id == id) {
                    let removed = tabs.remove(idx);
                    removed.bot.stop_bot();
                    removed.bot.auto_attack.stop(removed.bot.clone());
                    removed.bot.maid.stop(&removed.bot);
                    if *active_idx >= tabs.len() {
                        *active_idx = tabs.len() - 1;
                    }
                    *needs_game_render = true;
                }
            }
            _ => {}
        }

        let clipped = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [cfg.width, cfg.height],
            pixels_per_point: scale_factor,
        };
        for (id, deltas) in &full_output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(&desc.device, &desc.queue, *id, delta);
            }
        }

        match surf.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = desc.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor {
                        label: Some("grimlite_android_present"),
                    },
                );
                let mut cmds = renderer.update_buffers(
                    &desc.device,
                    &desc.queue,
                    &mut encoder,
                    &clipped,
                    &screen,
                );
                {
                    let mut pass = encoder
                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("grimlite_android_pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        })
                        .forget_lifetime();

                    // Blit active game texture opaquely (`BlendState::REPLACE`) into its viewport!
                    if let Some(tab) = tabs.get(*active_idx)
                        && let Some(p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                        && let Some(movie) = <dyn std::any::Any>::downcast_ref::<
                            WgpuRenderBackend<GpuTextureTarget>,
                        >(p.renderer())
                        .map(|r| r.target())
                    {
                        let (left_px, top_px, raw_w, raw_h, _, _, _) =
                            compute_game_layout(cfg.width, cfg.height, scale_factor, flash_kb_visible);
                        pass.set_viewport(
                            left_px,
                            top_px,
                            raw_w as f32,
                            raw_h as f32,
                            0.0,
                            1.0,
                        );
                        blit.draw(blit_pipeline, &movie.bind_group, &mut pass);
                        pass.set_viewport(0.0, 0.0, cfg.width as f32, cfg.height as f32, 0.0, 1.0);
                    }

                    renderer.render(&mut pass, &clipped, &screen);
                }
                cmds.push(encoder.finish());
                desc.queue.submit(cmds);
                desc.queue.present(frame);
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                surf.configure(&desc.device, cfg);
            }
            _ => {}
        }

        for id in &full_output.textures_delta.free {
            renderer.free_texture(id);
        }
        full_output.textures_delta.clear();
    }

    fn engine_loop(
        app: AndroidApp,
        data_dir: PathBuf,
        swf_url: Url,
        engine_tx: mpsc::Sender<EngineMsg>,
        engine_rx: mpsc::Receiver<EngineMsg>,
        kb_open: Arc<AtomicBool>,
        ime_reset_flag: Arc<AtomicBool>,
    ) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let _rt_guard = rt.enter();

        // Thread-local Ruffle-AQW + mobile VRAM settings (matching Turu's `engine` + `memory::init`):
        ruffle_core::set_show_all_apis(true);
        ruffle_core::set_forgiving_removal(true);
        ruffle_core::set_shape_wait(Some(Duration::from_millis(12)));
        ruffle_render::tessellator::set_flash_aa(true);
        ruffle_render_wgpu::set_memory_pressure(true);

        let egui_ctx = Context::default();
        grimlite_egui::theme::apply(&egui_ctx);

        let scale_factor: f32 = app
            .config()
            .density()
            .map_or(2.0, |dpi| (dpi as f32 / 160.0).clamp(1.5, 3.0));

        // Declared BEFORE `surface` so local drop order drops `surface` first, then `held_window`.
        let mut held_window: Option<ndk::native_window::NativeWindow> = None;
        let mut descriptors: Option<Arc<Descriptors>> = None;
        let mut blit: Option<Arc<GpuBlit>> = None;
        let mut blit_pipeline: Option<wgpu::RenderPipeline> = None;
        let mut surface: Option<wgpu::Surface<'static>> = None;
        let mut surface_config: Option<wgpu::SurfaceConfiguration> = None;
        let mut egui_renderer: Option<egui_wgpu::Renderer> = None;

        let mut tabs: Vec<AndroidTab> = Vec::new();
        let mut active_idx: usize = 0;
        let mut next_tab_id: u64 = 1;
        let mut paused = false;
        let mut egui_ime_active = false;
        let mut kb_draft = String::new();
        let mut kb_masked = false;
        let mut kb_manual_mask: Option<bool> = None;
        let mut needs_game_render = true;
        let mut ui_dirty = true;
        let mut ui_due: Option<Instant> = None;
        let mut pending_egui_events: Vec<egui::Event> = Vec::new();
        let started = Instant::now();
        let mut last_tick = Instant::now();
        let mut last_draw = Instant::now() - Duration::from_secs(1);
        let mut last_picture = Instant::now() - Duration::from_secs(1);
        let mut last_shape_sweep = Instant::now();
        let mut touch_on_ui = false;

        #[allow(clippy::too_many_arguments)]
        fn handle_msg(
            msg: EngineMsg,
            app: &AndroidApp,
            data_dir: &Path,
            swf_url: &Url,
            engine_tx: &mpsc::Sender<EngineMsg>,
            kb_open: &Arc<AtomicBool>,
            ime_reset_flag: &Arc<AtomicBool>,
            egui_ctx: &Context,
            scale_factor: f32,
            egui_ime_active: bool,
            descriptors: &mut Option<Arc<Descriptors>>,
            blit: &mut Option<Arc<GpuBlit>>,
            blit_pipeline: &mut Option<wgpu::RenderPipeline>,
            surface: &mut Option<wgpu::Surface<'static>>,
            surface_config: &mut Option<wgpu::SurfaceConfiguration>,
            egui_renderer: &mut Option<egui_wgpu::Renderer>,
            held_window: &mut Option<ndk::native_window::NativeWindow>,
            tabs: &mut Vec<AndroidTab>,
            active_idx: &mut usize,
            next_tab_id: &mut u64,
            paused: &mut bool,
            needs_game_render: &mut bool,
            ui_dirty: &mut bool,
            pending_egui_events: &mut Vec<egui::Event>,
            kb_draft: &mut String,
            touch_on_ui: &mut bool,
        ) -> bool {
            match msg {
                EngineMsg::Destroy => return true,
                EngineMsg::Paused(p) => {
                    *paused = p;
                    if !p {
                        *needs_game_render = true;
                        *ui_dirty = true;
                    }
                }
                EngineMsg::DetachWindow(done) => {
                    *surface = None;
                    *held_window = None;
                    let _ = done.send(());
                    let _ = ruffle_core::evict_unused_shapes(120);
                }
                EngineMsg::InitWindow => {
                    if let Some(native_win) = app.native_window() {
                        let w = native_win.width().max(1) as u32;
                        let h = native_win.height().max(1) as u32;
                        if let Some(desc) = descriptors.as_ref() {
                            *surface = None;
                            if let Ok(surf) = make_raw_surface(&desc.wgpu_instance, &native_win) {
                                if let Some(cfg) = surface_config.as_mut() {
                                    cfg.width = w;
                                    cfg.height = h;
                                    surf.configure(&desc.device, cfg);
                                }
                                for tab in tabs.iter_mut() {
                                    resize_tab_viewport(tab, w, h, scale_factor);
                                }
                                *surface = Some(surf);
                                *held_window = Some(native_win);
                                *needs_game_render = true;
                                *ui_dirty = true;
                            }
                        } else if let Some((desc, gpu_blit, pipe, surf, cfg)) =
                            create_mobile_gpu(&native_win)
                        {
                            let renderer = egui_wgpu::Renderer::new(
                                &desc.device,
                                cfg.format,
                                egui_wgpu::RendererOptions {
                                    msaa_samples: 1,
                                    depth_stencil_format: None,
                                    dithering: false,
                                    predictable_texture_filtering: false,
                                },
                            );
                            if tabs.is_empty() {
                                let tab = spawn_tab(
                                    app,
                                    data_dir,
                                    kb_open.clone(),
                                    ime_reset_flag.clone(),
                                    *next_tab_id,
                                    desc.clone(),
                                    gpu_blit.clone(),
                                    swf_url,
                                    engine_tx.clone(),
                                    w,
                                    h,
                                    scale_factor,
                                );
                                *next_tab_id += 1;
                                tabs.push(tab);
                                *active_idx = 0;
                            }
                            *descriptors = Some(desc);
                            *blit = Some(gpu_blit);
                            *blit_pipeline = Some(pipe);
                            *surface = Some(surf);
                            *surface_config = Some(cfg);
                            *egui_renderer = Some(renderer);
                            *held_window = Some(native_win);
                            *needs_game_render = true;
                            *ui_dirty = true;
                        }
                    }
                }
                EngineMsg::ResizeWindow => {
                    if let (Some(native_win), Some(desc), Some(surf), Some(cfg)) = (
                        app.native_window(),
                        descriptors.as_ref(),
                        surface.as_ref(),
                        surface_config.as_mut(),
                    ) {
                        let w = native_win.width().max(1) as u32;
                        let h = native_win.height().max(1) as u32;
                        if (w, h) != (cfg.width, cfg.height) {
                            cfg.width = w;
                            cfg.height = h;
                            surf.configure(&desc.device, cfg);
                            for tab in tabs.iter_mut() {
                                resize_tab_viewport(tab, w, h, scale_factor);
                            }
                            *needs_game_render = true;
                            *ui_dirty = true;
                        }
                    }
                }
                EngineMsg::Touch { kind, px, py } => {
                    let logical = Pos2::new(px / scale_factor, py / scale_factor);
                    let flash_kb_bar = kb_open.load(Ordering::Acquire) && !egui_ime_active;
                    let top_chrome_h = if flash_kb_bar {
                        NAV_BAR_HEIGHT + 34.0
                    } else {
                        NAV_BAR_HEIGHT
                    };
                    if matches!(kind, TouchKind::Down) {
                        *touch_on_ui = px < TAB_RAIL_WIDTH * scale_factor
                            || py < top_chrome_h * scale_factor
                            || egui_ctx
                                .layer_id_at(logical)
                                .is_some_and(|l| l.order != egui::Order::Background);
                    }
                    if *touch_on_ui || !matches!(kind, TouchKind::Move) {
                        push_egui_touch(pending_egui_events, kind, logical);
                        if *touch_on_ui {
                            *ui_dirty = true;
                        }
                    }
                    if !*touch_on_ui
                        && let Some(tab) = tabs.get(*active_idx)
                        && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                    {
                        let (win_w, win_h) = surface_config
                            .as_ref()
                            .map_or((1920, 1080), |c| (c.width, c.height));
                        let (left_px, top_px, _, _, _, _, render_scale) =
                            compute_game_layout(win_w, win_h, scale_factor, flash_kb_bar);
                        let gx = f64::from((px - left_px).max(0.0)) * render_scale;
                        let gy = f64::from((py - top_px).max(0.0)) * render_scale;
                        match kind {
                            TouchKind::Down => {
                                p.set_mouse_in_stage(true);
                                p.handle_event(PlayerEvent::MouseMove { x: gx, y: gy });
                                p.handle_event(PlayerEvent::MouseDown {
                                    x: gx,
                                    y: gy,
                                    button: MouseButton::Left,
                                    index: None,
                                });
                            }
                            TouchKind::Move => {
                                p.handle_event(PlayerEvent::MouseMove { x: gx, y: gy });
                            }
                            TouchKind::Up => {
                                p.handle_event(PlayerEvent::MouseUp {
                                    x: gx,
                                    y: gy,
                                    button: MouseButton::Left,
                                });
                            }
                        }
                    }
                }
                EngineMsg::TextDelta {
                    backspaces,
                    inserted,
                } => {
                    if egui_ime_active {
                        for _ in 0..backspaces {
                            pending_egui_events.push(egui::Event::Key {
                                key: egui::Key::Backspace,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            });
                        }
                        if !inserted.is_empty() {
                            pending_egui_events.push(egui::Event::Text(inserted));
                        }
                        *ui_dirty = true;
                    } else if let Some(tab) = tabs.get(*active_idx)
                        && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                    {
                        for _ in 0..backspaces {
                            kb_draft.pop();
                            send_flash_backspace(&mut p);
                        }
                        for ch in inserted.chars() {
                            if ch == '\n' || ch == '\r' {
                                send_flash_enter(&mut p);
                                kb_draft.clear();
                                kb_open.store(false, Ordering::Release);
                                hide_ime(app, ime_reset_flag);
                            } else {
                                kb_draft.push(ch);
                                p.handle_event(PlayerEvent::TextInput { codepoint: ch });
                            }
                        }
                        *needs_game_render = true;
                        *ui_dirty = true;
                    }
                }
                EngineMsg::KeyBackspace => {
                    if egui_ime_active {
                        pending_egui_events.push(egui::Event::Key {
                            key: egui::Key::Backspace,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        });
                        *ui_dirty = true;
                    } else if let Some(tab) = tabs.get(*active_idx)
                        && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                    {
                        kb_draft.pop();
                        send_flash_backspace(&mut p);
                        *needs_game_render = true;
                        *ui_dirty = true;
                    }
                }
                EngineMsg::KeyEnter => {
                    if egui_ime_active {
                        pending_egui_events.push(egui::Event::Key {
                            key: egui::Key::Enter,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        });
                        *ui_dirty = true;
                    } else if let Some(tab) = tabs.get(*active_idx)
                        && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                    {
                        send_flash_enter(&mut p);
                        kb_draft.clear();
                        kb_open.store(false, Ordering::Release);
                        hide_ime(app, ime_reset_flag);
                        *needs_game_render = true;
                        *ui_dirty = true;
                    }
                }
                EngineMsg::KeyChar(ch) => {
                    if egui_ime_active {
                        pending_egui_events.push(egui::Event::Text(ch.to_string()));
                        *ui_dirty = true;
                    } else if let Some(tab) = tabs.get(*active_idx)
                        && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                    {
                        kb_draft.push(ch);
                        p.handle_event(PlayerEvent::TextInput { codepoint: ch });
                        *needs_game_render = true;
                        *ui_dirty = true;
                    }
                }
                EngineMsg::Task(tab_id, task) => {
                    if let Some(tab) = tabs.iter().find(|t| t.id == tab_id) {
                        let outer = ruffle_render_wgpu::segments::take_pending();
                        let saved = std::mem::take(
                            &mut *tab
                                .segments
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner()),
                        );
                        ruffle_render_wgpu::segments::restore(saved);
                        task.run();
                        *tab.segments
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                            ruffle_render_wgpu::segments::take_pending();
                        if !outer.is_empty() {
                            ruffle_render_wgpu::segments::restore(outer);
                        }
                    }
                }
                EngineMsg::FlashCall(tab_id, req) => {
                    if let Some(tab) = tabs.iter().find(|t| t.id == tab_id) {
                        let mut p = TabPlayerGuard::try_lock(&tab.player, &tab.segments);
                        answer_flash_call(p.as_deref_mut(), req);
                    } else {
                        answer_flash_call(None, req);
                    }
                }
            }
            false
        }

        // Outer loop: ticks SWF ONLY when a game frame is actually due (`run_loop_paced` pattern).
        loop {
            let now = Instant::now();
            let mut next_frame = now + Duration::from_millis(42);

            if !paused {
                let dt = FloatDuration::from_std(now.duration_since(last_tick));
                last_tick = now;

                if now.duration_since(last_shape_sweep) >= SHAPE_SWEEP_EVERY {
                    last_shape_sweep = now;
                    let _ = ruffle_core::evict_unused_shapes(300);
                }

                let mut should_present = false;
                for (idx, tab) in tabs.iter_mut().enumerate() {
                    if idx == active_idx {
                        if let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments) {
                            let before = p.frames_run();
                            p.tick(dt);
                            let ran_frame = p.frames_run() != before;

                            if kb_open.load(Ordering::Acquire)
                                && let Some((live_text, is_pw)) = inspect_focused_flash_field(&mut p)
                            {
                                if kb_draft != live_text {
                                    kb_draft = live_text;
                                    ui_dirty = true;
                                }
                                kb_masked = kb_manual_mask.unwrap_or(is_pw);
                            } else if !kb_open.load(Ordering::Acquire) {
                                kb_manual_mask = None;
                            }

                            if (ran_frame && p.needs_render()) || needs_game_render {
                                let frame_dur =
                                    Duration::from_secs_f64(1.0 / p.frame_rate().max(1.0));
                                let behind = p.is_playing()
                                    && !needs_game_render
                                    && last_picture.elapsed() < MAX_DRAW_GAP
                                    && p.time_til_next_frame() * 2 < frame_dur;
                                if !behind && surface.is_some() {
                                    p.render();
                                    last_picture = Instant::now();
                                    needs_game_render = false;
                                    should_present = true;
                                }
                            }
                            next_frame = Instant::now() + p.time_til_next_frame();
                        }
                    } else {
                        let busy = !tab.bot.is_game_loaded()
                            || tab.bot.runner.is_running()
                            || tab.bot.auto_attack.is_running()
                            || tab.bot.maid.is_running();
                        let target_ms = if busy { 100.0 } else { 250.0 };
                        tab.bg_accum_ms += dt.as_millis();
                        if tab.bg_accum_ms >= target_ms {
                            let step_ms = tab.bg_accum_ms.min(250.0);
                            tab.bg_accum_ms = 0.0;
                            if let Some(mut p) =
                                TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                            {
                                if p.mouse_in_stage() {
                                    p.set_mouse_in_stage(false);
                                    p.handle_event(PlayerEvent::MouseLeave);
                                }
                                p.tick(FloatDuration::from_millis(step_ms));
                            }
                        }
                    }
                }

                if should_present
                    && let (
                        Some(desc),
                        Some(gpu_blit),
                        Some(pipe),
                        Some(surf),
                        Some(cfg),
                        Some(renderer),
                    ) = (
                        descriptors.as_ref(),
                        blit.as_ref(),
                        blit_pipeline.as_ref(),
                        surface.as_ref(),
                        surface_config.as_ref(),
                        egui_renderer.as_mut(),
                    )
                {
                    ui_dirty = false;
                    last_draw = Instant::now();
                    draw_screen(
                        &app,
                        &data_dir,
                        &swf_url,
                        &engine_tx,
                        &kb_open,
                        &ime_reset_flag,
                        &egui_ctx,
                        scale_factor,
                        started,
                        desc,
                        gpu_blit,
                        pipe,
                        surf,
                        cfg,
                        renderer,
                        &mut tabs,
                        &mut active_idx,
                        &mut next_tab_id,
                        &mut egui_ime_active,
                        &mut kb_draft,
                        &mut kb_masked,
                        &mut kb_manual_mask,
                        &mut pending_egui_events,
                        &mut ui_due,
                        &mut needs_game_render,
                    );
                }
            } else {
                last_tick = now;
                next_frame = now + Duration::from_millis(250);
            }

            // Inner loop: drains async tasks, FlashCalls, and touch/key events UNTIL `next_frame`
            // without running premature SWF ticks or stealing a due game frame's vsync slot!
            loop {
                let now = Instant::now();
                if !paused && now >= next_frame {
                    break;
                }
                let ui_wake = if !paused && (ui_dirty || ui_due.is_some()) {
                    let earliest = last_draw + UI_FRAME;
                    Some(ui_due.map_or(earliest, |d| d.max(earliest)))
                } else {
                    None
                };
                let wake = ui_wake.map_or(next_frame, |u| u.min(next_frame));
                let wait = wake.saturating_duration_since(now);

                let prev_tabs_len = tabs.len();
                let prev_active = active_idx;

                match engine_rx.recv_timeout(wait) {
                    Ok(first) => {
                        if handle_msg(
                            first,
                            &app,
                            &data_dir,
                            &swf_url,
                            &engine_tx,
                            &kb_open,
                            &ime_reset_flag,
                            &egui_ctx,
                            scale_factor,
                            egui_ime_active,
                            &mut descriptors,
                            &mut blit,
                            &mut blit_pipeline,
                            &mut surface,
                            &mut surface_config,
                            &mut egui_renderer,
                            &mut held_window,
                            &mut tabs,
                            &mut active_idx,
                            &mut next_tab_id,
                            &mut paused,
                            &mut needs_game_render,
                            &mut ui_dirty,
                            &mut pending_egui_events,
                            &mut kb_draft,
                            &mut touch_on_ui,
                        ) {
                            return;
                        }
                        while let Ok(msg) = engine_rx.try_recv() {
                            if handle_msg(
                                msg,
                                &app,
                                &data_dir,
                                &swf_url,
                                &engine_tx,
                                &kb_open,
                                &ime_reset_flag,
                                &egui_ctx,
                                scale_factor,
                                egui_ime_active,
                                &mut descriptors,
                                &mut blit,
                                &mut blit_pipeline,
                                &mut surface,
                                &mut surface_config,
                                &mut egui_renderer,
                                &mut held_window,
                                &mut tabs,
                                &mut active_idx,
                                &mut next_tab_id,
                                &mut paused,
                                &mut needs_game_render,
                                &mut ui_dirty,
                                &mut pending_egui_events,
                                &mut kb_draft,
                                &mut touch_on_ui,
                            ) {
                                return;
                            }
                        }
                        if tabs.len() != prev_tabs_len || active_idx != prev_active || needs_game_render {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }

                let now = Instant::now();
                if !paused && now >= next_frame {
                    break;
                }

                // Standalone UI-only draw between game frames (only when >=10ms away from next
                // SWF frame so it never steals the game frame's vsync slot!).
                let ui_ready = (ui_dirty || ui_due.is_some_and(|d| now >= d))
                    && last_draw.elapsed() >= UI_FRAME
                    && next_frame.saturating_duration_since(now) >= Duration::from_millis(10);
                if !paused
                    && ui_ready
                    && let (
                        Some(desc),
                        Some(gpu_blit),
                        Some(pipe),
                        Some(surf),
                        Some(cfg),
                        Some(renderer),
                    ) = (
                        descriptors.as_ref(),
                        blit.as_ref(),
                        blit_pipeline.as_ref(),
                        surface.as_ref(),
                        surface_config.as_ref(),
                        egui_renderer.as_mut(),
                    )
                {
                    ui_dirty = false;
                    last_draw = now;
                    draw_screen(
                        &app,
                        &data_dir,
                        &swf_url,
                        &engine_tx,
                        &kb_open,
                        &ime_reset_flag,
                        &egui_ctx,
                        scale_factor,
                        started,
                        desc,
                        gpu_blit,
                        pipe,
                        surf,
                        cfg,
                        renderer,
                        &mut tabs,
                        &mut active_idx,
                        &mut next_tab_id,
                        &mut egui_ime_active,
                        &mut kb_draft,
                        &mut kb_masked,
                        &mut kb_manual_mask,
                        &mut pending_egui_events,
                        &mut ui_due,
                        &mut needs_game_render,
                    );
                }
            }
        }
    }

    pub fn run(app: AndroidApp) {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("GrimliteRi")
                .with_filter(
                    android_logger::FilterBuilder::new()
                        .parse("warn,grimlite_android=info,grimlite_bot=info,ruffle_aqw_perf=info,ruffle_core::avm2=off,ruffle_core::stub=off,avm_trace=off,symphonia_bundle_mp3=error")
                        .build(),
                ),
        );
        let _ = rustls::crypto::ring::default_provider().install_default();
        init_android_tls(&app);

        for var in DEFAULT_ON_SWITCHES {
            if std::env::var_os(var).is_none() {
                unsafe { std::env::set_var(var, "1") };
            }
        }

        let worker_threads = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(4);
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_threads)
            .thread_name(|i| format!("grimlite-rayon-{i}"))
            .build_global();
        log::info!("Grimlite Ri Android starting ({worker_threads} Rayon worker threads)");

        let data_dir = app
            .internal_data_path()
            .unwrap_or_else(|| PathBuf::from("/data/local/tmp/grimlite-ri"));
        let swf_path = ensure_host_swf(&data_dir);
        let swf_url = Url::from_file_path(&swf_path).expect("host swf file url");

        let (engine_tx, engine_rx) = mpsc::channel::<EngineMsg>();
        let kb_open = Arc::new(AtomicBool::new(false));
        let ime_reset_flag = Arc::new(AtomicBool::new(false));

        let engine_app = app.clone();
        let engine_tx_clone = engine_tx.clone();
        let engine_kb_open = kb_open.clone();
        let engine_ime_reset = ime_reset_flag.clone();
        let engine_handle = std::thread::Builder::new()
            .name("Grimlite Engine".into())
            .stack_size(8 << 20)
            .spawn(move || {
                engine_loop(
                    engine_app,
                    data_dir,
                    swf_url,
                    engine_tx_clone,
                    engine_rx,
                    engine_kb_open,
                    engine_ime_reset,
                )
            })
            .expect("spawn Grimlite Engine thread");

        let mut should_exit = false;
        let mut last_ime_chars: Vec<char> = Vec::new();

        while !should_exit {
            app.poll_events(None, |event| match event {
                PollEvent::Main(MainEvent::Destroy) => {
                    let _ = engine_tx.send(EngineMsg::Destroy);
                    should_exit = true;
                }
                PollEvent::Main(MainEvent::Pause | MainEvent::Stop) => {
                    let _ = engine_tx.send(EngineMsg::Paused(true));
                }
                PollEvent::Main(MainEvent::Resume { .. } | MainEvent::Start) => {
                    let _ = engine_tx.send(EngineMsg::Paused(false));
                }
                PollEvent::Main(MainEvent::InitWindow { .. }) => {
                    let _ = engine_tx.send(EngineMsg::InitWindow);
                }
                PollEvent::Main(
                    MainEvent::WindowResized { .. } | MainEvent::ContentRectChanged { .. },
                ) => {
                    let _ = engine_tx.send(EngineMsg::ResizeWindow);
                }
                PollEvent::Main(MainEvent::TerminateWindow { .. }) => {
                    let (done_tx, done_rx) = mpsc::sync_channel(1);
                    if engine_tx.send(EngineMsg::DetachWindow(done_tx)).is_ok() {
                        let _ = done_rx.recv_timeout(Duration::from_millis(250));
                    }
                }
                PollEvent::Main(MainEvent::InputAvailable) => {
                    if ime_reset_flag.swap(false, Ordering::AcqRel) {
                        last_ime_chars.clear();
                    }
                    if let Ok(mut iter) = app.input_events_iter() {
                        while iter.next(|input| match input {
                            InputEvent::MotionEvent(motion) => {
                                if let Some(pointer) = motion.pointers().next() {
                                    let kind = match motion.action() {
                                        MotionAction::Down | MotionAction::PointerDown => {
                                            Some(TouchKind::Down)
                                        }
                                        MotionAction::Move => Some(TouchKind::Move),
                                        MotionAction::Up
                                        | MotionAction::PointerUp
                                        | MotionAction::Cancel => Some(TouchKind::Up),
                                        _ => None,
                                    };
                                    if let Some(kind) = kind {
                                        let _ = engine_tx.send(EngineMsg::Touch {
                                            kind,
                                            px: pointer.x(),
                                            py: pointer.y(),
                                        });
                                    }
                                }
                                InputStatus::Handled
                            }
                            InputEvent::TextEvent(state) => {
                                if ime_reset_flag.swap(false, Ordering::AcqRel) {
                                    last_ime_chars.clear();
                                }
                                let new_chars: Vec<char> = state.text.chars().collect();
                                let common = last_ime_chars
                                    .iter()
                                    .zip(&new_chars)
                                    .take_while(|(a, b)| a == b)
                                    .count();
                                let backspaces = last_ime_chars.len().saturating_sub(common);
                                let inserted: String = new_chars[common..].iter().collect();
                                last_ime_chars = new_chars;
                                if backspaces > 0 || !inserted.is_empty() {
                                    let _ = engine_tx.send(EngineMsg::TextDelta {
                                        backspaces,
                                        inserted,
                                    });
                                }
                                InputStatus::Handled
                            }
                            InputEvent::KeyEvent(ke) => match ke.key_code() {
                                Keycode::Back
                                | Keycode::VolumeUp
                                | Keycode::VolumeDown
                                | Keycode::VolumeMute => InputStatus::Unhandled,
                                Keycode::Del => {
                                    if ke.action() == KeyAction::Down {
                                        last_ime_chars.pop();
                                        let _ = engine_tx.send(EngineMsg::KeyBackspace);
                                    }
                                    InputStatus::Handled
                                }
                                Keycode::Enter | Keycode::NumpadEnter => {
                                    if ke.action() == KeyAction::Down {
                                        last_ime_chars.clear();
                                        let _ = engine_tx.send(EngineMsg::KeyEnter);
                                    }
                                    InputStatus::Handled
                                }
                                _ => {
                                    if ke.action() == KeyAction::Down
                                        && let Ok(cmap) =
                                            app.device_key_character_map(ke.device_id())
                                        && let Ok(KeyMapChar::Unicode(ch)) =
                                            cmap.get(ke.key_code(), ke.meta_state())
                                        && !ch.is_control()
                                    {
                                        let _ = engine_tx.send(EngineMsg::KeyChar(ch));
                                    }
                                    InputStatus::Handled
                                }
                            },
                            _ => InputStatus::Unhandled,
                        }) {}
                    }
                }
                _ => {}
            });
        }

        let _ = engine_handle.join();
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: android_activity::AndroidApp) {
    runtime::run(app);
}
