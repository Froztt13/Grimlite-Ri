//! Grimlite Ri Android runtime (`cdylib`, `android-activity` GameActivity + wgpu + egui).
//!
//! Layout on mobile:
//! - Left edge: vertical multi-account tab rail (`grimlite_egui::show_tab_rail_left`)
//! - Top edge (right of rail): compact Grimlite navbar + in-canvas tool windows (`GrimliteUi`)
//! - Remaining area: active account's AQW Ruffle viewport
//! - Battery-friendly: pauses rendering when Android sends `Pause`/`Stop` and only renders on
//!   active game frames or touch interaction (never unbounded `request_repaint`).

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
    use grimlite_frontend::ruffle_host::{FlashCallRequest, RuffleFlashHost, answer_flash_call};
    use ruffle_core::backend::navigator::{OwnedFuture, SocketMode};
    use ruffle_core::backend::ui::{
        DialogResultFuture, FileFilter, FontDefinition, FullscreenError, LanguageIdentifier,
        MouseCursor, MultiDialogResultFuture, NullUiBackend, UiBackend,
    };
    use ruffle_core::events::{
        KeyDescriptor, LogicalKey, MouseButton, NamedKey, PhysicalKey, PlayerEvent,
        TextControlCode,
    };
    use ruffle_core::font::FontQuery;
    use ruffle_core::{FloatDuration, Player, PlayerBuilder};
    use ruffle_frontend_utils::backends::navigator::{
        ExternalNavigatorBackend, FutureSpawner, NavigatorInterface,
    };
    use ruffle_frontend_utils::content::{ContentDescriptor, PlayingContent};
    use ruffle_render::backend::ViewportDimensions;
    use ruffle_render_wgpu::backend::{
        WgpuRenderBackend, create_wgpu_instance, request_adapter_and_device,
    };
    use ruffle_render_wgpu::descriptors::Descriptors;
    use ruffle_render_wgpu::target::TextureTarget;
    use std::collections::HashSet;
    use std::fs::File;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};
    use url::Url;

    struct AndroidTab {
        id: u64,
        bot: Arc<Bot>,
        player: Arc<Mutex<Player>>,
        ui: GrimliteUi,
        game_tex_id: egui::TextureId,
    }

    struct LocalExecutor {
        tx: mpsc::Sender<async_task::Runnable>,
    }

    impl<E: std::error::Error + 'static> FutureSpawner<E> for LocalExecutor {
        fn spawn(&self, future: OwnedFuture<(), E>) {
            let future = async {
                if let Err(e) = future.await {
                    log::error!("Android navigator future error: {e}");
                }
            };
            let tx = self.tx.clone();
            let schedule = move |task| {
                let _ = tx.send(task);
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

    fn set_soft_keyboard(app: &AndroidApp, show: bool) {
        if show {
            app.set_ime_editor_info(
                InputType::TYPE_NULL,
                TextInputAction::None,
                ImeOptions::IMG_FLAG_NO_EXTRACT_UI | ImeOptions::IME_FLAG_NO_FULLSCREEN,
            );
            app.set_text_input_state(TextInputState::default());
            app.show_soft_input(false);
        } else {
            app.hide_soft_input(false);
        }
    }

    struct AndroidUiBackend {
        inner: NullUiBackend,
        app: AndroidApp,
        kb_open: Arc<AtomicBool>,
        kb_opened_event: Arc<AtomicBool>,
    }

    impl AndroidUiBackend {
        fn new(
            app: AndroidApp,
            kb_open: Arc<AtomicBool>,
            kb_opened_event: Arc<AtomicBool>,
        ) -> Self {
            Self {
                inner: NullUiBackend::new(),
                app,
                kb_open,
                kb_opened_event,
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
            self.kb_open.store(true, Ordering::Release);
            self.kb_opened_event.store(true, Ordering::Release);
            set_soft_keyboard(&self.app, true);
        }
        fn close_virtual_keyboard(&self) {
            self.kb_open.store(false, Ordering::Release);
            set_soft_keyboard(&self.app, false);
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

    #[allow(clippy::too_many_arguments)]
    fn spawn_tab(
        app: &AndroidApp,
        kb_open: Arc<AtomicBool>,
        kb_opened_event: Arc<AtomicBool>,
        tab_id: u64,
        descriptors: Arc<Descriptors>,
        egui_renderer: &mut egui_wgpu::Renderer,
        swf_url: &Url,
        task_tx: mpsc::Sender<async_task::Runnable>,
        flash_tx: mpsc::Sender<(u64, FlashCallRequest)>,
        width: u32,
        height: u32,
        scale: f64,
    ) -> AndroidTab {
        let bot = Bot::new();
        let game_w = width
            .saturating_sub((TAB_RAIL_WIDTH * scale as f32) as u32)
            .max(1);
        let game_h = height
            .saturating_sub((NAV_BAR_HEIGHT * scale as f32) as u32)
            .max(1);
        let target =
            TextureTarget::new(&descriptors.device, (game_w, game_h)).expect("Android TextureTarget");
        let renderer =
            WgpuRenderBackend::new(descriptors.clone(), target).expect("Android WgpuRenderBackend");

        let content = PlayingContent::DirectFile(ContentDescriptor {
            url: swf_url.clone(),
            root_content_path: None,
        });
        let raw_nav = ExternalNavigatorBackend::new(
            swf_url.clone(),
            None,
            None,
            LocalExecutor { tx: task_tx },
            None,
            false,
            HashSet::new(),
            SocketMode::Allow,
            Rc::new(content),
            NoDialogs,
        );
        let navigator = CaptureNavigator::new(raw_nav, Some(bot.capture.clone()));

        let player = PlayerBuilder::new()
            .with_renderer(renderer)
            .with_navigator(navigator)
            .with_ui(AndroidUiBackend::new(app.clone(), kb_open, kb_opened_event))
            .with_external_interface(Box::new(GrimliteExternalInterface::new(bot.clone())))
            .with_player_version(Some(9))
            .with_letterbox(ruffle_core::config::Letterbox::On)
            .with_autoplay(true)
            .build();

        let tx = flash_tx.clone();
        let host = Arc::new(RuffleFlashHost::new(
            std::thread::current().id(),
            move |req| {
                tx.send((tab_id, req))
                    .map_err(|_| "android loop closed".to_string())
            },
        ));
        bot.set_host(host);

        let game_tex_id = {
            let mut p = player.lock().expect("player lock");
            p.set_viewport_dimensions(ViewportDimensions {
                width: game_w,
                height: game_h,
                scale_factor: scale,
            });
            p.fetch_root_movie(swf_url.to_string(), vec![], Box::new(|_| {}));
            // Register texture AFTER `set_viewport_dimensions` because `TextureTarget::resize`
            // replaces the underlying `wgpu::Texture`.
            let wgpu_backend = (<dyn std::any::Any>::downcast_ref::<
                WgpuRenderBackend<TextureTarget>,
            >(p.renderer()))
            .expect("WgpuRenderBackend<TextureTarget>");
            let game_view = wgpu_backend
                .target()
                .get_texture()
                .create_view(&wgpu::TextureViewDescriptor::default());
            egui_renderer.register_native_texture(
                &descriptors.device,
                &game_view,
                wgpu::FilterMode::Linear,
            )
        };

        let ui = GrimliteUi::new(bot.clone(), None, None);
        AndroidTab {
            id: tab_id,
            bot,
            player,
            ui,
            game_tex_id,
        }
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

    pub fn run(app: AndroidApp) {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("GrimliteRi"),
        );
        let _ = rustls::crypto::ring::default_provider().install_default();
        // Required for AQW Game.swf (same as desktop `app/src/main.rs`).
        ruffle_core::set_show_all_apis(true);
        ruffle_core::set_forgiving_removal(true);
        // Initialize Android platform certificate verifier for HTTPS (AQW gameversion / Game.swf).
        init_android_tls(&app);
        // Disable thread-local segment queue so multi-tab players share the main thread safely.
        unsafe { std::env::set_var("RUFFLE_SERIAL_ENCODING", "1") };

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let _rt_guard = rt.enter();

        let data_dir = app
            .internal_data_path()
            .unwrap_or_else(|| PathBuf::from("/data/local/tmp/grimlite-ri"));
        let swf_path = ensure_host_swf(&data_dir);
        let swf_url = Url::from_file_path(&swf_path).expect("host swf file url");

        let (task_tx, task_rx) = mpsc::channel::<async_task::Runnable>();
        let (flash_tx, flash_rx) = mpsc::channel::<(u64, FlashCallRequest)>();

        let egui_ctx = Context::default();
        grimlite_egui::theme::apply(&egui_ctx);

        let mut descriptors: Option<Arc<Descriptors>> = None;
        let mut surface: Option<wgpu::Surface<'static>> = None;
        let mut surface_config: Option<wgpu::SurfaceConfiguration> = None;
        let mut egui_renderer: Option<egui_wgpu::Renderer> = None;
        let mut tabs: Vec<AndroidTab> = Vec::new();
        let mut active_idx: usize = 0;
        let mut next_tab_id: u64 = 1;
        let mut paused = false;
        let mut was_egui_kb = false;
        let kb_open = Arc::new(AtomicBool::new(false));
        let kb_opened_event = Arc::new(AtomicBool::new(false));
        let mut kb_draft = String::new();
        let mut kb_masked = false;
        let mut needs_game_render = true;
        let mut pending_egui_events: Vec<egui::Event> = Vec::new();
        let mut last_tick = Instant::now();
        let scale_factor: f32 = 2.0;

        loop {
            let poll_timeout = if paused {
                Some(Duration::from_millis(250))
            } else {
                Some(Duration::from_millis(16))
            };

            let mut should_exit = false;
            app.poll_events(poll_timeout, |event| match event {
                PollEvent::Main(MainEvent::Destroy) => should_exit = true,
                PollEvent::Main(MainEvent::Pause | MainEvent::Stop) => paused = true,
                PollEvent::Main(MainEvent::Resume { .. } | MainEvent::Start) => paused = false,
                PollEvent::Main(MainEvent::InitWindow { .. }) => {
                    if let Some(native_win) = app.native_window() {
                        let w = native_win.width().max(1) as u32;
                        let h = native_win.height().max(1) as u32;
                        let instance = create_wgpu_instance(
                            wgpu::Backends::VULKAN | wgpu::Backends::GL,
                            wgpu::BackendOptions::default(),
                            None,
                        );
                        let raw_window_handle = wgpu::rwh::RawWindowHandle::AndroidNdk(
                            wgpu::rwh::AndroidNdkWindowHandle::new(native_win.ptr().cast()),
                        );
                        let raw_display_handle = wgpu::rwh::RawDisplayHandle::Android(
                            wgpu::rwh::AndroidDisplayHandle::new(),
                        );
                        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
                            raw_display_handle: Some(raw_display_handle),
                            raw_window_handle,
                        };
                        if let Ok(surf) = unsafe { instance.create_surface_unsafe(target) } {
                            if let Ok((adapter, device, queue)) =
                                futures::executor::block_on(request_adapter_and_device(
                                    wgpu::Backends::VULKAN | wgpu::Backends::GL,
                                    &instance,
                                    Some(&surf),
                                    wgpu::PowerPreference::HighPerformance,
                                ))
                            {
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
                                let desc =
                                    Arc::new(Descriptors::new(instance, adapter, device, queue));
                                let mut renderer = egui_wgpu::Renderer::new(
                                    &desc.device,
                                    format,
                                    egui_wgpu::RendererOptions::default(),
                                );
                                if tabs.is_empty() {
                                    let tab = spawn_tab(
                                        &app,
                                        kb_open.clone(),
                                        kb_opened_event.clone(),
                                        next_tab_id,
                                        desc.clone(),
                                        &mut renderer,
                                        &swf_url,
                                        task_tx.clone(),
                                        flash_tx.clone(),
                                        w,
                                        h,
                                        scale_factor as f64,
                                    );
                                    next_tab_id += 1;
                                    tabs.push(tab);
                                    active_idx = 0;
                                }
                                descriptors = Some(desc);
                                surface = Some(surf);
                                surface_config = Some(cfg);
                                egui_renderer = Some(renderer);
                            }
                        }
                    }
                }
                PollEvent::Main(MainEvent::TerminateWindow { .. }) => {
                    surface = None;
                }
                _ => {}
            });

            if should_exit {
                break;
            }

            // Poll async Ruffle navigator tasks on this engine thread
            while let Ok(task) = task_rx.try_recv() {
                task.run();
            }

            // Drain touch, key, and soft-keyboard text events -> egui + active Ruffle player
            if let Ok(mut iter) = app.input_events_iter() {
                while iter.next(|event| match event {
                    InputEvent::MotionEvent(motion) => {
                        if let Some(pointer) = motion.pointers().next() {
                            let px = pointer.x();
                            let py = pointer.y();
                            let logical = Pos2::new(px / scale_factor, py / scale_factor);
                            let top_chrome_h = if kb_open.load(Ordering::Acquire) && !was_egui_kb {
                                NAV_BAR_HEIGHT + 34.0
                            } else {
                                NAV_BAR_HEIGHT
                            };
                            let over_chrome = px < TAB_RAIL_WIDTH * scale_factor
                                || py < top_chrome_h * scale_factor
                                || egui_ctx
                                    .layer_id_at(logical)
                                    .is_some_and(|l| l.order != egui::Order::Background);

                            match motion.action() {
                                MotionAction::Down => {
                                    pending_egui_events.push(egui::Event::PointerMoved(logical));
                                    pending_egui_events.push(egui::Event::PointerButton {
                                        pos: logical,
                                        button: egui::PointerButton::Primary,
                                        pressed: true,
                                        modifiers: Default::default(),
                                    });
                                    if !over_chrome
                                        && let Some(tab) = tabs.get(active_idx)
                                        && let Ok(mut p) = tab.player.try_lock()
                                    {
                                        let gx = (px - TAB_RAIL_WIDTH * scale_factor).max(0.0) as f64;
                                        let gy = (py - NAV_BAR_HEIGHT * scale_factor).max(0.0) as f64;
                                        p.handle_event(PlayerEvent::MouseMove { x: gx, y: gy });
                                        p.handle_event(PlayerEvent::MouseDown {
                                            x: gx,
                                            y: gy,
                                            button: MouseButton::Left,
                                            index: None,
                                        });
                                    }
                                }
                                MotionAction::Up | MotionAction::Cancel => {
                                    pending_egui_events.push(egui::Event::PointerButton {
                                        pos: logical,
                                        button: egui::PointerButton::Primary,
                                        pressed: false,
                                        modifiers: Default::default(),
                                    });
                                    if !over_chrome
                                        && let Some(tab) = tabs.get(active_idx)
                                        && let Ok(mut p) = tab.player.try_lock()
                                    {
                                        let gx = (px - TAB_RAIL_WIDTH * scale_factor).max(0.0) as f64;
                                        let gy = (py - NAV_BAR_HEIGHT * scale_factor).max(0.0) as f64;
                                        p.handle_event(PlayerEvent::MouseUp {
                                            x: gx,
                                            y: gy,
                                            button: MouseButton::Left,
                                        });
                                    }
                                }
                                MotionAction::Move => {
                                    pending_egui_events.push(egui::Event::PointerMoved(logical));
                                    if !over_chrome
                                        && let Some(tab) = tabs.get(active_idx)
                                        && let Ok(mut p) = tab.player.try_lock()
                                    {
                                        let gx = (px - TAB_RAIL_WIDTH * scale_factor).max(0.0) as f64;
                                        let gy = (py - NAV_BAR_HEIGHT * scale_factor).max(0.0) as f64;
                                        p.handle_event(PlayerEvent::MouseMove { x: gx, y: gy });
                                    }
                                }
                                _ => {}
                            }
                        }
                        InputStatus::Handled
                    }
                    InputEvent::TextEvent(_) => {
                        let text = app.text_input_state().text;
                        app.set_text_input_state(TextInputState::default());
                        if !text.is_empty() {
                            if egui_ctx.egui_wants_keyboard_input() {
                                pending_egui_events.push(egui::Event::Text(text));
                            } else if let Some(tab) = tabs.get(active_idx)
                                && let Ok(mut p) = tab.player.try_lock()
                            {
                                kb_draft.push_str(&text);
                                for ch in text.chars() {
                                    p.handle_event(PlayerEvent::TextInput { codepoint: ch });
                                }
                                needs_game_render = true;
                            }
                        }
                        InputStatus::Handled
                    }
                    InputEvent::KeyEvent(ke) => {
                        if ke.action() == KeyAction::Down {
                            let wants_egui = egui_ctx.egui_wants_keyboard_input();
                            match ke.key_code() {
                                Keycode::Del => {
                                    if wants_egui {
                                        pending_egui_events.push(egui::Event::Key {
                                            key: egui::Key::Backspace,
                                            physical_key: None,
                                            pressed: true,
                                            repeat: false,
                                            modifiers: Default::default(),
                                        });
                                    } else if let Some(tab) = tabs.get(active_idx)
                                        && let Ok(mut p) = tab.player.try_lock()
                                    {
                                        kb_draft.pop();
                                        p.handle_event(PlayerEvent::TextControl {
                                            code: TextControlCode::Backspace,
                                        });
                                        p.handle_event(PlayerEvent::KeyDown {
                                            key: KeyDescriptor {
                                                physical_key: PhysicalKey::Backspace,
                                                logical_key: LogicalKey::Named(NamedKey::Backspace),
                                                key_location: ruffle_core::events::KeyLocation::Standard,
                                            },
                                        });
                                        needs_game_render = true;
                                    }
                                }
                                Keycode::Enter | Keycode::NumpadEnter => {
                                    if wants_egui {
                                        pending_egui_events.push(egui::Event::Key {
                                            key: egui::Key::Enter,
                                            physical_key: None,
                                            pressed: true,
                                            repeat: false,
                                            modifiers: Default::default(),
                                        });
                                    } else if let Some(tab) = tabs.get(active_idx)
                                        && let Ok(mut p) = tab.player.try_lock()
                                    {
                                        p.handle_event(PlayerEvent::KeyDown {
                                            key: KeyDescriptor {
                                                physical_key: PhysicalKey::Enter,
                                                logical_key: LogicalKey::Named(NamedKey::Enter),
                                                key_location: ruffle_core::events::KeyLocation::Standard,
                                            },
                                        });
                                        kb_draft.clear();
                                        kb_open.store(false, Ordering::Release);
                                        set_soft_keyboard(&app, false);
                                        needs_game_render = true;
                                    }
                                }
                                _ => {
                                    if let Ok(cmap) = app.device_key_character_map(ke.device_id())
                                        && let Ok(KeyMapChar::Unicode(ch)) =
                                            cmap.get(ke.key_code(), ke.meta_state())
                                    {
                                        if wants_egui {
                                            pending_egui_events
                                                .push(egui::Event::Text(ch.to_string()));
                                        } else if let Some(tab) = tabs.get(active_idx)
                                            && let Ok(mut p) = tab.player.try_lock()
                                        {
                                            kb_draft.push(ch);
                                            p.handle_event(PlayerEvent::TextInput { codepoint: ch });
                                            needs_game_render = true;
                                        }
                                    }
                                }
                            }
                        }
                        InputStatus::Handled
                    }
                    _ => InputStatus::Unhandled,
                }) {}
            }

            // Drain Flash calls for all tabs (active + background)
            while let Ok((tab_id, req)) = flash_rx.try_recv() {
                if let Some(tab) = tabs.iter().find(|t| t.id == tab_id) {
                    if let Ok(mut p) = tab.player.try_lock() {
                        answer_flash_call(Some(&mut p), req);
                    }
                } else {
                    answer_flash_call(None, req);
                }
            }

            // Reset preview draft whenever Flash opens a fresh text field
            if kb_opened_event.swap(false, Ordering::AcqRel) {
                kb_draft.clear();
            }

            // Tick all tabs; only re-render active tab's Ruffle GPU target when a new SWF frame ran
            let now = Instant::now();
            let dt = FloatDuration::from_std(now.duration_since(last_tick));
            last_tick = now;
            if !paused {
                for (idx, tab) in tabs.iter().enumerate() {
                    if let Ok(mut p) = tab.player.try_lock() {
                        let before = p.frames_run();
                        p.tick(dt);
                        if idx == active_idx && (p.frames_run() != before || needs_game_render) {
                            p.render();
                            needs_game_render = false;
                        }
                    }
                }
            }

            // Render active UI + Left-Side Mobile Tab Rail when surface is ready
            if paused {
                continue;
            }
            let (Some(desc), Some(surf), Some(cfg), Some(renderer)) = (
                descriptors.as_ref(),
                surface.as_ref(),
                surface_config.as_ref(),
                egui_renderer.as_mut(),
            ) else {
                continue;
            };

            let raw_input = RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(
                        cfg.width as f32 / scale_factor,
                        cfg.height as f32 / scale_factor,
                    ),
                )),
                events: std::mem::take(&mut pending_egui_events),
                ..Default::default()
            };

            let mut tab_action = TabBarAction::None;
            let active_id = tabs.get(active_idx).map(|t| t.id).unwrap_or(0);
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
            let flash_kb_visible = kb_open.load(Ordering::Acquire) && !was_egui_kb;

            let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
                // Mobile layout: vertical multi-account tab rail on the LEFT edge
                tab_action =
                    grimlite_egui::show_tabs(ui, TabLayout::LeftRail, &descriptors_ui, active_id);
                if let Some(tab) = tabs.get_mut(active_idx) {
                    tab.ui.show(&egui_ctx, ui);

                    // Floating live text-input preview bar above the soft keyboard (Turu-style)
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
                                    let preview = if kb_masked {
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
                                                .selectable_label(kb_masked, "•••")
                                                .on_hover_text("Hide/show typed text")
                                                .clicked()
                                            {
                                                kb_masked = !kb_masked;
                                            }
                                        },
                                    );
                                });
                            });
                    }

                    let tex_id = tab.game_tex_id;
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| {
                            let avail = ui.available_size();
                            ui.image((tex_id, avail));
                        });
                }
            });

            if kb_action_enter {
                if let Some(tab) = tabs.get(active_idx)
                    && let Ok(mut p) = tab.player.try_lock()
                {
                    p.handle_event(PlayerEvent::KeyDown {
                        key: KeyDescriptor {
                            physical_key: PhysicalKey::Enter,
                            logical_key: LogicalKey::Named(NamedKey::Enter),
                            key_location: ruffle_core::events::KeyLocation::Standard,
                        },
                    });
                    needs_game_render = true;
                }
                kb_draft.clear();
                kb_open.store(false, Ordering::Release);
                set_soft_keyboard(&app, false);
            } else if kb_action_close {
                kb_draft.clear();
                kb_open.store(false, Ordering::Release);
                set_soft_keyboard(&app, false);
            }

            // Auto show/hide soft keyboard when an egui TextEdit gains/loses focus
            let egui_kb = egui_ctx.egui_wants_keyboard_input();
            if egui_kb != was_egui_kb {
                was_egui_kb = egui_kb;
                set_soft_keyboard(&app, egui_kb);
            }

            match tab_action {
                TabBarAction::NewTab => {
                    let tab = spawn_tab(
                        &app,
                        kb_open.clone(),
                        kb_opened_event.clone(),
                        next_tab_id,
                        desc.clone(),
                        renderer,
                        &swf_url,
                        task_tx.clone(),
                        flash_tx.clone(),
                        cfg.width,
                        cfg.height,
                        scale_factor as f64,
                    );
                    next_tab_id += 1;
                    tabs.push(tab);
                    active_idx = tabs.len() - 1;
                    needs_game_render = true;
                }
                TabBarAction::Switch(id) => {
                    if let Some(idx) = tabs.iter().position(|t| t.id == id) {
                        active_idx = idx;
                        needs_game_render = true;
                    }
                }
                TabBarAction::Close(id) if tabs.len() > 1 => {
                    if let Some(idx) = tabs.iter().position(|t| t.id == id) {
                        let removed = tabs.remove(idx);
                        renderer.free_texture(&removed.game_tex_id);
                        removed.bot.stop_bot();
                        removed.bot.auto_attack.stop(removed.bot.clone());
                        removed.bot.maid.stop(&removed.bot);
                        if active_idx >= tabs.len() {
                            active_idx = tabs.len() - 1;
                        }
                        needs_game_render = true;
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

            if let wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) = surf.get_current_texture()
            {
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = desc
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                let cmds = renderer.update_buffers(
                    &desc.device,
                    &desc.queue,
                    &mut encoder,
                    &clipped,
                    &screen,
                );
                {
                    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
                    });
                    renderer.render(&mut pass.forget_lifetime(), &clipped, &screen);
                }
                desc.queue
                    .submit(cmds.into_iter().chain(std::iter::once(encoder.finish())));
                desc.queue.present(frame);
            }

            for id in &full_output.textures_delta.free {
                renderer.free_texture(id);
            }
            full_output.textures_delta.clear();
        }
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: android_activity::AndroidApp) {
    runtime::run(app);
}
