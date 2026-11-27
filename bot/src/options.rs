//! Grimlite-Li game options (`OptionsManager`) enforced safely on a background worker thread.

use parking_lot::RwLock;

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
        Self {
            infinite_range: false,
            provoke_monsters: false,
            enemy_magnet: false,
            lag_killer: false,
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
