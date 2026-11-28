//! Turu-style Headless Ruffle engine: runs `grimlite.swf` -> AQW `Game.swf` with
//! `NullRenderer` (0% GPU, no window, no sound, 2 FPS once in-game).

use grimlite_bot::Bot;
use grimlite_bot::host::FlashArg;
use grimlite_frontend::capture::CaptureNavigator;
use grimlite_frontend::external::GrimliteExternalInterface;
use grimlite_frontend::ruffle_host::{FlashCallRequest, RuffleFlashHost, answer_flash_call};
use ruffle_core::backend::navigator::{OwnedFuture, SocketMode};
use ruffle_core::backend::ui::NullUiBackend;
use ruffle_core::config::Letterbox;
use ruffle_core::{FloatDuration, LoadBehavior, PlayerBuilder};
use ruffle_frontend_utils::backends::navigator::{
    ExternalNavigatorBackend, FutureSpawner, NavigatorInterface,
};
use ruffle_frontend_utils::backends::storage::DiskStorageBackend;
use ruffle_frontend_utils::content::{ContentDescriptor, PlayingContent};
use ruffle_render::backend::ViewportDimensions;
use ruffle_render::backend::null::NullRenderer;
use ruffle_render::quality::StageQuality;
use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};
use url::Url;

const STAGE_W: u32 = 958;
const STAGE_H: u32 = 550;

enum EngineEvent {
    TaskPoll(async_task::Runnable),
    FlashCall(FlashCallRequest),
    Stop,
}

struct Executor {
    sender: Sender<EngineEvent>,
}

impl<E: std::error::Error + 'static> FutureSpawner<E> for Executor {
    fn spawn(&self, future: OwnedFuture<(), E>) {
        let future = async {
            let _ = future.await;
        };
        let sender = self.sender.clone();
        let schedule = move |task| {
            let _ = sender.send(EngineEvent::TaskPoll(task));
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

pub struct HeadlessEngine {
    running: Arc<AtomicBool>,
    sender: Sender<EngineEvent>,
}

impl HeadlessEngine {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
        let _ = self.sender.send(EngineEvent::Stop);
    }

    /// Spawns a Ruffle `NullRenderer` thread, loads `grimlite.swf` -> AQW, and logs in automatically.
    pub fn spawn(
        bot: Arc<Bot>,
        username: String,
        password: String,
        server_name: String,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let (sender, events) = mpsc::channel::<EngineEvent>();

        let run_flag = running.clone();
        let engine_sender = sender.clone();
        let bot_engine = bot.clone();

        let _ = std::thread::Builder::new()
            .name("grimlite-null-ruffle".into())
            .spawn(move || {
                ruffle_core::set_show_all_apis(true);
                ruffle_core::set_forgiving_removal(true);
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                let _guard = rt.enter();

                bot_engine.set_game_loaded(false);
                bot_engine.state.reset_session();
                bot_engine
                    .logs
                    .push_debug("[Headless] Starting Ruffle NullRenderer (0% GPU)...");

                let swf_path = ensure_host_swf();
                let Ok(movie_url) = Url::from_file_path(&swf_path) else {
                    bot_engine
                        .logs
                        .push_debug("[Headless] Invalid host SWF path");
                    run_flag.store(false, Ordering::Release);
                    return;
                };

                let content = PlayingContent::DirectFile(ContentDescriptor {
                    url: movie_url.clone(),
                    root_content_path: None,
                });

                let inner_nav = ExternalNavigatorBackend::new(
                    movie_url.clone(),
                    None,
                    None,
                    Executor {
                        sender: engine_sender.clone(),
                    },
                    None,
                    false,
                    HashSet::new(),
                    SocketMode::Allow,
                    Rc::new(content),
                    NoDialogs,
                );
                let navigator = CaptureNavigator::new(inner_nav, Some(bot_engine.capture.clone()));

                let renderer = NullRenderer::new(ViewportDimensions {
                    width: STAGE_W,
                    height: STAGE_H,
                    scale_factor: 1.0,
                });

                let storage_dir = data_dir().join("shared_objects");
                let _ = std::fs::create_dir_all(&storage_dir);

                let player = PlayerBuilder::new()
                    .with_navigator(navigator)
                    .with_renderer(renderer)
                    .with_storage(Box::new(DiskStorageBackend::new(storage_dir)))
                    .with_ui(NullUiBackend::new())
                    .with_external_interface(Box::new(GrimliteExternalInterface::new(
                        bot_engine.clone(),
                    )))
                    .with_autoplay(true)
                    .with_letterbox(Letterbox::On)
                    .with_quality(StageQuality::Low)
                    .with_load_behavior(LoadBehavior::Streaming)
                    .with_player_version(Some(9))
                    .with_max_execution_duration(Duration::MAX)
                    .build();

                // Connect Rust -> Flash marshalling onto this engine thread
                let dispatch_sender = engine_sender.clone();
                let flash_host = Arc::new(RuffleFlashHost::new(
                    std::thread::current().id(),
                    move |req| {
                        dispatch_sender
                            .send(EngineEvent::FlashCall(req))
                            .map_err(|_| "headless engine stopped".to_string())
                    },
                ));
                bot_engine.set_host(flash_host);

                if let Ok(mut guard) = player.lock() {
                    guard.set_mouse_in_stage(false);
                    guard.fetch_root_movie(movie_url.to_string(), vec![], Box::new(|_| {}));
                }

                // Spawn auto-login helper thread (waits for Game.swf -> login -> clickServer -> 2 FPS)
                spawn_auto_login(
                    bot_engine.clone(),
                    run_flag.clone(),
                    username,
                    password,
                    server_name,
                );

                // Main windowless tick loop
                let mut last_tick = Instant::now();
                while run_flag.load(Ordering::Acquire) {
                    let now = Instant::now();
                    let next_frame = if let Ok(mut p) = player.lock() {
                        p.tick(FloatDuration::from_std(now.duration_since(last_tick)));
                        now + p.time_til_next_frame()
                    } else {
                        now + Duration::from_millis(50)
                    };
                    last_tick = now;

                    loop {
                        let wait = next_frame
                            .saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(50));
                        match events.recv_timeout(wait) {
                            Ok(EngineEvent::TaskPoll(task)) => {
                                task.run();
                            }
                            Ok(EngineEvent::FlashCall(req)) => {
                                if let Ok(mut p) = player.lock() {
                                    answer_flash_call(Some(&mut p), req);
                                } else {
                                    answer_flash_call(None, req);
                                }
                            }
                            Ok(EngineEvent::Stop) => {
                                run_flag.store(false, Ordering::Release);
                                break;
                            }
                            Err(RecvTimeoutError::Timeout) => {
                                if Instant::now() >= next_frame || !run_flag.load(Ordering::Acquire)
                                {
                                    break;
                                }
                            }
                            Err(RecvTimeoutError::Disconnected) => {
                                run_flag.store(false, Ordering::Release);
                                break;
                            }
                        }
                    }
                }

                bot_engine.stop_bot();
                bot_engine.set_game_loaded(false);
                bot_engine
                    .logs
                    .push_debug("[Headless] NullRenderer stopped");
            });

        Self { running, sender }
    }
}

fn spawn_auto_login(
    bot: Arc<Bot>,
    running: Arc<AtomicBool>,
    username: String,
    password: String,
    server_name: String,
) {
    let _ = std::thread::Builder::new()
        .name("grimlite-headless-login".into())
        .spawn(move || {
            // 1. Wait for Game.swf to finish loading
            let start = Instant::now();
            while running.load(Ordering::Acquire) && !bot.is_game_loaded() {
                if start.elapsed() > Duration::from_secs(45) {
                    bot.logs
                        .push_debug("[Headless] Timeout waiting for Game.swf");
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            if !running.load(Ordering::Acquire) {
                return;
            }
            std::thread::sleep(Duration::from_millis(800));

            let Some(flash) = bot.flash() else {
                return;
            };

            // 2. Invoke AQW's native game.login(user, pass)
            bot.logs
                .push_debug(format!("[Headless] Logging in as `{username}`..."));
            let _ = flash.call_bool(
                "login",
                &[FlashArg::from(username.as_str()), FlashArg::from(password.as_str())],
            );

            // 3. Wait for server list and click selected server
            let srv_start = Instant::now();
            let mut clicked = false;
            while running.load(Ordering::Acquire) && srv_start.elapsed() < Duration::from_secs(30) {
                if flash.call_bool("isLoggedIn", &[]).unwrap_or(false) {
                    break;
                }
                if !clicked
                    && flash
                        .call_bool("clickServer", &[FlashArg::from(server_name.as_str())])
                        .unwrap_or(false)
                {
                    clicked = true;
                    bot.logs
                        .push_debug(format!("[Headless] Connecting to server `{server_name}`..."));
                }
                std::thread::sleep(Duration::from_millis(400));
            }

            // 4. Once in-game, drop stage FPS to 2 FPS (Turu headless optimization)
            let join_start = Instant::now();
            while running.load(Ordering::Acquire) && join_start.elapsed() < Duration::from_secs(30)
            {
                if flash.call_bool("isLoggedIn", &[]).unwrap_or(false) {
                    let mut opts = bot.options.get();
                    opts.fps = 2;
                    opts.lag_killer = true;
                    opts.disable_animations = true;
                    opts.skip_cutscenes = true;
                    bot.apply_options(opts);
                    bot.logs.push_debug(format!(
                        "[Headless] Connected to `{server_name}` (2 FPS low-CPU mode active)"
                    ));
                    return;
                }
                std::thread::sleep(Duration::from_millis(400));
            }
        });
}

fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Grimlite-Ri")
}

fn ensure_host_swf() -> PathBuf {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("grimlite.swf");
    let up_to_date = std::fs::read(&path).is_ok_and(|b| b == grimlite_bot::HOST_SWF);
    if !up_to_date {
        let _ = std::fs::write(&path, grimlite_bot::HOST_SWF);
    }
    path
}
