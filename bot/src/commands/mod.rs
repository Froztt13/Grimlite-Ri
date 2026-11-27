//! Modular Grimlite-Li command executors mirroring `Grimoire/Botting/Commands/`.

pub mod combat;
pub mod item;
pub mod map;
pub mod misc;
pub mod quest;
pub mod statements;

use crate::Bot;
use crate::flash::Flash;
use crate::gbot::{BotCommand, BotConfig};
use crate::host::FlashArg;
use crate::runner::BotVars;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Shared execution context passed to every command handler (eliminates parameter sprawl).
pub struct CommandContext<'a> {
    pub bot: &'a Bot,
    pub flash: &'a Flash,
    pub config: &'a mut BotConfig,
    pub idx: usize,
    pub vars: &'a mut BotVars,
    pub skill_cursor: &'a mut usize,
    pub active_skill_set: &'a mut String,
    pub running: &'a AtomicBool,
    pub username: String,
}

impl<'a> CommandContext<'a> {
    pub fn resolve(&self, input: &str) -> String {
        self.vars.resolve(input, &self.username)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn sleep(&self, ms: u64) {
        sleep_interruptible(ms, self.running);
    }

    pub fn handle_drops(&self) {
        handle_drops(self.bot, self.config);
    }

    /// Polls `pred` every `interval_ms` until it returns `true`, `timeout_ms` elapses, or bot stops.
    pub fn wait_until(&self, timeout_ms: u64, interval_ms: u64, mut pred: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        while self.is_running() {
            if pred() {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            self.sleep(interval_ms.max(25));
        }
        false
    }

    /// AQW `intState == 2` means the player is currently in combat.
    pub fn is_in_combat(&self) -> bool {
        self.bot.player_snapshot().state == 2
    }

    /// Exits combat by jumping to the current cell (mirrors Grimlite-Li `Player.ExitCombat` / `refreshCell`).
    pub fn exit_combat_in_place(&self, post_delay_ms: u64) {
        if !self.is_in_combat() {
            return;
        }
        let _ = self.flash.call_bool("cancelTarget", &[]);
        let snap = self.bot.player_snapshot();
        let cell = if snap.cell.is_empty() { "Enter" } else { snap.cell.as_str() };
        let pad = if snap.pad.is_empty() { "Spawn" } else { snap.pad.as_str() };
        let _ = self
            .flash
            .call_bool("moveToCell", &[FlashArg::from(cell), FlashArg::from(pad)]);
        self.wait_until(3000, 150, || !self.is_in_combat());
        if post_delay_ms > 0 {
            self.sleep(post_delay_ms);
        }
    }

    /// Exits combat by jumping to safe cell `Blank, Spawn` (used before `Join` map).
    pub fn exit_combat_to_blank(&self, post_delay_ms: u64) {
        if !self.is_in_combat() {
            return;
        }
        let _ = self.flash.call_bool("cancelTarget", &[]);
        let _ = self
            .flash
            .call_bool("moveToCell", &[FlashArg::from("Blank"), FlashArg::from("Spawn")]);
        self.wait_until(3000, 150, || !self.is_in_combat());
        if post_delay_ms > 0 {
            self.sleep(post_delay_ms);
        }
    }
}

pub fn sleep_interruptible(ms: u64, running: &AtomicBool) {
    let mut slept = 0u64;
    while slept < ms && running.load(Ordering::Acquire) {
        let step = (ms - slept).min(50);
        std::thread::sleep(Duration::from_millis(step));
        slept += step;
    }
}

pub fn handle_drops(bot: &Bot, config: &BotConfig) {
    if !config.enable_pickup && !config.enable_pickup_all && !config.enable_pickup_ac_tagged {
        return;
    }
    let room_id = bot.state.room_id();
    for drop in bot.state.drop_list() {
        let want = config.enable_pickup_all
            || (config.enable_pickup_ac_tagged && drop.is_ac)
            || config
                .drops
                .iter()
                .any(|d| d.eq_ignore_ascii_case(&drop.name));
        if want {
            let pkt = format!("%xt%zm%getDrop%{room_id}%{}%", drop.id);
            let _ = bot.capture.send_to_server(&pkt);
        }
    }
}

/// Dispatches a `BotCommand` to its category module and returns the next command index.
pub fn execute_command(ctx: &mut CommandContext<'_>, cmd: &BotCommand) -> usize {
    match cmd {
        // Combat
        BotCommand::Kill { .. }
        | BotCommand::KillFor { .. }
        | BotCommand::ShortHunt { .. }
        | BotCommand::QuestHunt { .. }
        | BotCommand::Attack { .. }
        | BotCommand::UseSkill { .. }
        | BotCommand::SkillSet { .. }
        | BotCommand::Rest { .. }
        | BotCommand::CancelTarget => combat::execute(ctx, cmd),

        // Map
        BotCommand::Join { .. }
        | BotCommand::MoveToCell { .. }
        | BotCommand::Walk { .. }
        | BotCommand::SetSpawn
        | BotCommand::Yulgar => map::execute(ctx, cmd),

        // Item
        BotCommand::GetDrop { .. }
        | BotCommand::LoadShop { .. }
        | BotCommand::Buy { .. }
        | BotCommand::Sell { .. }
        | BotCommand::Equip { .. }
        | BotCommand::BankTransfer { .. }
        | BotCommand::MapItem { .. }
        | BotCommand::Whitelist { .. } => item::execute(ctx, cmd),

        // Quest
        BotCommand::AcceptQuest { .. }
        | BotCommand::CompleteQuest { .. }
        | BotCommand::AddQuestList { .. }
        | BotCommand::RemoveQuestList { .. } => quest::execute(ctx, cmd),

        // Conditional Statements
        BotCommand::Statement {
            kind,
            value1,
            value2,
        } => {
            let v1 = ctx.resolve(value1);
            let v2 = ctx.resolve(value2);
            if statements::evaluate(ctx, kind, &v1, &v2) {
                ctx.idx + 1
            } else {
                ctx.idx + 2
            }
        }

        // Misc & fallback
        _ => misc::execute(ctx, cmd),
    }
}
