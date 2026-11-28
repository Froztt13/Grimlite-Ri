//! Grimlite-Li game options (`OptionsManager`) enforced safely on a background worker thread.

use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

#[derive(Default)]
pub struct OptionsStore {
    inner: RwLock<BotOptions>,
}

impl OptionsStore {
    pub fn get(&self) -> BotOptions {
        self.inner.read().clone()
    }

    pub fn set(&self, opts: BotOptions) {
        *self.inner.write() = opts;
    }
}
