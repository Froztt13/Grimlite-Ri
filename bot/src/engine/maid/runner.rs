//! Maid Remake runtime engine: follow/chase, combat rotation, safe skill, stop-attack + buff,
//! attack priority, class preset special combos, Ultra Boss extras (Auto Potion, Enrage, Gramiel),
//! and taunt cycle execution.

use super::config::{
    LockedMapEntry, MaidConfig, MaidPreset, UltraBossMode, parse_locked_map_entries,
};
use crate::Bot;
use crate::flash::Flash;
use crate::host::FlashArg;
use parking_lot::RwLock;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const AUTO_POTIONS: &[&str] = &[
    "Felicitous Philtre",
    "Potent Malice Potion",
    "Potent Honor Potion",
];

/// Parsed `tc;<cycle>;<monster>;<second>;<order>` taunt cycle specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TauntCycleSpec {
    pub cycle: u32,
    pub monster: String,
    pub second: u64,
    pub order: u32,
}

impl TauntCycleSpec {
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let rest = trimmed
            .strip_prefix("tc;")
            .or_else(|| trimmed.strip_prefix("TC;"))?;
        let parts: Vec<&str> = rest.split(';').map(str::trim).collect();
        if parts.len() < 4 {
            return None;
        }
        let cycle = parts[0].parse::<u32>().ok()?.max(1);
        let monster = parts[1].to_string();
        let second = parts[2].parse::<u64>().ok()?;
        let order = parts[3].parse::<u32>().ok()?.max(1);
        Some(Self {
            cycle,
            monster,
            second,
            order,
        })
    }
}

/// Lightweight snapshot read by the UI every frame without holding locks.
#[derive(Clone, Debug, Default)]
pub struct MaidRuntimeSnapshot {
    pub running: bool,
    pub unfollow: bool,
    pub stop_attack: bool,
    pub on_pause: bool,
    pub stop_attack_elapsed_secs: Option<u64>,
    pub goto_fail_count: u32,
    pub taunt_count: u32,
    pub loaded_config_name: String,
}

pub struct MaidEngine {
    config: RwLock<MaidConfig>,
    loaded_config_name: RwLock<String>,

    running: AtomicBool,
    run_id: AtomicU64,
    unfollow: AtomicBool,
    stop_attack: AtomicBool,
    on_pause: AtomicBool,
    force_skill: AtomicBool,
    balance_crystal: AtomicBool,

    stop_attack_since: RwLock<Option<Instant>>,
    lock_target: RwLock<Option<String>>,

    alt_map_index: AtomicUsize,
    goto_fail_count: AtomicU32,
    taunt_count: AtomicU32,
    taunt_active: AtomicBool,
    counter_paused: AtomicBool,
}

impl Default for MaidEngine {
    fn default() -> Self {
        Self {
            config: RwLock::new(MaidConfig::default()),
            loaded_config_name: RwLock::new(String::new()),
            running: AtomicBool::new(false),
            run_id: AtomicU64::new(0),
            unfollow: AtomicBool::new(false),
            stop_attack: AtomicBool::new(false),
            on_pause: AtomicBool::new(false),
            force_skill: AtomicBool::new(false),
            balance_crystal: AtomicBool::new(false),
            stop_attack_since: RwLock::new(None),
            lock_target: RwLock::new(None),
            alt_map_index: AtomicUsize::new(0),
            goto_fail_count: AtomicU32::new(0),
            taunt_count: AtomicU32::new(0),
            taunt_active: AtomicBool::new(false),
            counter_paused: AtomicBool::new(false),
        }
    }
}

impl MaidEngine {
    pub fn config(&self) -> MaidConfig {
        self.config.read().clone()
    }

    pub fn set_config(&self, cfg: MaidConfig) {
        *self.config.write() = cfg;
    }

    pub fn update_config(&self, f: impl FnOnce(&mut MaidConfig)) {
        let mut guard = self.config.write();
        f(&mut guard);
    }

    pub fn apply_preset(&self, preset: MaidPreset) {
        let mut guard = self.config.write();
        preset.apply_to(&mut guard);
    }

    pub fn load_json_file(&self, path: &Path) -> Result<String, String> {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
        let cfg = MaidConfig::from_json(&raw)?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("maid.json")
            .to_string();
        *self.config.write() = cfg;
        *self.loaded_config_name.write() = name.clone();
        Ok(name)
    }

    pub fn save_json_file(&self, path: &Path) -> Result<String, String> {
        let cfg = self.config.read().clone();
        let json = cfg.to_json_pretty()?;
        std::fs::write(path, json).map_err(|e| format!("write error: {e}"))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("maid.json")
            .to_string();
        *self.loaded_config_name.write() = name.clone();
        Ok(name)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn is_unfollow(&self) -> bool {
        self.unfollow.load(Ordering::Acquire)
    }

    pub fn is_stop_attack(&self) -> bool {
        self.stop_attack.load(Ordering::Acquire)
    }

    pub fn is_on_pause(&self) -> bool {
        self.on_pause.load(Ordering::Acquire)
    }

    pub fn set_unfollow(&self, on: bool) {
        self.unfollow.store(on, Ordering::Release);
    }

    pub fn toggle_unfollow(&self) {
        let next = !self.unfollow.load(Ordering::Acquire);
        self.unfollow.store(next, Ordering::Release);
    }

    pub fn set_stop_attack(&self, on: bool, bot: Option<&Arc<Bot>>) {
        let prev = self.stop_attack.swap(on, Ordering::AcqRel);
        if on {
            if !prev {
                *self.stop_attack_since.write() = Some(Instant::now());
            }
            if let Some(b) = bot {
                b.spawn_flash_action("cancelTarget", vec![]);
            }
        } else {
            *self.stop_attack_since.write() = None;
            self.counter_paused.store(false, Ordering::Release);
        }
    }

    pub fn toggle_stop_attack(&self, bot: &Arc<Bot>) {
        let next = !self.stop_attack.load(Ordering::Acquire);
        self.set_stop_attack(next, Some(bot));
    }

    pub fn pause_follow(&self) {
        self.on_pause.store(true, Ordering::Release);
    }

    pub fn resume_follow(&self) {
        self.on_pause.store(false, Ordering::Release);
    }

    pub fn trigger_force_skill(&self) {
        self.force_skill.store(true, Ordering::Release);
    }

    pub fn set_lock_target(&self, target: Option<String>) {
        *self.lock_target.write() = target;
    }

    pub fn reset_specials(&self) {
        self.force_skill.store(false, Ordering::Release);
        self.balance_crystal.store(false, Ordering::Release);
        self.taunt_count.store(0, Ordering::Release);
        self.taunt_active.store(false, Ordering::Release);
        *self.lock_target.write() = None;
    }

    /// Triggers Maid's anti-counter pause (with 6s auto-resume safety timer, matching `MaidRemake.cs`).
    pub fn trigger_counter_pause(self: &Arc<Self>, bot: &Arc<Bot>, delay_ms: u64) {
        if self.counter_paused.swap(true, Ordering::AcqRel) {
            return;
        }
        let engine = Arc::clone(self);
        let bot = Arc::clone(bot);
        let _ = std::thread::Builder::new()
            .name("maid-anticounter".into())
            .spawn(move || {
                if delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(delay_ms));
                }
                if !engine.is_running() || !engine.counter_paused.load(Ordering::Acquire) {
                    return;
                }
                engine.set_stop_attack(true, Some(&bot));
                bot.logs
                    .push_debug("[Maid] Counter attack detected -> Stop Attack");
                // Safety auto-resume after 6s if aura-- packet was missed
                for _ in 0..60 {
                    std::thread::sleep(Duration::from_millis(100));
                    if !engine.is_running() || !engine.counter_paused.load(Ordering::Acquire) {
                        return;
                    }
                }
                if engine.counter_paused.swap(false, Ordering::AcqRel) {
                    engine.set_stop_attack(false, None);
                    bot.logs
                        .push_debug("[Maid] Counter attack timeout -> Resumed");
                }
            });
    }

    /// Resumes from Maid's anti-counter pause on `Counter Attack` `aura--`.
    pub fn clear_counter_pause(&self, bot: &Arc<Bot>) {
        if self.counter_paused.swap(false, Ordering::AcqRel) {
            self.set_stop_attack(false, None);
            bot.logs
                .push_debug("[Maid] Counter attack ended -> Resumed");
        }
    }

    /// Temporarily locks target to `"Gramiel"` for 4 seconds on `"Gramiel attempts to drain"`.
    pub fn trigger_gramiel_drain_lock(self: &Arc<Self>, bot: &Arc<Bot>) {
        self.set_lock_target(Some("Gramiel".to_string()));
        bot.logs
            .push_debug("[Maid] Gramiel drain detected -> targeting Gramiel (4s)");
        let engine = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("maid-gramiel-lock".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_secs(4));
                engine.set_lock_target(None);
            });
    }

    /// Executes a `tc;<cycle>;<monster>;<second>;<order>` taunt cycle trigger.
    pub fn trigger_taunt_cycle(self: &Arc<Self>, bot: &Arc<Bot>, spec: TauntCycleSpec) {
        let count = self.taunt_count.fetch_add(1, Ordering::AcqRel) + 1;
        let slot = ((count - 1) % spec.cycle) + 1;
        if slot != spec.order {
            return;
        }
        if self.taunt_active.swap(true, Ordering::AcqRel) {
            return;
        }
        let engine = Arc::clone(self);
        let bot = Arc::clone(bot);
        let _ = std::thread::Builder::new()
            .name("maid-taunt-cycle".into())
            .spawn(move || {
                let skill_idx = engine.config().special_act.to_string();
                if spec.second > 0 {
                    std::thread::sleep(Duration::from_secs(spec.second));
                }
                if engine.is_running()
                    && let Some(flash) = bot.flash()
                {
                    if !spec.monster.is_empty() {
                        attack_target_spec(&flash, &spec.monster);
                    }
                    let _ = flash.call_str("ForceUseSkill", &[FlashArg::from(skill_idx.as_str())]);
                    bot.logs.push_debug(format!(
                        "[Maid] TauntCycle #{count} (order {}/{}) -> skill {skill_idx} on {}",
                        spec.order, spec.cycle, spec.monster
                    ));
                }
                engine.taunt_active.store(false, Ordering::Release);
            });
    }

    /// Returns the next alternative map from `LockedZoneHandlerMaps` in round-robin order.
    pub fn next_locked_map(&self) -> Option<LockedMapEntry> {
        let maps = parse_locked_map_entries(&self.config.read().locked_zone_handler_maps);
        if maps.is_empty() {
            return None;
        }
        let idx = self.alt_map_index.fetch_add(1, Ordering::AcqRel) % maps.len();
        Some(maps[idx].clone())
    }

    pub fn snapshot(&self) -> MaidRuntimeSnapshot {
        let elapsed = self
            .stop_attack_since
            .read()
            .map(|t| t.elapsed().as_secs());
        MaidRuntimeSnapshot {
            running: self.is_running(),
            unfollow: self.is_unfollow(),
            stop_attack: self.is_stop_attack(),
            on_pause: self.is_on_pause(),
            stop_attack_elapsed_secs: elapsed,
            goto_fail_count: self.goto_fail_count.load(Ordering::Acquire),
            taunt_count: self.taunt_count.load(Ordering::Acquire),
            loaded_config_name: self.loaded_config_name.read().clone(),
        }
    }

    pub fn start(self: &Arc<Self>, bot: Arc<Bot>) {
        if self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let my_run_id = self.run_id.fetch_add(1, Ordering::AcqRel) + 1;
        self.reset_specials();
        self.goto_fail_count.store(0, Ordering::Release);
        self.on_pause.store(false, Ordering::Release);
        self.counter_paused.store(false, Ordering::Release);

        // Always enable Infinite Range once directly via Flash when Maid starts.
        bot.spawn_flash_action("setInfiniteRange", vec![FlashArg::from(true)]);

        bot.logs.push_debug("[Maid] Started");
        let engine = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("grimlite-maid".into())
            .spawn(move || engine.run_loop(bot, my_run_id));
    }

    pub fn stop(&self, bot: &Arc<Bot>) {
        if !self.running.swap(false, Ordering::AcqRel) {
            return;
        }
        self.run_id.fetch_add(1, Ordering::AcqRel);
        self.reset_specials();
        self.set_stop_attack(false, None);
        bot.spawn_flash_action("cancelTarget", vec![]);
        bot.logs.push_debug("[Maid] Stopped");
    }

    pub fn toggle(self: &Arc<Self>, bot: Arc<Bot>) {
        if self.is_running() {
            self.stop(&bot);
        } else {
            self.start(bot);
        }
    }

    fn run_loop(self: Arc<Self>, bot: Arc<Bot>, my_run_id: u64) {
        let mut skill_idx: usize = 0;
        let mut last_follow_check = Instant::now() - Duration::from_secs(10);
        let mut last_enrage_check = Instant::now() - Duration::from_secs(10);
        let mut last_potion_check = Instant::now() - Duration::from_secs(10);

        while self.running.load(Ordering::Acquire) && self.run_id.load(Ordering::Acquire) == my_run_id
        {
            let cfg = self.config();
            let snap = bot.player_snapshot();

            if !snap.logged_in {
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }

            let Some(flash) = bot.flash() else {
                std::thread::sleep(Duration::from_millis(300));
                continue;
            };

            // Wait while map is loading
            if flash.call_bool("isMapLoading", &[]).unwrap_or(false) {
                std::thread::sleep(Duration::from_millis(300));
                continue;
            }

            // 1. Follow target player (every ~1.5s when not unfollowed / paused)
            let target_player = cfg.target.trim().to_string();
            let my_name = snap.username.trim().to_string();
            if !target_player.is_empty()
                && !target_player.eq_ignore_ascii_case(&my_name)
                && !self.is_unfollow()
                && !self.is_on_pause()
                && last_follow_check.elapsed() >= Duration::from_millis(1500)
            {
                last_follow_check = Instant::now();
                let in_map = is_player_in_map(&flash, &target_player);
                if !in_map {
                    let fails = self.goto_fail_count.fetch_add(1, Ordering::AcqRel) + 1;
                    let limit = cfg.stop_failed_goto_limit.max(1);
                    if cfg.stop_failed_goto && fails >= limit {
                        bot.logs.push_debug(format!(
                            "[Maid] Stopping: failed to goto `{target_player}` {fails}/{limit} times"
                        ));
                        self.stop(&bot);
                        break;
                    }
                    // Immediately disengage combat & jump to current cell to drop aggro before /goto
                    let _ = flash.call_str("CancelAutoAttack", &[]);
                    let _ = flash.call_str("CancelTarget", &[]);
                    if !snap.cell.is_empty() {
                        let pad = if snap.pad.is_empty() {
                            "Left"
                        } else {
                            snap.pad.as_str()
                        };
                        let _ = flash.call_str(
                            "moveToCell",
                            &[FlashArg::from(snap.cell.as_str()), FlashArg::from(pad)],
                        );
                        std::thread::sleep(Duration::from_millis(250));
                    }
                    let pkt = format!("%xt%zm%cmd%1%goto%{target_player}%");
                    let _ = bot.capture.send_to_server(&pkt);
                    std::thread::sleep(Duration::from_millis(600));
                    continue;
                } else {
                    self.goto_fail_count.store(0, Ordering::Release);
                }
            }

            // 2. Equip Scroll of Enrage for Ultra Boss taunt modes (every 5s)
            let ultra = cfg.ultra_boss_mode();
            if ultra.uses_enrage() && last_enrage_check.elapsed() >= Duration::from_secs(5) {
                last_enrage_check = Instant::now();
                if flash
                    .call_bool("hasItem", &[FlashArg::from("Scroll of Enrage"), FlashArg::from(1)])
                    .unwrap_or(false)
                    && !flash
                        .call_bool("isEquipped", &[FlashArg::from("Scroll of Enrage")])
                        .unwrap_or(false)
                {
                    let _ = flash.call_str("EquipPotion", &[FlashArg::from("Scroll of Enrage")]);
                }
            }

            // 3. Auto Potion mode (every 3s)
            if ultra == UltraBossMode::AutoPotion
                && last_potion_check.elapsed() >= Duration::from_secs(3)
            {
                last_potion_check = Instant::now();
                for &pot in AUTO_POTIONS {
                    if flash
                        .call_bool("hasItem", &[FlashArg::from(pot), FlashArg::from(1)])
                        .unwrap_or(false)
                    {
                        if !flash
                            .call_bool("isEquipped", &[FlashArg::from(pot)])
                            .unwrap_or(false)
                        {
                            let _ = flash.call_str("EquipPotion", &[FlashArg::from(pot)]);
                        }
                        if is_skill_ready(&flash, "5") {
                            let _ = flash.call_str("UseSkill", &[FlashArg::from("5")]);
                        }
                        break;
                    }
                }
            }

            // 4. Stop Attack mode (rest + optional buff skills)
            if self.is_stop_attack() {
                if flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                    let _ = flash.call_bool("cancelTarget", &[]);
                }
                if cfg.buff_stop_attack {
                    for s in cfg.parsed_buff_skills() {
                        if !self.is_running() || !self.is_stop_attack() {
                            break;
                        }
                        if is_skill_ready(&flash, &s) {
                            let _ = flash.call_str("UseSkill", &[FlashArg::from(s.as_str())]);
                            std::thread::sleep(Duration::from_millis(cfg.skill_delay.max(100) as u64));
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }

            // 5. Force skill from Special Anims trigger
            if self.force_skill.swap(false, Ordering::AcqRel) {
                let act = cfg.special_act.to_string();
                let _ = flash.call_str("ForceUseSkill", &[FlashArg::from(act.as_str())]);
                std::thread::sleep(Duration::from_millis(cfg.skill_delay.max(100) as u64));
                continue;
            }

            // 6. Safe Skill (Heal when HP% <= SafeSkillHP)
            if cfg.safe_skill && snap.max_hp > 0 {
                let hp_pct = (snap.hp.max(0).saturating_mul(100)) / snap.max_hp;
                if hp_pct <= cfg.safe_skill_hp as i32 {
                    for s in cfg.parsed_safe_skills() {
                        if !self.is_running() || self.is_stop_attack() {
                            break;
                        }
                        if is_skill_ready(&flash, &s) {
                            let _ = flash.call_str("UseSkill", &[FlashArg::from(s.as_str())]);
                            std::thread::sleep(Duration::from_millis(cfg.skill_delay.max(100) as u64));
                        }
                    }
                }
            }

            // 7. Target selection: LockTarget > Gramiel Crystal Balance > Attack Priority > "*"
            let locked = self.lock_target.read().clone();
            if let Some(ref lock_mon) = locked {
                attack_target_spec(&flash, lock_mon);
            } else if ultra.is_gramiel() {
                self.select_gramiel_target(&flash, ultra);
            } else if cfg.attack_priority {
                let mut targeted = false;
                for mon in cfg.parsed_attack_priority() {
                    if is_monster_available(&flash, &mon) {
                        attack_target_spec(&flash, &mon);
                        targeted = true;
                        break;
                    }
                }
                if !targeted && !flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                    let _ = flash.call_str("AttackMonster", &[FlashArg::from("*")]);
                }
            } else if !flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                let _ = flash.call_str("AttackMonster", &[FlashArg::from("*")]);
            }

            // Never spam offensive skills if no monster target is active
            if !flash.call_bool("hasTarget", &[]).unwrap_or(false) {
                std::thread::sleep(Duration::from_millis(cfg.skill_delay.max(80) as u64));
                continue;
            }

            // 8. Aura-aware SpecialCombo for presets (CSH, AI, AM, AP)
            if let Some(combo_skill) = self.pick_special_combo_skill(&flash, cfg.class_preset) {
                if is_skill_ready(&flash, combo_skill) {
                    let _ = flash.call_str("UseSkill", &[FlashArg::from(combo_skill)]);
                    std::thread::sleep(Duration::from_millis(cfg.skill_delay.max(80) as u64));
                    continue;
                }
            }

            // 9. Normal skill rotation
            let skills = cfg.parsed_skills();
            if !skills.is_empty() {
                let s = &skills[skill_idx % skills.len()];
                if cfg.wait_skill {
                    // Wait up to 2.5s for this skill to come off cooldown
                    let deadline = Instant::now() + Duration::from_millis(2500);
                    while self.is_running()
                        && !self.is_stop_attack()
                        && !self.force_skill.load(Ordering::Acquire)
                        && !is_skill_ready(&flash, s)
                        && Instant::now() < deadline
                    {
                        std::thread::sleep(Duration::from_millis(60));
                    }
                    if self.is_running() && !self.is_stop_attack() {
                        let _ = flash.call_str("UseSkill", &[FlashArg::from(s.as_str())]);
                        skill_idx = skill_idx.wrapping_add(1);
                    }
                } else {
                    if is_skill_ready(&flash, s) {
                        let _ = flash.call_str("UseSkill", &[FlashArg::from(s.as_str())]);
                    }
                    skill_idx = skill_idx.wrapping_add(1);
                }
            }

            std::thread::sleep(Duration::from_millis(cfg.skill_delay.max(80) as u64));
        }
    }

    /// Gramiel crystal HP balancing: attacks assigned crystal (`id.2` or `id.3`), switches to the
    /// other crystal if ours drops too far ahead, or attacks `id.1` (Gramiel) when both are dead.
    fn select_gramiel_target(&self, flash: &Flash, ultra: UltraBossMode) {
        let Some((mine, other)) = ultra.gramiel_crystal_pair() else {
            return;
        };
        let mine_alive = flash
            .call_bool("IsMonsterAvailableByMonMapID", &[FlashArg::from(mine)])
            .unwrap_or(false);
        let other_alive = flash
            .call_bool("IsMonsterAvailableByMonMapID", &[FlashArg::from(other)])
            .unwrap_or(false);

        if mine_alive {
            let _ = flash.call_str("AttackMonsterByMonMapId", &[FlashArg::from(mine)]);
        } else if other_alive {
            let _ = flash.call_str("AttackMonsterByMonMapId", &[FlashArg::from(other)]);
        } else {
            let _ = flash.call_str("AttackMonsterByMonMapId", &[FlashArg::from("1")]);
        }
    }

    /// Aura-aware SpecialCombo mirroring `MaidRemake.cs` (`CSH`, `CSHv2`, `AI`, `AM`, `AP`).
    fn pick_special_combo_skill(&self, flash: &Flash, preset: MaidPreset) -> Option<&'static str> {
        match preset {
            MaidPreset::AP => {
                // Keep Righteous Seal (skill 3) active when aura drops
                let aura = get_aura_value(flash, "Righteous Seal");
                if aura <= 0.0 && is_skill_ready(flash, "3") {
                    return Some("3");
                }
                None
            }
            MaidPreset::AM => {
                // Maintain Corporeal Ascension / Arcane Sigil
                let corporeal = get_aura_value(flash, "Corporeal Ascension");
                if corporeal <= 0.0 && is_skill_ready(flash, "4") {
                    return Some("4");
                }
                None
            }
            MaidPreset::AI => {
                // Arcana Invoker: fire skill 4 when The World is active
                let world = get_aura_value(flash, "The World");
                if world > 0.0 && is_skill_ready(flash, "4") {
                    return Some("4");
                }
                None
            }
            MaidPreset::CSH | MaidPreset::CSHv2 => {
                // Chrono ShadowHunter: reload (skill 1) when bullets are empty
                let rounds = get_aura_value(flash, "Rounds");
                if rounds <= 0.0 && is_skill_ready(flash, "1") {
                    return Some("1");
                }
                None
            }
            _ => None,
        }
    }
}

fn is_player_in_map(flash: &Flash, username: &str) -> bool {
    flash
        .call_str("PlayersInMap", &[])
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .map(|list| {
            list.iter()
                .any(|p| p.trim().eq_ignore_ascii_case(username.trim()))
        })
        .unwrap_or(false)
}

fn is_skill_ready(flash: &Flash, idx: &str) -> bool {
    flash
        .call_str("SkillAvailable", &[FlashArg::from(idx)])
        .ok()
        .and_then(|s| s.trim().parse::<i32>().ok())
        .map(|v| v == 0)
        .unwrap_or(true)
}

fn get_aura_value(flash: &Flash, aura_name: &str) -> f64 {
    flash
        .call_str("GetAurasValue", &[FlashArg::from("true"), FlashArg::from(aura_name)])
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn parse_mon_map_id(spec: &str) -> Option<&str> {
    let trimmed = spec.trim();
    trimmed
        .strip_prefix("id:")
        .or_else(|| trimmed.strip_prefix("ID:"))
        .or_else(|| trimmed.strip_prefix("id."))
        .or_else(|| trimmed.strip_prefix("ID."))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn is_monster_available(flash: &Flash, spec: &str) -> bool {
    if let Some(id) = parse_mon_map_id(spec) {
        flash
            .call_bool("IsMonsterAvailableByMonMapID", &[FlashArg::from(id)])
            .unwrap_or(false)
    } else {
        flash
            .call_bool("IsMonsterAvailable", &[FlashArg::from(spec.trim())])
            .unwrap_or(false)
    }
}

fn attack_target_spec(flash: &Flash, spec: &str) {
    if let Some(id) = parse_mon_map_id(spec) {
        let _ = flash.call_str("AttackMonsterByMonMapId", &[FlashArg::from(id)]);
    } else {
        let _ = flash.call_str("AttackMonster", &[FlashArg::from(spec.trim())]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::maid::config::{parse_locked_map_entries, parse_whitelist_entries};

    #[test]
    fn maid_config_roundtrip_grimlite_li_json() {
        let li_json = r#"{
            "Target": "LeaderPlayer",
            "SkillList": "1,2,3,4",
            "LockedZoneHandlerMaps": "whitemap-100000;Enter;Spawn\nyulgar-9999;Enter;Spawn",
            "SafeSkillList": "2,3",
            "BuffStopAttackList": "5",
            "AttackPriorityMonster": "id.2,Ultra Gramiel",
            "SpecialMsg": "tc;2;Ultra Gramiel;0;1",
            "WhitelistMapMaps": "ultraengineer;r2\nyulgar",
            "RelogDelay": 4500,
            "SkillDelay": 150,
            "SafeSkillHP": 65,
            "SpecialAct": 5,
            "UltraBossExtra": 7,
            "AntiCounter": true,
            "CopyWalk": true,
            "AttackPriority": true,
            "BuffStopAttack": true,
            "WaitSkill": true,
            "StopFailedGoto": true,
            "LockedZoneHandler": true,
            "WhitelistMap": true,
            "GlobalHotkey": true,
            "SafeSkill": true
        }"#;

        let cfg = MaidConfig::from_json(li_json).expect("should parse Grimlite-Li MaidConfig");
        assert_eq!(cfg.target, "LeaderPlayer");
        assert!(cfg.special_anims);
        assert_eq!(cfg.stop_failed_goto_limit, 5);
        assert_eq!(cfg.ultra_boss_mode(), UltraBossMode::GramielL1);
        assert_eq!(UltraBossMode::AutoPotion.as_index(), 11);
        assert!(cfg.ultra_boss_mode().uses_enrage());
        assert_eq!(cfg.parsed_attack_priority(), vec!["id.2", "Ultra Gramiel"]);

        let locked = parse_locked_map_entries(&cfg.locked_zone_handler_maps);
        assert_eq!(locked.len(), 2);
        assert_eq!(locked[0].map, "whitemap-100000");
        assert_eq!(locked[1].cell, "Enter");

        let wl = parse_whitelist_entries(&cfg.whitelist_map_maps);
        assert_eq!(wl.len(), 2);
        assert_eq!(wl[0].map, "ultraengineer");
        assert_eq!(wl[0].cell, "r2");
        assert_eq!(wl[1].map, "yulgar");
        assert_eq!(wl[1].cell, "all");

        let tc = TauntCycleSpec::parse(&cfg.special_msg).expect("should parse tc;");
        assert_eq!(tc.cycle, 2);
        assert_eq!(tc.monster, "Ultra Gramiel");
        assert_eq!(tc.second, 0);
        assert_eq!(tc.order, 1);
    }

    #[test]
    fn preset_applies_exact_settings() {
        let mut cfg = MaidConfig::default();
        MaidPreset::CSH.apply_to(&mut cfg);
        assert_eq!(cfg.skill_list, "3,2,3,3,0,3,3,4,1");
        assert_eq!(cfg.safe_skill_hp, 80);
        assert_eq!(cfg.buff_stop_attack_list, "5");
        assert!(cfg.wait_skill);
        assert!(!cfg.safe_skill);
        assert!(cfg.buff_stop_attack);
    }
}
