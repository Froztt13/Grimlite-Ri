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
        GrimliteUi, MOBILE_RAIL_WIDTH, MobileGraphicsApi, MobilePerfState, MobileRenderPreset,
        TabBarAction, TabDescriptor,
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
    use ruffle_core::{FloatDuration, LoadBehavior, Player, PlayerBuilder, TextSelection};
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
    use std::sync::{Arc, Mutex, OnceLock, mpsc};
    use std::time::{Duration, Instant};
    use url::Url;

    const DOC_PICK_LAG_IMAGE: i32 = 1;
    const DOC_PICK_GBOT: i32 = 2;
    const DOC_SAVE_GBOT: i32 = 3;
    const DOC_PICK_MAID_JSON: i32 = 4;
    const DOC_SAVE_MAID_JSON: i32 = 5;

    static GLOBAL_ENGINE_TX: OnceLock<Mutex<Option<mpsc::Sender<EngineMsg>>>> = OnceLock::new();

    pub(super) fn send_global_engine_msg(msg: EngineMsg) {
        if let Some(slot) = GLOBAL_ENGINE_TX.get()
            && let Ok(guard) = slot.lock()
            && let Some(tx) = guard.as_ref()
        {
            let _ = tx.send(msg);
        }
    }

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

    /// Longest gap between drawn pictures when skipping behind frames (`turu`'s `MAX_DRAW_GAP`).
    const MAX_DRAW_GAP: Duration = Duration::from_millis(250);
    /// Minimum interval between standalone UI-only draws (`turu`'s `UI_FRAME` = 33ms / 30Hz).
    const UI_FRAME: Duration = Duration::from_millis(33);
    /// Unused vector shape eviction cadence (`turu`'s `SWEEP_EVERY` = 60s, 5 min unused).
    const SHAPE_SWEEP_EVERY: Duration = Duration::from_secs(60);
    /// Periodic GPU non-blocking poll & `/proc/meminfo` check cadence (`turu`'s `CHECK_EVERY` = 10s).
    const GPU_MAINT_EVERY: Duration = Duration::from_secs(10);
    /// Minimum cooldown between automatic low-memory GPU trims (`turu`'s `TRIM_EVERY` = 30s).
    const LOW_MEM_TRIM_EVERY: Duration = Duration::from_secs(30);
    /// Trigger automatic GPU/shape trim when available phone RAM drops below 15%.
    const LOW_MEM_PERCENT: u64 = 15;
    /// Interval between Android frame-pacing performance summaries in logcat (`GrimliteRi` tag).
    const PERF_LOG_EVERY: Duration = Duration::from_secs(5);

    struct FramePerf {
        window_start: Instant,
        fps_since: Instant,
        fps_renders: u32,
        last_frame_at: Option<Instant>,
        game_frames: u64,
        tick_count: u64,
        tick_us: u64,
        tick_max_us: u64,
        render_count: u64,
        render_us: u64,
        render_max_us: u64,
        skipped_behind: u64,
        draw_count: u64,
        draw_us: u64,
        draw_max_us: u64,
        ui_us: u64,
        acquire_us: u64,
        present_us: u64,
        hitches: u64,
        worst_gap_us: u64,
        coalesced_moves: u64,
    }

    impl FramePerf {
        fn new() -> Self {
            let now = Instant::now();
            Self {
                window_start: now,
                fps_since: now,
                fps_renders: 0,
                last_frame_at: None,
                game_frames: 0,
                tick_count: 0,
                tick_us: 0,
                tick_max_us: 0,
                render_count: 0,
                render_us: 0,
                render_max_us: 0,
                skipped_behind: 0,
                draw_count: 0,
                draw_us: 0,
                draw_max_us: 0,
                ui_us: 0,
                acquire_us: 0,
                present_us: 0,
                hitches: 0,
                worst_gap_us: 0,
                coalesced_moves: 0,
            }
        }

        fn record_frames(&mut self, frames: u64, took: Duration, frame_dur: Duration) {
            let us = took.as_micros() as u64;
            self.game_frames += frames;
            self.tick_count += 1;
            self.tick_us += us;
            self.tick_max_us = self.tick_max_us.max(us);

            let now = Instant::now();
            if let Some(prev) = self.last_frame_at.replace(now) {
                let gap = now.duration_since(prev);
                let gap_us = gap.as_micros() as u64;
                self.worst_gap_us = self.worst_gap_us.max(gap_us);
                if gap >= frame_dur * 3 / 2 {
                    self.hitches += 1;
                }
            }
        }

        fn record_render(&mut self, took: Duration, mobile_perf: &MobilePerfState) {
            let us = took.as_micros() as u64;
            self.render_count += 1;
            self.render_us += us;
            self.render_max_us = self.render_max_us.max(us);

            self.fps_renders += 1;
            let elapsed = self.fps_since.elapsed().as_secs_f32();
            if elapsed >= 1.0 {
                mobile_perf.set_rendered_fps(self.fps_renders as f32 / elapsed);
                self.fps_renders = 0;
                self.fps_since = Instant::now();
            }
        }

        fn record_draw(
            &mut self,
            total: Duration,
            ui: Duration,
            acquire: Duration,
            present: Duration,
        ) {
            let us = total.as_micros() as u64;
            self.draw_count += 1;
            self.draw_us += us;
            self.draw_max_us = self.draw_max_us.max(us);
            self.ui_us += ui.as_micros() as u64;
            self.acquire_us += acquire.as_micros() as u64;
            self.present_us += present.as_micros() as u64;
        }

        fn maybe_log(&mut self, now: Instant) {
            let elapsed = now.duration_since(self.window_start);
            if elapsed < PERF_LOG_EVERY {
                return;
            }
            let secs = elapsed.as_secs_f64().max(0.001);
            if self.game_frames > 0 || self.draw_count > 0 {
                let avg = |us: u64, count: u64| -> f64 {
                    if count > 0 {
                        us as f64 / 1000.0 / count as f64
                    } else {
                        0.0
                    }
                };
                log::info!(
                    "perf: game {:.1} fps, rendered {:.1} fps ({} skipped behind), {} hitches (worst {:.0}ms) | \
                     tick {:.1}/{:.1}ms, render {:.1}/{:.1}ms, draw {:.1}/{:.1}ms (ui {:.1}, acq {:.1}, pres {:.1}ms) | \
                     moves coalesced {}",
                    self.game_frames as f64 / secs,
                    self.render_count as f64 / secs,
                    self.skipped_behind,
                    self.hitches,
                    self.worst_gap_us as f64 / 1000.0,
                    avg(self.tick_us, self.tick_count),
                    self.tick_max_us as f64 / 1000.0,
                    avg(self.render_us, self.render_count),
                    self.render_max_us as f64 / 1000.0,
                    avg(self.draw_us, self.draw_count),
                    self.draw_max_us as f64 / 1000.0,
                    avg(self.ui_us, self.draw_count),
                    avg(self.acquire_us, self.draw_count),
                    avg(self.present_us, self.draw_count),
                    self.coalesced_moves,
                );
            }
            self.window_start = now;
            self.game_frames = 0;
            self.tick_count = 0;
            self.tick_us = 0;
            self.tick_max_us = 0;
            self.render_count = 0;
            self.render_us = 0;
            self.render_max_us = 0;
            self.skipped_behind = 0;
            self.draw_count = 0;
            self.draw_us = 0;
            self.draw_max_us = 0;
            self.ui_us = 0;
            self.acquire_us = 0;
            self.present_us = 0;
            self.hitches = 0;
            self.worst_gap_us = 0;
            self.coalesced_moves = 0;
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub(super) enum TouchKind {
        Down,
        Move,
        Up,
    }

    pub(super) enum EngineMsg {
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
        DocumentPicked {
            tab_id: u64,
            kind: i32,
            path: Option<String>,
        },
        DocumentSaved {
            tab_id: u64,
            kind: i32,
            name: Option<String>,
        },
        StopAllBots,
        TrimMemory(i32),
        FreeMemoryNow,
        GameFieldEdited {
            text: String,
            start: usize,
            end: usize,
        },
        GameFieldEnter,
        GameFieldClosed,
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
    }

    impl AndroidUiBackend {
        fn new() -> Self {
            Self {
                inner: NullUiBackend::new(),
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
            // Handled by `GameField::sync` -> native Android `NativeTextFieldBar` docked above keyboard.
        }
        fn close_virtual_keyboard(&self) {
            // Handled by `GameField::sync`.
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

    /// Computes physical viewport rect `(left_px, raw_w, raw_h)` and scaled offscreen
    /// render dimensions `(drawn_w, drawn_h, render_scale)`.
    fn compute_game_layout(
        win_w: u32,
        win_h: u32,
        ppp: f32,
        rail_collapsed: bool,
        max_render_height: Option<u32>,
    ) -> (f32, u32, u32, u32, u32, f64) {
        let left_px = if rail_collapsed {
            0.0
        } else {
            (MOBILE_RAIL_WIDTH * ppp).round()
        };
        let raw_w = win_w.saturating_sub(left_px as u32).max(1);
        let raw_h = win_h.max(1);
        let render_scale = match max_render_height {
            Some(cap) if raw_h > cap => f64::from(cap) / f64::from(raw_h),
            _ => 1.0,
        };
        let drawn_w = ((f64::from(raw_w) * render_scale).round() as u32).max(1);
        let drawn_h = ((f64::from(raw_h) * render_scale).round() as u32).max(1);
        (left_px, raw_w, raw_h, drawn_w, drawn_h, render_scale)
    }

    const MAX_GAME_FIELD_CHARS: usize = 150;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct GameFieldContents {
        text: String,
        start: usize,
        end: usize,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct FocusedGameField {
        contents: GameFieldContents,
        password: bool,
    }

    /// Synchronizes Ruffle's focused Flash `EditText` with Kotlin's native `NativeTextFieldBar`
    /// docked directly above the soft keyboard (matching Turu's `game_field.rs`).
    #[derive(Default)]
    struct GameField {
        shown: Option<FocusedGameField>,
    }

    impl GameField {
        fn sync(&mut self, app: &AndroidApp, player: &mut Player, aside: bool) {
            let focused = if aside {
                None
            } else {
                inspect_focused_flash_field(player)
            };
            match (&self.shown, focused) {
                (None, Some(focused)) => {
                    jni_show_game_field(app, &focused);
                    self.shown = Some(focused);
                }
                (Some(shown), Some(focused)) if shown.password != focused.password => {
                    // Switched directly between plain text (e.g. username) and password field.
                    jni_show_game_field(app, &focused);
                    self.shown = Some(focused);
                }
                (Some(shown), Some(focused)) if shown.contents != focused.contents => {
                    jni_update_game_field(app, &focused.contents);
                    self.shown = Some(focused);
                }
                (Some(_), None) => {
                    jni_hide_game_field(app);
                    self.shown = None;
                }
                _ => {}
            }
        }

        fn hide(&mut self, app: &AndroidApp) {
            if self.shown.take().is_some() {
                jni_hide_game_field(app);
            }
        }

        fn edited(&mut self, player: &mut Player, text: String, start: usize, end: usize) {
            player.mutate_with_update_context(|context| {
                let Some(field) = context.focus_tracker.get_as_edit_text() else {
                    return;
                };
                if !field.is_editable() {
                    return;
                }
                let old: Vec<u16> = field.text().iter().collect();
                let new: Vec<u16> = text.encode_utf16().collect();
                if old != new {
                    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
                    let most = old.len().min(new.len()) - prefix;
                    let suffix = old
                        .iter()
                        .rev()
                        .zip(new.iter().rev())
                        .take(most)
                        .take_while(|(a, b)| a == b)
                        .count();
                    field.set_selection(Some(TextSelection::for_range(prefix, old.len() - suffix)));
                    let typed = String::from_utf16_lossy(&new[prefix..new.len() - suffix]);
                    if typed.is_empty() {
                        field.text_control_input(TextControlCode::Backspace, context);
                    } else {
                        field.text_input(typed, context);
                    }
                }
                field.set_selection(Some(TextSelection::for_range(start, end)));
            });
            let password = self.shown.as_ref().is_some_and(|s| s.password);
            self.shown = Some(FocusedGameField {
                contents: GameFieldContents { text, start, end },
                password,
            });
        }

        fn closed(&mut self, player: &mut Player) {
            self.shown = None;
            player.mutate_with_update_context(|context| {
                let tracker = context.focus_tracker;
                tracker.set(None, context);
            });
        }
    }

    fn jni_show_game_field(app: &AndroidApp, focused: &FocusedGameField) {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let activity_raw = app.activity_as_ptr() as jni::sys::jobject;
        let res = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&activity_raw) }?;
            let jtext = jni::objects::JString::from_str(env, &focused.contents.text)?;
            env.call_method(
                &*activity,
                jni::jni_str!("showGameField"),
                jni::jni_sig!("(Ljava/lang/String;IIZI)V"),
                &[
                    jni::objects::JValue::Object(&jtext),
                    jni::objects::JValue::Int(focused.contents.start as i32),
                    jni::objects::JValue::Int(focused.contents.end as i32),
                    jni::objects::JValue::Bool(focused.password),
                    jni::objects::JValue::Int(MAX_GAME_FIELD_CHARS as i32),
                ],
            )?;
            Ok(())
        });
        if let Err(e) = res {
            log::warn!("JNI showGameField call failed: {e}");
        }
    }

    fn jni_update_game_field(app: &AndroidApp, contents: &GameFieldContents) {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let activity_raw = app.activity_as_ptr() as jni::sys::jobject;
        let res = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&activity_raw) }?;
            let jtext = jni::objects::JString::from_str(env, &contents.text)?;
            env.call_method(
                &*activity,
                jni::jni_str!("updateGameField"),
                jni::jni_sig!("(Ljava/lang/String;II)V"),
                &[
                    jni::objects::JValue::Object(&jtext),
                    jni::objects::JValue::Int(contents.start as i32),
                    jni::objects::JValue::Int(contents.end as i32),
                ],
            )?;
            Ok(())
        });
        if let Err(e) = res {
            log::warn!("JNI updateGameField call failed: {e}");
        }
    }

    fn jni_hide_game_field(app: &AndroidApp) {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let activity_raw = app.activity_as_ptr() as jni::sys::jobject;
        let res = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&activity_raw) }?;
            env.call_method(
                &*activity,
                jni::jni_str!("hideGameField"),
                jni::jni_sig!("()V"),
                &[],
            )?;
            Ok(())
        });
        if let Err(e) = res {
            log::warn!("JNI hideGameField call failed: {e}");
        }
    }

    /// Reads `MemAvailable` and `MemTotal` (in bytes) from `/proc/meminfo`.
    fn read_meminfo() -> Option<(u64, u64)> {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let field = |name: &str| -> Option<u64> {
            text.lines()
                .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
                .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
                .map(|kb| kb * 1024)
        };
        Some((field("MemAvailable")?, field("MemTotal")?))
    }

    /// Frees unused shapes (>2 min) and idle pooled GPU textures across all tabs.
    fn trim_tabs_memory(tabs: &mut [AndroidTab], why: &str) {
        let shapes = ruffle_core::evict_unused_shapes(120);
        for tab in tabs.iter_mut() {
            if let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                && let Some(r) =
                    <dyn std::any::Any>::downcast_mut::<WgpuRenderBackend<GpuTextureTarget>>(
                        p.renderer_mut(),
                    )
            {
                r.aqw_trim_memory();
            }
        }
        log::info!("memory trim ({why}): freed {shapes} unused shapes and spare GPU textures");
    }

    /// On-demand aggressive trim (Game Options -> "Free Memory Now"): unloads unused SWF
    /// libraries, evicts shapes idle for >30 s, and flushes spare pooled GPU textures.
    fn free_tabs_memory_now(tabs: &mut [AndroidTab], mobile_perf: &MobilePerfState) {
        let mut total_libs = 0usize;
        let mut total_ms = 0.0f64;
        let shapes = ruffle_core::evict_unused_shapes(30);
        for tab in tabs.iter_mut() {
            if let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments) {
                let (libs, took_ms) = p.free_unused_libraries_now();
                total_libs += libs;
                total_ms += took_ms;
                if let Some(r) =
                    <dyn std::any::Any>::downcast_mut::<WgpuRenderBackend<GpuTextureTarget>>(
                        p.renderer_mut(),
                    )
                {
                    r.aqw_trim_memory();
                }
            }
        }
        let msg = format!(
            "Freed {total_libs} SWFs ({total_ms:.1}ms), {shapes} shapes & spare GPU textures"
        );
        log::info!("memory free now: {msg}");
        if let Some(tab) = tabs.first() {
            tab.bot.logs.push_debug(format!("[Memory] {msg}"));
        }
        mobile_perf.set_free_status(msg);
    }

    /// Reads the live text, UTF-16 selection, and password flag from Ruffle's focused `EditText`.
    fn inspect_focused_flash_field(player: &mut Player) -> Option<FocusedGameField> {
        player.mutate_with_update_context(|context| {
            let field = context.focus_tracker.get_as_edit_text()?;
            if !field.is_editable() {
                return None;
            }
            let text = field.text();
            let (start, end) = field
                .selection()
                .map_or((text.len(), text.len()), |s| (s.start(), s.end()));
            Some(FocusedGameField {
                contents: GameFieldContents {
                    text: text.to_utf8_lossy().into_owned(),
                    start,
                    end,
                },
                password: field.is_password(),
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_tab(
        app: &AndroidApp,
        data_dir: &Path,
        mobile_perf: Arc<MobilePerfState>,
        tab_id: u64,
        descriptors: Arc<Descriptors>,
        blit: Arc<GpuBlit>,
        swf_url: &Url,
        engine_tx: mpsc::Sender<EngineMsg>,
        win_w: u32,
        win_h: u32,
        ppp: f32,
        rail_collapsed: bool,
    ) -> AndroidTab {
        let bot = Bot::new();
        let max_h = mobile_perf.render_preset().max_render_height();
        let (_, _, _, drawn_w, drawn_h, render_scale) =
            compute_game_layout(win_w, win_h, ppp, rail_collapsed, max_h);
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
            .with_ui(AndroidUiBackend::new())
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

        let ui = build_android_grimlite_ui(app, tab_id, bot.clone(), mobile_perf);
        AndroidTab {
            id: tab_id,
            bot,
            player,
            segments,
            ui,
            bg_accum_ms: 0.0,
        }
    }

    fn build_android_grimlite_ui(
        app: &AndroidApp,
        tab_id: u64,
        bot: Arc<Bot>,
        mobile_perf: Arc<MobilePerfState>,
    ) -> GrimliteUi {
        let app_gbot_pick = app.clone();
        let pick_gbot: grimlite_egui::bot_managers::FileDialogFn = Box::new(move || {
            jni_open_document(
                &app_gbot_pick,
                tab_id,
                DOC_PICK_GBOT,
                "application/json\napplication/octet-stream\ntext/plain\n*/*",
            );
            None
        });

        let app_gbot_save = app.clone();
        let bot_gbot_save = bot.clone();
        let save_gbot: grimlite_egui::bot_managers::FileDialogFn = Box::new(move || {
            let cfg = bot_gbot_save.runner.config();
            if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                jni_save_document(
                    &app_gbot_save,
                    tab_id,
                    DOC_SAVE_GBOT,
                    "application/json",
                    "bot.gbot",
                    &json,
                );
            }
            None
        });

        let app_maid_pick = app.clone();
        let pick_maid: grimlite_egui::bot_managers::FileDialogFn = Box::new(move || {
            jni_open_document(
                &app_maid_pick,
                tab_id,
                DOC_PICK_MAID_JSON,
                "application/json\ntext/plain\n*/*",
            );
            None
        });

        let app_maid_save = app.clone();
        let bot_maid_save = bot.clone();
        let save_maid: grimlite_egui::bot_managers::FileDialogFn = Box::new(move || {
            let cfg = bot_maid_save.maid.config();
            if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                jni_save_document(
                    &app_maid_save,
                    tab_id,
                    DOC_SAVE_MAID_JSON,
                    "application/json",
                    "maid.json",
                    &json,
                );
            }
            None
        });

        let app_img_pick = app.clone();
        let pick_img: grimlite_egui::bot_managers::FileDialogFn = Box::new(move || {
            jni_open_document(
                &app_img_pick,
                tab_id,
                DOC_PICK_LAG_IMAGE,
                "image/png\nimage/jpeg\nimage/webp",
            );
            None
        });

        GrimliteUi::new(bot, Some(pick_gbot), Some(save_gbot))
            .with_maid_dialogs(Some(pick_maid), Some(save_maid))
            .with_image_dialog(Some(pick_img))
            .with_mobile_perf(mobile_perf)
    }

    fn jni_open_document(app: &AndroidApp, tab_id: u64, kind: i32, mime_types: &str) {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let activity_raw = app.activity_as_ptr() as jni::sys::jobject;
        let res = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&activity_raw) }?;
            let jtypes = jni::objects::JString::from_str(env, mime_types)?;
            env.call_method(
                &*activity,
                jni::jni_str!("openDocument"),
                jni::jni_sig!("(JILjava/lang/String;)V"),
                &[
                    jni::objects::JValue::Long(tab_id as i64),
                    jni::objects::JValue::Int(kind),
                    jni::objects::JValue::Object(&jtypes),
                ],
            )?;
            Ok(())
        });
        if let Err(e) = res {
            log::warn!("JNI openDocument call failed: {e}");
        }
    }

    fn jni_save_document(
        app: &AndroidApp,
        tab_id: u64,
        kind: i32,
        mime_type: &str,
        file_name: &str,
        content: &str,
    ) {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let activity_raw = app.activity_as_ptr() as jni::sys::jobject;
        let res = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&activity_raw) }?;
            let jmime = jni::objects::JString::from_str(env, mime_type)?;
            let jname = jni::objects::JString::from_str(env, file_name)?;
            let jcontent = jni::objects::JString::from_str(env, content)?;
            env.call_method(
                &*activity,
                jni::jni_str!("saveDocument"),
                jni::jni_sig!("(JILjava/lang/String;Ljava/lang/String;Ljava/lang/String;)V"),
                &[
                    jni::objects::JValue::Long(tab_id as i64),
                    jni::objects::JValue::Int(kind),
                    jni::objects::JValue::Object(&jmime),
                    jni::objects::JValue::Object(&jname),
                    jni::objects::JValue::Object(&jcontent),
                ],
            )?;
            Ok(())
        });
        if let Err(e) = res {
            log::warn!("JNI saveDocument call failed: {e}");
        }
    }

    fn jni_update_bot_service(app: &AndroidApp, active: bool, title: &str, text: &str) {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
        let activity_raw = app.activity_as_ptr() as jni::sys::jobject;
        let res = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&activity_raw) }?;
            let jtitle = jni::objects::JString::from_str(env, title)?;
            let jtext = jni::objects::JString::from_str(env, text)?;
            env.call_method(
                &*activity,
                jni::jni_str!("updateBotService"),
                jni::jni_sig!("(ZLjava/lang/String;Ljava/lang/String;)V"),
                &[
                    jni::objects::JValue::Bool(active),
                    jni::objects::JValue::Object(&jtitle),
                    jni::objects::JValue::Object(&jtext),
                ],
            )?;
            Ok(())
        });
        if let Err(e) = res {
            log::warn!("JNI updateBotService call failed: {e}");
        }
    }

    fn summarize_botting_status(tabs: &[AndroidTab]) -> (bool, String, String) {
        let mut lines: Vec<String> = Vec::new();
        for tab in tabs {
            let bot_on = tab.bot.runner.is_running();
            let maid_on = tab.bot.maid.is_running();
            let aa_on = tab.bot.auto_attack.is_running();
            if !bot_on && !maid_on && !aa_on {
                continue;
            }
            let snap = tab.bot.player_snapshot();
            let who = if snap.logged_in && !snap.username.trim().is_empty() {
                snap.username
            } else {
                format!("Account #{}", tab.id)
            };
            let mut modes = Vec::new();
            if bot_on {
                modes.push("Bot");
            }
            if maid_on {
                modes.push("Maid");
            }
            if aa_on {
                modes.push("Auto");
            }
            let mode_str = modes.join(" + ");
            if !snap.map.is_empty() {
                lines.push(format!("{who} · {} [{mode_str}]", snap.map));
            } else {
                lines.push(format!("{who} [{mode_str}]"));
            }
        }
        if lines.is_empty() {
            return (false, String::new(), String::new());
        }
        let title = if lines.len() == 1 {
            "Grimlite Ri — Bot Running".to_string()
        } else {
            format!("Grimlite Ri — {} Bots Running", lines.len())
        };
        (true, title, lines.join("\n"))
    }

    fn resize_tab_viewport(
        tab: &mut AndroidTab,
        win_w: u32,
        win_h: u32,
        ppp: f32,
        rail_collapsed: bool,
        max_render_height: Option<u32>,
    ) {
        let (_, _, _, drawn_w, drawn_h, render_scale) =
            compute_game_layout(win_w, win_h, ppp, rail_collapsed, max_render_height);
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
        preferred: MobileGraphicsApi,
    ) -> Option<(
        Arc<Descriptors>,
        Arc<GpuBlit>,
        wgpu::RenderPipeline,
        wgpu::Surface<'static>,
        wgpu::SurfaceConfiguration,
        &'static str,
    )> {
        let w = native_win.width().max(1) as u32;
        let h = native_win.height().max(1) as u32;
        let backends = match preferred {
            MobileGraphicsApi::Vulkan => [wgpu::Backends::VULKAN, wgpu::Backends::GL],
            MobileGraphicsApi::Auto | MobileGraphicsApi::Gl => {
                [wgpu::Backends::GL, wgpu::Backends::VULKAN]
            }
        };
        for backend in backends {
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
            let backend_name = info.backend.to_str();
            log::info!(
                "Graphics initialized: {} on {} ({:?}) [preference: {:?}]",
                backend_name,
                info.name,
                info.device_type,
                preferred
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
            return Some((desc, blit, blit_pipeline, surf, cfg, backend_name));
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

    /// Updates `buf` in-place from `tabs` without cloning `PlayerSnapshot` or reallocating
    /// unchanged tab labels on every frame.
    fn sync_tab_descriptors(tabs: &[AndroidTab], buf: &mut Vec<TabDescriptor>) {
        buf.truncate(tabs.len());
        for (i, t) in tabs.iter().enumerate() {
            let bot_running = t.bot.runner.is_running()
                || t.bot.auto_attack.is_running()
                || t.bot.maid.is_running();
            if let Some(entry) = buf.get_mut(i) {
                let id_changed = entry.id != t.id;
                entry.id = t.id;
                entry.bot_running = bot_running;
                t.bot.read_player(|snap| {
                    let prev_logged_in = entry.logged_in;
                    entry.logged_in = snap.logged_in;
                    let trimmed = snap.username.trim();
                    if snap.logged_in && !trimmed.is_empty() {
                        if entry.label != trimmed {
                            entry.label.clear();
                            entry.label.push_str(trimmed);
                        }
                    } else if id_changed || prev_logged_in || entry.label.is_empty() {
                        use std::fmt::Write as _;
                        entry.label.clear();
                        let _ = write!(&mut entry.label, "Account #{}", t.id);
                    }
                });
            } else {
                let (logged_in, label) = t.bot.read_player(|snap| {
                    let trimmed = snap.username.trim();
                    let label = if snap.logged_in && !trimmed.is_empty() {
                        trimmed.to_string()
                    } else {
                        format!("Account #{}", t.id)
                    };
                    (snap.logged_in, label)
                });
                buf.push(TabDescriptor {
                    id: t.id,
                    label,
                    logged_in,
                    bot_running,
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_screen(
        app: &AndroidApp,
        data_dir: &Path,
        swf_url: &Url,
        engine_tx: &mpsc::Sender<EngineMsg>,
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
        tab_descriptors: &mut Vec<TabDescriptor>,
        active_idx: &mut usize,
        next_tab_id: &mut u64,
        rail_collapsed: &mut bool,
        egui_ime_active: &mut bool,
        game_field: &mut GameField,
        pending_egui_events: &mut Vec<egui::Event>,
        ui_due: &mut Option<Instant>,
        needs_game_render: &mut bool,
        mobile_perf: &Arc<MobilePerfState>,
        perf: &mut FramePerf,
    ) {
        let draw_started = Instant::now();
        let ui_started = draw_started;
        let prev_collapsed = *rail_collapsed;
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
        sync_tab_descriptors(tabs, tab_descriptors);

        // Note: NO `CentralPanel` `ui.image` here — the game texture is blitted directly onto the
        // surface via `GpuBlit` (`BlendState::REPLACE`) underneath egui's chrome & windows!
        // Flash text input uses Kotlin's native `NativeTextFieldBar` docked directly above the
        // soft keyboard (`GameField::sync`), never taking space at the top of the Flash viewport.
        let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
            if let Some(tab) = tabs.get_mut(*active_idx) {
                tab_action = tab.ui.show_mobile(
                    egui_ctx,
                    ui,
                    tab_descriptors,
                    active_id,
                    rail_collapsed,
                );
            }
        });

        *ui_due = full_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .and_then(|vp| Instant::now().checked_add(vp.repaint_delay));

        let wants_egui_ime = full_output.platform_output.ime.is_some();
        if wants_egui_ime != *egui_ime_active {
            *egui_ime_active = wants_egui_ime;
            if wants_egui_ime {
                game_field.hide(app);
                show_ime(app, ime_reset_flag);
            } else {
                hide_ime(app, ime_reset_flag);
            }
        }

        let max_render_h = mobile_perf.render_preset().max_render_height();
        if *rail_collapsed != prev_collapsed {
            for tab in tabs.iter_mut() {
                resize_tab_viewport(
                    tab,
                    cfg.width,
                    cfg.height,
                    scale_factor,
                    *rail_collapsed,
                    max_render_h,
                );
            }
            *needs_game_render = true;
        }

        let prev_active_idx = *active_idx;
        let mut tab_switched = false;
        match tab_action {
            TabBarAction::NewTab => {
                if let Some(old_tab) = tabs.get_mut(prev_active_idx)
                    && let Some(mut p) = TabPlayerGuard::try_lock(&old_tab.player, &old_tab.segments)
                {
                    p.set_mouse_in_stage(false);
                    p.handle_event(PlayerEvent::MouseLeave);
                    p.handle_event(PlayerEvent::FocusLost);
                }
                let tab = spawn_tab(
                    app,
                    data_dir,
                    mobile_perf.clone(),
                    *next_tab_id,
                    desc.clone(),
                    blit.clone(),
                    swf_url,
                    engine_tx.clone(),
                    cfg.width,
                    cfg.height,
                    scale_factor,
                    *rail_collapsed,
                );
                *next_tab_id += 1;
                tabs.push(tab);
                *active_idx = tabs.len() - 1;
                *needs_game_render = true;
                tab_switched = true;
            }
            TabBarAction::Switch(id) => {
                if let Some(idx) = tabs.iter().position(|t| t.id == id)
                    && idx != prev_active_idx
                {
                    if let Some(old_tab) = tabs.get_mut(prev_active_idx)
                        && let Some(mut p) =
                            TabPlayerGuard::try_lock(&old_tab.player, &old_tab.segments)
                    {
                        p.set_mouse_in_stage(false);
                        p.handle_event(PlayerEvent::MouseLeave);
                        p.handle_event(PlayerEvent::FocusLost);
                    }
                    *active_idx = idx;
                    *needs_game_render = true;
                    tab_switched = true;
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
                    tab_switched = true;
                }
            }
            _ => {}
        }

        if tab_switched {
            egui_ctx.memory_mut(|mem| {
                if let Some(focused) = mem.focused() {
                    mem.surrender_focus(focused);
                }
            });
            game_field.hide(app);
            if *egui_ime_active {
                *egui_ime_active = false;
                hide_ime(app, ime_reset_flag);
            }
            if let Some(active_tab) = tabs.get_mut(*active_idx) {
                resize_tab_viewport(
                    active_tab,
                    cfg.width,
                    cfg.height,
                    scale_factor,
                    *rail_collapsed,
                    max_render_h,
                );
                if let Some(mut p) =
                    TabPlayerGuard::try_lock(&active_tab.player, &active_tab.segments)
                {
                    p.set_mouse_in_stage(true);
                    p.handle_event(PlayerEvent::FocusGained);
                }
            }

            // Re-run egui immediately for the newly active tab so `egui_ctx.layer_id_at(...)`
            // drops the previous tab's flyout/window rectangles before the next touch arrives.
            let new_active_id = tabs.get(*active_idx).map(|t| t.id).unwrap_or(0);
            sync_tab_descriptors(tabs, tab_descriptors);
            let mut switched_input = RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(
                        cfg.width as f32 / scale_factor,
                        cfg.height as f32 / scale_factor,
                    ),
                )),
                time: Some(started.elapsed().as_secs_f64()),
                max_texture_side: Some(desc.limits.max_texture_dimension_2d as usize),
                ..Default::default()
            };
            if let Some(vp) = switched_input.viewports.get_mut(&egui::ViewportId::ROOT) {
                vp.native_pixels_per_point = Some(scale_factor);
            }
            let switched_output = egui_ctx.run_ui(switched_input, |ui| {
                if let Some(tab) = tabs.get_mut(*active_idx) {
                    let _ = tab.ui.show_mobile(
                        egui_ctx,
                        ui,
                        tab_descriptors,
                        new_active_id,
                        rail_collapsed,
                    );
                }
            });
            full_output.textures_delta.append(switched_output.textures_delta);
            full_output.shapes = switched_output.shapes;
            full_output.pixels_per_point = switched_output.pixels_per_point;
            full_output.platform_output = switched_output.platform_output;
            full_output.viewport_output = switched_output.viewport_output;
            *ui_due = full_output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .and_then(|vp| Instant::now().checked_add(vp.repaint_delay));
        }

        let clipped = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let ui_dur = ui_started.elapsed();
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [cfg.width, cfg.height],
            pixels_per_point: scale_factor,
        };
        for (id, deltas) in &full_output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(&desc.device, &desc.queue, *id, delta);
            }
        }

        let acquire_started = Instant::now();
        let acquired = surf.get_current_texture();
        let acquire_dur = acquire_started.elapsed();
        let mut present_dur = Duration::ZERO;
        match acquired {
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
                    // Read-only renderer borrow: no need to swap thread-local `segments` via TabPlayerGuard.
                    if let Some(tab) = tabs.get(*active_idx)
                        && let Ok(p) = tab.player.try_lock()
                        && let Some(movie) = <dyn std::any::Any>::downcast_ref::<
                            WgpuRenderBackend<GpuTextureTarget>,
                        >(p.renderer())
                        .map(|r| r.target())
                    {
                        let (left_px, raw_w, raw_h, _, _, _) = compute_game_layout(
                            cfg.width,
                            cfg.height,
                            scale_factor,
                            *rail_collapsed,
                            mobile_perf.render_preset().max_render_height(),
                        );
                        pass.set_viewport(
                            left_px,
                            0.0,
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
                let present_started = Instant::now();
                desc.queue.submit(cmds);
                desc.queue.present(frame);
                present_dur = present_started.elapsed();
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
        perf.record_draw(draw_started.elapsed(), ui_dur, acquire_dur, present_dur);
    }

    fn engine_loop(
        app: AndroidApp,
        data_dir: PathBuf,
        swf_url: Url,
        engine_tx: mpsc::Sender<EngineMsg>,
        engine_rx: mpsc::Receiver<EngineMsg>,
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
        let mut tab_descriptors: Vec<TabDescriptor> = Vec::new();
        let mut active_idx: usize = 0;
        let mut next_tab_id: u64 = 1;
        let mut rail_collapsed = false;
        let mut paused = false;
        let mut egui_ime_active = false;
        let mut game_field = GameField::default();
        let mut needs_game_render = true;
        let mut ui_dirty = true;
        let mut ui_due: Option<Instant> = None;
        let mut pending_egui_events: Vec<egui::Event> = Vec::new();
        let started = Instant::now();
        let free_tx = engine_tx.clone();
        let mobile_perf = MobilePerfState::load(
            data_dir.clone(),
            Box::new(move || {
                let _ = free_tx.send(EngineMsg::FreeMemoryNow);
            }),
        );
        let mut last_render_preset: MobileRenderPreset = mobile_perf.render_preset();

        let mut last_tick = Instant::now();
        let mut last_draw = Instant::now() - Duration::from_secs(1);
        let mut last_picture = Instant::now() - Duration::from_secs(1);
        let mut last_shape_sweep = Instant::now();
        let mut last_gpu_maint = Instant::now();
        let mut last_low_mem_trim: Option<Instant> = None;
        let mut last_notif_check = Instant::now() - Duration::from_secs(2);
        let mut last_notif_state: (bool, String, String) = (false, String::new(), String::new());
        let mut touch_on_ui = false;
        let mut pending_game_move: Option<(f64, f64)> = None;
        let mut perf = FramePerf::new();

        #[allow(clippy::too_many_arguments)]
        fn handle_msg(
            msg: EngineMsg,
            app: &AndroidApp,
            data_dir: &Path,
            swf_url: &Url,
            engine_tx: &mpsc::Sender<EngineMsg>,
            mobile_perf: &Arc<MobilePerfState>,
            egui_ctx: &Context,
            scale_factor: f32,
            rail_collapsed: bool,
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
            game_field: &mut GameField,
            touch_on_ui: &mut bool,
            pending_game_move: &mut Option<(f64, f64)>,
            perf: &mut FramePerf,
        ) -> bool {
            match msg {
                EngineMsg::Destroy => return true,
                EngineMsg::Paused(p) => {
                    *paused = p;
                    if p {
                        *pending_game_move = None;
                        game_field.hide(app);
                    } else {
                        *needs_game_render = true;
                        *ui_dirty = true;
                    }
                }
                EngineMsg::DetachWindow(done) => {
                    *pending_game_move = None;
                    game_field.hide(app);
                    *surface = None;
                    *held_window = None;
                    let _ = done.send(());
                    trim_tabs_memory(tabs, "window detached");
                }
                EngineMsg::InitWindow => {
                    if let Some(native_win) = app.native_window() {
                        let w = native_win.width().max(1) as u32;
                        let h = native_win.height().max(1) as u32;
                        let max_h = mobile_perf.render_preset().max_render_height();
                        if let Some(desc) = descriptors.as_ref() {
                            *surface = None;
                            if let Ok(surf) = make_raw_surface(&desc.wgpu_instance, &native_win) {
                                if let Some(cfg) = surface_config.as_mut() {
                                    cfg.width = w;
                                    cfg.height = h;
                                    surf.configure(&desc.device, cfg);
                                }
                                for tab in tabs.iter_mut() {
                                    resize_tab_viewport(
                                        tab,
                                        w,
                                        h,
                                        scale_factor,
                                        rail_collapsed,
                                        max_h,
                                    );
                                }
                                *surface = Some(surf);
                                *held_window = Some(native_win);
                                *needs_game_render = true;
                                *ui_dirty = true;
                            }
                        } else if let Some((desc, gpu_blit, pipe, surf, cfg, backend_name)) =
                            create_mobile_gpu(&native_win, mobile_perf.graphics_api())
                        {
                            mobile_perf.set_active_backend(backend_name);
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
                                    mobile_perf.clone(),
                                    *next_tab_id,
                                    desc.clone(),
                                    gpu_blit.clone(),
                                    swf_url,
                                    engine_tx.clone(),
                                    w,
                                    h,
                                    scale_factor,
                                    rail_collapsed,
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
                            let max_h = mobile_perf.render_preset().max_render_height();
                            for tab in tabs.iter_mut() {
                                resize_tab_viewport(
                                    tab,
                                    w,
                                    h,
                                    scale_factor,
                                    rail_collapsed,
                                    max_h,
                                );
                            }
                            *needs_game_render = true;
                            *ui_dirty = true;
                        }
                    }
                }
                EngineMsg::Touch { kind, px, py } => {
                    let logical = Pos2::new(px / scale_factor, py / scale_factor);
                    let rail_w = if rail_collapsed {
                        0.0
                    } else {
                        MOBILE_RAIL_WIDTH
                    };
                    let flyout_open = tabs
                        .get(*active_idx)
                        .is_some_and(|t| t.ui.has_active_flyout());
                    if matches!(kind, TouchKind::Down) {
                        *touch_on_ui = px < rail_w * scale_factor
                            || flyout_open
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
                    if !*touch_on_ui {
                        let (win_w, win_h) = surface_config
                            .as_ref()
                            .map_or((1920, 1080), |c| (c.width, c.height));
                        let (left_px, _, _, _, _, render_scale) = compute_game_layout(
                            win_w,
                            win_h,
                            scale_factor,
                            rail_collapsed,
                            mobile_perf.render_preset().max_render_height(),
                        );
                        let gx = f64::from((px - left_px).max(0.0)) * render_scale;
                        let gy = f64::from(py.max(0.0)) * render_scale;
                        match kind {
                            TouchKind::Move => {
                                // Coalesce 60-240 Hz touch moves: flush at most once per game tick
                                // so Ruffle does not run full display-list hit-tests per touch sample.
                                if pending_game_move.replace((gx, gy)).is_some() {
                                    perf.coalesced_moves += 1;
                                }
                            }
                            TouchKind::Down => {
                                *pending_game_move = None;
                                if let Some(tab) = tabs.get(*active_idx)
                                    && let Some(mut p) =
                                        TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                                {
                                    p.set_mouse_in_stage(true);
                                    p.handle_event(PlayerEvent::MouseMove { x: gx, y: gy });
                                    p.handle_event(PlayerEvent::MouseDown {
                                        x: gx,
                                        y: gy,
                                        button: MouseButton::Left,
                                        index: None,
                                    });
                                }
                            }
                            TouchKind::Up => {
                                let had_move = pending_game_move.take().is_some();
                                if let Some(tab) = tabs.get(*active_idx)
                                    && let Some(mut p) =
                                        TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                                {
                                    if had_move {
                                        p.handle_event(PlayerEvent::MouseMove { x: gx, y: gy });
                                    }
                                    p.handle_event(PlayerEvent::MouseUp {
                                        x: gx,
                                        y: gy,
                                        button: MouseButton::Left,
                                    });
                                }
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
                            send_flash_backspace(&mut p);
                        }
                        for ch in inserted.chars() {
                            if ch == '\n' || ch == '\r' {
                                send_flash_enter(&mut p);
                            } else {
                                p.handle_event(PlayerEvent::TextInput { codepoint: ch });
                            }
                        }
                        *needs_game_render = true;
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
                        send_flash_backspace(&mut p);
                        *needs_game_render = true;
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
                        *needs_game_render = true;
                    }
                }
                EngineMsg::KeyChar(ch) => {
                    if egui_ime_active {
                        pending_egui_events.push(egui::Event::Text(ch.to_string()));
                        *ui_dirty = true;
                    } else if let Some(tab) = tabs.get(*active_idx)
                        && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                    {
                        p.handle_event(PlayerEvent::TextInput { codepoint: ch });
                        *needs_game_render = true;
                    }
                }
                EngineMsg::GameFieldEdited { text, start, end } => {
                    if let Some(tab) = tabs.get(*active_idx)
                        && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                    {
                        game_field.edited(&mut p, text, start, end);
                        *needs_game_render = true;
                    }
                }
                EngineMsg::GameFieldEnter => {
                    if let Some(tab) = tabs.get(*active_idx)
                        && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                    {
                        send_flash_enter(&mut p);
                        *needs_game_render = true;
                    }
                }
                EngineMsg::GameFieldClosed => {
                    if let Some(tab) = tabs.get(*active_idx)
                        && let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                    {
                        game_field.closed(&mut p);
                        *needs_game_render = true;
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
                EngineMsg::DocumentPicked { tab_id, kind, path } => {
                    if let Some(raw_path) = path {
                        let picked = PathBuf::from(&raw_path);
                        if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
                            match kind {
                                DOC_PICK_LAG_IMAGE => tab.ui.apply_picked_lag_killer_image(&picked),
                                DOC_PICK_GBOT => tab.ui.apply_picked_gbot(&picked),
                                DOC_PICK_MAID_JSON => tab.ui.apply_picked_maid_json(&picked),
                                _ => {}
                            }
                            *ui_dirty = true;
                        }
                        let _ = std::fs::remove_file(&picked);
                        if let Some(parent) = picked.parent() {
                            let _ = std::fs::remove_dir(parent);
                        }
                    }
                }
                EngineMsg::DocumentSaved { tab_id, kind, name } => {
                    if let Some(saved_name) = name
                        && let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id)
                    {
                        match kind {
                            DOC_SAVE_GBOT => tab.ui.notify_saved_gbot(&saved_name),
                            DOC_SAVE_MAID_JSON => tab.ui.notify_saved_maid_json(&saved_name),
                            _ => {}
                        }
                        *ui_dirty = true;
                    }
                }
                EngineMsg::StopAllBots => {
                    for tab in tabs.iter() {
                        tab.bot.stop_bot();
                        tab.bot.auto_attack.stop(tab.bot.clone());
                        tab.bot.maid.stop(&tab.bot);
                    }
                    jni_update_bot_service(app, false, "", "");
                    *ui_dirty = true;
                }
                EngineMsg::TrimMemory(level) => {
                    trim_tabs_memory(tabs, &format!("Android onTrimMemory({level})"));
                }
                EngineMsg::FreeMemoryNow => {
                    free_tabs_memory_now(tabs, mobile_perf);
                    *ui_dirty = true;
                }
            }
            false
        }

        // Outer loop: ticks SWF ONLY when a game frame is actually due (`run_loop_paced` pattern).
        loop {
            let now = Instant::now();
            perf.maybe_log(now);
            let mut next_frame = now + Duration::from_millis(33);

            // Apply live Render Scale preset changes immediately without requiring a restart.
            let current_preset = mobile_perf.render_preset();
            if current_preset != last_render_preset {
                last_render_preset = current_preset;
                if let Some(cfg) = surface_config.as_ref() {
                    let max_h = current_preset.max_render_height();
                    for tab in tabs.iter_mut() {
                        resize_tab_viewport(
                            tab,
                            cfg.width,
                            cfg.height,
                            scale_factor,
                            rail_collapsed,
                            max_h,
                        );
                    }
                    needs_game_render = true;
                    ui_dirty = true;
                    log::info!(
                        "Render preset switched to {:?} (max height: {:?})",
                        current_preset,
                        max_h
                    );
                }
            }

            // Periodic 10s GPU poll (releases finished command buffers even when idle/background)
            // + `/proc/meminfo` low-memory check (`turu`'s `memory::tick`).
            if now.duration_since(last_gpu_maint) >= GPU_MAINT_EVERY {
                last_gpu_maint = now;
                if ruffle_render_wgpu::device_lost().is_none()
                    && let Some(desc) = descriptors.as_ref()
                {
                    let _ = desc.device.poll(wgpu::PollType::Poll);
                }
                if let Some((available, total)) = read_meminfo()
                    && available * 100 < total * LOW_MEM_PERCENT
                    && last_low_mem_trim
                        .is_none_or(|t| now.duration_since(t) >= LOW_MEM_TRIM_EVERY)
                {
                    last_low_mem_trim = Some(now);
                    trim_tabs_memory(
                        &mut tabs,
                        &format!(
                            "low phone memory ({}/{} MB available)",
                            available >> 20,
                            total >> 20
                        ),
                    );
                }
            }

            if now.duration_since(last_notif_check) >= Duration::from_secs(1) {
                last_notif_check = now;
                let desired = summarize_botting_status(&tabs);
                if desired != last_notif_state {
                    jni_update_bot_service(&app, desired.0, &desired.1, &desired.2);
                    last_notif_state = desired;
                }
            }

            if !paused {
                let dt = FloatDuration::from_std(now.duration_since(last_tick));
                last_tick = now;

                if now.duration_since(last_shape_sweep) >= SHAPE_SWEEP_EVERY {
                    last_shape_sweep = now;
                    let _ = ruffle_core::evict_unused_shapes(300);
                }

                let mut should_present = false;
                let mut skipped_this_tick = false;
                for (idx, tab) in tabs.iter_mut().enumerate() {
                    if idx == active_idx {
                        if let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments) {
                            if let Some((gx, gy)) = pending_game_move.take() {
                                p.handle_event(PlayerEvent::MouseMove { x: gx, y: gy });
                            }
                            let before = p.frames_run();
                            let tick_started = Instant::now();
                            p.tick(dt);
                            let ran_frames = p.frames_run().saturating_sub(before);
                            let frame_dur =
                                Duration::from_secs_f64(1.0 / p.frame_rate().max(1.0));
                            if ran_frames > 0 {
                                perf.record_frames(ran_frames, tick_started.elapsed(), frame_dur);
                            }

                            game_field.sync(&app, &mut p, egui_ime_active);

                            // Anchor `next_frame` to `now` (START of tick), NOT `Instant::now()`
                            // after `tick()` + `render()`: otherwise render time shifts every
                            // frame late, inflating the next `dt` and triggering a sawtooth
                            // `behind == true` render-skip cycle while in-game FPS still reads 24/30!
                            let til_next = p.time_til_next_frame();
                            next_frame = now + til_next;

                            if (ran_frames > 0 && p.needs_render()) || needs_game_render {
                                // `Player::time_til_next_frame()` returns `min(frame_remaining, time_til_next_timer)`.
                                // When an AQW AS3 `Timer` is due within half a frame, `til_next` is small even
                                // though `frame_accumulator == 0`! Guard against false-positive behind skips.
                                let clamped_by_timer = p.time_til_next_timer().is_some_and(|ms| {
                                    ms >= 0.0 && ms <= til_next.as_secs_f64() * 1000.0 + 0.5
                                });
                                let behind = p.is_playing()
                                    && !needs_game_render
                                    && last_picture.elapsed() < MAX_DRAW_GAP
                                    && if clamped_by_timer {
                                        dt.as_millis() >= frame_dur.as_secs_f64() * 1500.0
                                    } else {
                                        til_next * 2 < frame_dur
                                    };
                                if behind {
                                    perf.skipped_behind += 1;
                                    skipped_this_tick = true;
                                } else if surface.is_some() {
                                    let render_started = Instant::now();
                                    p.render();
                                    perf.record_render(render_started.elapsed(), &mobile_perf);
                                    last_picture = Instant::now();
                                    needs_game_render = false;
                                    should_present = true;
                                }
                            }
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
                    let active_before = active_idx;
                    ui_dirty = false;
                    last_draw = Instant::now();
                    draw_screen(
                        &app,
                        &data_dir,
                        &swf_url,
                        &engine_tx,
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
                        &mut tab_descriptors,
                        &mut active_idx,
                        &mut next_tab_id,
                        &mut rail_collapsed,
                        &mut egui_ime_active,
                        &mut game_field,
                        &mut pending_egui_events,
                        &mut ui_due,
                        &mut needs_game_render,
                        &mobile_perf,
                        &mut perf,
                    );
                    if active_idx != active_before {
                        pending_game_move = None;
                    }
                }
                if skipped_this_tick {
                    // Don't let a standalone UI repaint fire right after a skipped game frame.
                    last_draw = Instant::now();
                }
            } else {
                // App is minimized or screen is off: zero GPU rendering, while keeping active
                // bots/Maid/AutoAttack ticking at a lean 10 FPS headless cadence (protected by
                // GrimliteService's foreground notification + PARTIAL_WAKE_LOCK).
                let dt = FloatDuration::from_std(
                    now.duration_since(last_tick).min(Duration::from_millis(250)),
                );
                last_tick = now;

                let mut any_botting = false;
                for tab in tabs.iter_mut() {
                    let bot_busy = !tab.bot.is_game_loaded()
                        || tab.bot.runner.is_running()
                        || tab.bot.auto_attack.is_running()
                        || tab.bot.maid.is_running();
                    if bot_busy {
                        any_botting = true;
                        if let Some(mut p) = TabPlayerGuard::try_lock(&tab.player, &tab.segments) {
                            if p.mouse_in_stage() {
                                p.set_mouse_in_stage(false);
                                p.handle_event(PlayerEvent::MouseLeave);
                            }
                            p.tick(dt);
                        }
                    } else if tab.bot.is_logged_in() {
                        tab.bg_accum_ms += dt.as_millis();
                        if tab.bg_accum_ms >= 500.0 {
                            let step_ms = tab.bg_accum_ms.min(500.0);
                            tab.bg_accum_ms = 0.0;
                            if let Some(mut p) =
                                TabPlayerGuard::try_lock(&tab.player, &tab.segments)
                            {
                                p.tick(FloatDuration::from_millis(step_ms));
                            }
                        }
                    }
                }
                next_frame = now
                    + if any_botting {
                        Duration::from_millis(100)
                    } else {
                        Duration::from_millis(500)
                    };
            }

            // Inner loop: drains async tasks, FlashCalls, and touch/key events UNTIL `next_frame`
            // without running premature SWF ticks or stealing a due game frame's vsync slot!
            loop {
                let now = Instant::now();
                if now >= next_frame {
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
                            &mobile_perf,
                            &egui_ctx,
                            scale_factor,
                            rail_collapsed,
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
                            &mut game_field,
                            &mut touch_on_ui,
                            &mut pending_game_move,
                            &mut perf,
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
                                &mobile_perf,
                                &egui_ctx,
                                scale_factor,
                                rail_collapsed,
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
                                &mut game_field,
                                &mut touch_on_ui,
                                &mut pending_game_move,
                                &mut perf,
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
                if now >= next_frame {
                    break;
                }

                // Standalone UI-only draw between game frames (only when >=16ms away from next
                // SWF frame so it never steals the game frame's vsync slot!).
                let ui_ready = (ui_dirty || ui_due.is_some_and(|d| now >= d))
                    && last_draw.elapsed() >= UI_FRAME
                    && next_frame.saturating_duration_since(now) >= Duration::from_millis(16);
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
                    let active_before = active_idx;
                    ui_dirty = false;
                    last_draw = now;
                    draw_screen(
                        &app,
                        &data_dir,
                        &swf_url,
                        &engine_tx,
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
                        &mut tab_descriptors,
                        &mut active_idx,
                        &mut next_tab_id,
                        &mut rail_collapsed,
                        &mut egui_ime_active,
                        &mut game_field,
                        &mut pending_egui_events,
                        &mut ui_due,
                        &mut needs_game_render,
                        &mobile_perf,
                        &mut perf,
                    );
                    if active_idx != active_before {
                        pending_game_move = None;
                    }
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
        grimlite_bot::options::set_cache_dir(data_dir.join("cache"));
        let swf_path = ensure_host_swf(&data_dir);
        let swf_url = Url::from_file_path(&swf_path).expect("host swf file url");

        let (engine_tx, engine_rx) = mpsc::channel::<EngineMsg>();
        if let Ok(mut slot) = GLOBAL_ENGINE_TX.get_or_init(|| Mutex::new(None)).lock() {
            *slot = Some(engine_tx.clone());
        }
        let ime_reset_flag = Arc::new(AtomicBool::new(false));

        let engine_app = app.clone();
        let engine_tx_clone = engine_tx.clone();
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
        if let Some(slot) = GLOBAL_ENGINE_TX.get()
            && let Ok(mut guard) = slot.lock()
        {
            *guard = None;
        }
        jni_update_bot_service(&app, false, "", "");
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: android_activity::AndroidApp) {
    runtime::run(app);
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_grimlite_ri_MainActivity_nativeOnDocumentPicked<'local>(
    mut unowned_env: jni::EnvUnowned<'local>,
    _class: jni::objects::JClass<'local>,
    tab_id: jni::sys::jlong,
    kind: jni::sys::jint,
    path: jni::objects::JString<'local>,
) {
    let resolved = unowned_env
        .with_env(|env| -> jni::errors::Result<Option<String>> {
            if path.is_null() {
                return Ok(None);
            }
            Ok(Some(path.try_to_string(env)?))
        })
        .resolve::<jni::errors::LogErrorAndDefault>();
    runtime::send_global_engine_msg(runtime::EngineMsg::DocumentPicked {
        tab_id: tab_id as u64,
        kind,
        path: resolved,
    });
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_grimlite_ri_MainActivity_nativeOnDocumentSaved<'local>(
    mut unowned_env: jni::EnvUnowned<'local>,
    _class: jni::objects::JClass<'local>,
    tab_id: jni::sys::jlong,
    kind: jni::sys::jint,
    name: jni::objects::JString<'local>,
) {
    let resolved = unowned_env
        .with_env(|env| -> jni::errors::Result<Option<String>> {
            if name.is_null() {
                return Ok(None);
            }
            Ok(Some(name.try_to_string(env)?))
        })
        .resolve::<jni::errors::LogErrorAndDefault>();
    runtime::send_global_engine_msg(runtime::EngineMsg::DocumentSaved {
        tab_id: tab_id as u64,
        kind,
        name: resolved,
    });
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_grimlite_ri_MainActivity_nativeStopAllBots(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
) {
    runtime::send_global_engine_msg(runtime::EngineMsg::StopAllBots);
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_grimlite_ri_MainActivity_nativeTrimMemory(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
    level: jni::sys::jint,
) {
    runtime::send_global_engine_msg(runtime::EngineMsg::TrimMemory(level));
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_grimlite_ri_MainActivity_nativeOnGameFieldEdited<'local>(
    mut unowned_env: jni::EnvUnowned<'local>,
    _class: jni::objects::JClass<'local>,
    text: jni::objects::JString<'local>,
    start: jni::sys::jint,
    end: jni::sys::jint,
) {
    let resolved = unowned_env
        .with_env(|env| -> jni::errors::Result<String> {
            if text.is_null() {
                return Ok(String::new());
            }
            text.try_to_string(env)
        })
        .resolve::<jni::errors::LogErrorAndDefault>();
    runtime::send_global_engine_msg(runtime::EngineMsg::GameFieldEdited {
        text: resolved,
        start: start.max(0) as usize,
        end: end.max(0) as usize,
    });
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_grimlite_ri_MainActivity_nativeOnGameFieldEnter(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
) {
    runtime::send_global_engine_msg(runtime::EngineMsg::GameFieldEnter);
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_grimlite_ri_MainActivity_nativeOnGameFieldClosed(
    _env: jni::EnvUnowned<'_>,
    _class: jni::objects::JClass<'_>,
) {
    runtime::send_global_engine_msg(runtime::EngineMsg::GameFieldClosed);
}
