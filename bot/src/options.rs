//! Grimlite-Li game options (`OptionsManager`) enforced safely on a background worker thread.

use parking_lot::RwLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BotOptions {
    pub infinite_range: bool,
    pub provoke_monsters: bool,
    pub enemy_magnet: bool,
    pub lag_killer: bool,
    pub disable_animations: bool,
    pub skip_cutscenes: bool,
    pub walk_speed: i32,
    pub fps: i32,
}

impl Default for BotOptions {
    fn default() -> Self {
        Self {
            infinite_range: false,
            provoke_monsters: false,
            enemy_magnet: false,
            lag_killer: false,
            disable_animations: false,
            skip_cutscenes: false,
            walk_speed: 8,
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
