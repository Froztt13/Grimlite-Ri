//! Grimlite-Li game options (`OptionsManager`) enforced safely on a background worker thread.

use crate::gbot::BotConfig;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Target AQW stage dimensions for cached Lag Killer wallpapers.
pub const LAG_KILLER_WIDTH: u32 = 960;
pub const LAG_KILLER_HEIGHT: u32 = 550;

static CUSTOM_CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Overrides the base cache directory on platforms where `dirs::cache_dir()` is unavailable (Android).
pub fn set_cache_dir(dir: PathBuf) {
    let _ = CUSTOM_CACHE_DIR.set(dir);
}

/// Persistent app data/cache directory (`Grimlite-Ri`) across Desktop and Android.
pub fn app_data_dir() -> PathBuf {
    CUSTOM_CACHE_DIR
        .get()
        .cloned()
        .or_else(dirs::cache_dir)
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Grimlite-Ri")
}

/// Persistent cache path for the cropped & resized Lag Killer wallpaper.
pub fn lag_killer_cache_path() -> PathBuf {
    app_data_dir().join("lag_killer_wallpaper.jpg")
}

/// Center-crops & resizes `src` to `960×550` (AQW stage aspect ratio) and saves it to the
/// persistent app cache so it loads instantly on subsequent launches and across tabs.
pub fn prepare_lag_killer_wallpaper(src: &Path) -> Result<PathBuf, String> {
    let img = image::open(src).map_err(|e| format!("Failed to decode image: {e}"))?;
    let fitted = img.resize_to_fill(
        LAG_KILLER_WIDTH,
        LAG_KILLER_HEIGHT,
        image::imageops::FilterType::Lanczos3,
    );
    let dest = lag_killer_cache_path();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create cache dir: {e}"))?;
    }
    fitted
        .to_rgb8()
        .save_with_format(&dest, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to save cached wallpaper: {e}"))?;
    Ok(dest)
}

/// Deletes the cached Lag Killer wallpaper from disk if present.
pub fn clear_lag_killer_wallpaper_cache() {
    let _ = std::fs::remove_file(lag_killer_cache_path());
}

/// Canonical AQW server list (shared across Bot Manager Options & Headless login).
pub const AQW_SERVERS: &[&str] = &[
    "Artix",
    "Gravelyn",
    "Yorumi",
    "Espada",
    "Twilly",
    "Twig",
    "Sepulchure",
    "Safiria",
    "Galanoth",
    "Alteon",
    "Swordhaven (EU)",
    "Sir Ver",
    "Yokai (SEA)",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BotOptions {
    pub infinite_range: bool,
    pub provoke_monsters: bool,
    pub enemy_magnet: bool,
    pub lag_killer: bool,
    pub lag_killer_image: Option<String>,
    pub lag_killer_image_rev: u64,
    pub hide_players: bool,
    pub disable_animations: bool,
    pub skip_cutscenes: bool,
    pub walk_speed_enabled: bool,
    pub walk_speed: i32,
    pub anti_counter: bool,
    pub follow_enabled: bool,
    pub follow_player: String,
    pub fps: i32,
}

impl Default for BotOptions {
    fn default() -> Self {
        let cached_wallpaper = {
            let p = lag_killer_cache_path();
            p.is_file().then(|| p.to_string_lossy().to_string())
        };
        Self {
            infinite_range: false,
            provoke_monsters: false,
            enemy_magnet: false,
            lag_killer: false,
            lag_killer_image: cached_wallpaper,
            lag_killer_image_rev: 0,
            hide_players: false,
            disable_animations: false,
            skip_cutscenes: false,
            walk_speed_enabled: false,
            walk_speed: 8,
            anti_counter: true,
            follow_enabled: false,
            follow_player: String::new(),
            fps: 30,
        }
    }
}

/// Unified single-source-of-truth Game Options engine shared by the standalone Game Options
/// window, Bot Manager's Options tab, Hotkeys, and `BotRunner`.
#[derive(Default)]
pub struct OptionsStore {
    inner: RwLock<BotOptions>,
    live_without_starting: AtomicBool,
    pre_bot_hide_players: AtomicBool,
    bot_enabled_hide_players: AtomicBool,
}

impl OptionsStore {
    pub fn get(&self) -> BotOptions {
        self.inner.read().clone()
    }

    pub fn set(&self, opts: BotOptions) {
        *self.inner.write() = opts;
    }

    pub fn update(&self, f: impl FnOnce(&mut BotOptions)) -> BotOptions {
        let mut guard = self.inner.write();
        f(&mut guard);
        guard.clone()
    }

    pub fn is_live_without_starting(&self) -> bool {
        self.live_without_starting.load(Ordering::Acquire)
    }

    pub fn set_live_without_starting(&self, enabled: bool) {
        self.live_without_starting.store(enabled, Ordering::Release);
    }

    /// Returns `true` if any active runtime gameplay toggle is enabled (for UI badge dot).
    pub fn any_gameplay_active(&self) -> bool {
        let o = self.inner.read();
        o.infinite_range
            || o.provoke_monsters
            || o.enemy_magnet
            || o.lag_killer
            || o.hide_players
            || o.disable_animations
            || o.skip_cutscenes
            || o.walk_speed_enabled
            || o.follow_enabled
    }

    /// Activates all options configured in a `.gbot` when `BotRunner` starts.
    /// Remembers whether `hide_players` was already enabled in-game before starting so stopping
    /// the bot does not forcibly disable the user's pre-existing in-game Hide Players preference.
    pub fn activate_for_bot(&self, cfg: &BotConfig) -> BotOptions {
        let mut guard = self.inner.write();
        let was_hidden = guard.hide_players;
        self.pre_bot_hide_players
            .store(was_hidden, Ordering::Release);
        self.bot_enabled_hide_players
            .store(cfg.hide_players, Ordering::Release);

        guard.infinite_range |= cfg.infinite_attack_range;
        guard.provoke_monsters |= cfg.provoke_monsters;
        guard.enemy_magnet |= cfg.enemy_magnet;
        guard.lag_killer |= cfg.lag_killer;
        if cfg.hide_players {
            guard.hide_players = true;
        }
        guard.skip_cutscenes |= cfg.skip_cutscenes;
        guard.disable_animations |= cfg.disable_animations;
        guard.anti_counter |= cfg.anti_counter;
        if cfg.walk_speed_enabled {
            guard.walk_speed_enabled = true;
            guard.walk_speed = cfg.walk_speed.clamp(1, 99);
        }
        if cfg.follow_check && !cfg.follow_name.trim().is_empty() {
            guard.follow_enabled = true;
            guard.follow_player = cfg.follow_name.clone();
        }
        guard.clone()
    }

    /// Mirrors Bot Manager's Options tab directly into the live runtime options (used while the
    /// bot is running or "Enable options without starting" is checked).
    pub fn sync_from_bot_config(&self, cfg: &BotConfig) -> BotOptions {
        let mut guard = self.inner.write();
        guard.infinite_range = cfg.infinite_attack_range;
        guard.provoke_monsters = cfg.provoke_monsters;
        guard.enemy_magnet = cfg.enemy_magnet;
        guard.lag_killer = cfg.lag_killer;
        guard.hide_players = cfg.hide_players;
        guard.skip_cutscenes = cfg.skip_cutscenes;
        guard.disable_animations = cfg.disable_animations;
        guard.walk_speed_enabled = cfg.walk_speed_enabled;
        guard.walk_speed = if cfg.walk_speed_enabled {
            cfg.walk_speed.clamp(1, 99)
        } else {
            8
        };
        guard.anti_counter = cfg.anti_counter;
        guard.follow_enabled = cfg.follow_check && !cfg.follow_name.trim().is_empty();
        guard.follow_player = cfg.follow_name.clone();
        guard.clone()
    }

    /// Disables all active runtime gameplay options when the bot stops (or when live preview is
    /// turned off). Preserves persistent preferences (`fps`, `lag_killer_image`, `anti_counter`)
    /// and respects pre-existing in-game `hide_players` if the bot did not enable it.
    pub fn stop_active_options(&self) -> BotOptions {
        self.live_without_starting.store(false, Ordering::Release);
        let pre_hide = self.pre_bot_hide_players.swap(false, Ordering::AcqRel);
        let bot_hide = self.bot_enabled_hide_players.swap(false, Ordering::AcqRel);

        let mut guard = self.inner.write();
        guard.infinite_range = false;
        guard.provoke_monsters = false;
        guard.enemy_magnet = false;
        guard.lag_killer = false;
        guard.disable_animations = false;
        guard.skip_cutscenes = false;
        guard.walk_speed_enabled = false;
        guard.walk_speed = 8;
        guard.follow_enabled = false;
        if bot_hide {
            guard.hide_players = false;
        } else {
            guard.hide_players = pre_hide;
        }
        guard.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_active_options_turns_off_bot_toggles_and_preserves_fps() {
        let store = OptionsStore::default();
        store.update(|o| {
            o.fps = 45;
            o.hide_players = false;
        });

        let mut cfg = BotConfig::default();
        cfg.infinite_attack_range = true;
        cfg.provoke_monsters = true;
        cfg.enemy_magnet = true;
        cfg.lag_killer = true;
        cfg.hide_players = true;
        cfg.disable_animations = true;
        cfg.skip_cutscenes = true;
        cfg.walk_speed_enabled = true;
        cfg.walk_speed = 20;
        cfg.follow_check = true;
        cfg.follow_name = "Master".to_string();

        let active = store.activate_for_bot(&cfg);
        assert!(active.lag_killer);
        assert!(active.provoke_monsters);
        assert!(active.hide_players);
        assert_eq!(active.walk_speed, 20);
        assert!(store.any_gameplay_active());

        let stopped = store.stop_active_options();
        assert!(!stopped.infinite_range);
        assert!(!stopped.provoke_monsters);
        assert!(!stopped.enemy_magnet);
        assert!(!stopped.lag_killer);
        assert!(!stopped.hide_players);
        assert!(!stopped.disable_animations);
        assert!(!stopped.skip_cutscenes);
        assert!(!stopped.walk_speed_enabled);
        assert_eq!(stopped.walk_speed, 8);
        assert!(!stopped.follow_enabled);
        assert_eq!(stopped.fps, 45);
        assert!(!store.any_gameplay_active());
    }

    #[test]
    fn stop_active_options_preserves_preexisting_ingame_hide_players_when_bot_did_not_enable_it() {
        let store = OptionsStore::default();
        store.update(|o| o.hide_players = true);

        let mut cfg = BotConfig::default();
        cfg.lag_killer = true;
        cfg.hide_players = false;

        store.activate_for_bot(&cfg);
        let stopped = store.stop_active_options();
        assert!(!stopped.lag_killer);
        assert!(stopped.hide_players);
    }
}
