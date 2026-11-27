//! Map navigation commands: Join, MoveToCell, Walk, SetSpawn, Yulgar
//! (aligned with Grimlite-Li `CmdJoin`, `CmdMoveToCell`, `CmdWalk`, `CmdSetSpawn`, `CmdYulgar`).

use super::CommandContext;
use crate::gbot::BotCommand;
use crate::host::FlashArg;
use std::sync::atomic::Ordering;

pub fn execute(ctx: &mut CommandContext<'_>, cmd: &BotCommand) -> usize {
    match cmd {
        BotCommand::Join { map, cell, pad } => {
            ensure_in_map_and_cell(ctx, map, cell, pad);
        }
        BotCommand::Yulgar => {
            ctx.exit_combat_in_place(1250);
            if !ctx.bot.player_snapshot().map.eq_ignore_ascii_case("yulgar") {
                join_map_wait(ctx, "yulgar", "Enter", "Spawn", 2);
            }
            let seed = pseudo_rand_u32();
            let rx = 150 + (seed % 550) as i32;
            let ry = 320 + ((seed / 7) % 130) as i32;
            let _ = ctx
                .flash
                .call_bool("walkTo", &[FlashArg::from(rx), FlashArg::from(ry)]);
            ctx.running.store(false, Ordering::Release);
            return ctx.idx;
        }
        BotCommand::MoveToCell { cell, pad } => {
            let c = ctx.resolve(cell);
            let p = ctx.resolve(pad);
            move_to_cell_loop(ctx, &c, &p);
        }
        BotCommand::Walk { x, y } => {
            let _ = ctx
                .flash
                .call_bool("walkTo", &[FlashArg::from(*x), FlashArg::from(*y)]);
            ctx.wait_until(5000, 150, || {
                match ctx.flash.call_json::<[i32; 2]>("getPosition", &[]) {
                    Ok(pos) => (pos[0] - *x).abs() <= 10 && (pos[1] - *y).abs() <= 10,
                    Err(_) => true,
                }
            });
        }
        BotCommand::SetSpawn => {
            let _ = ctx.flash.call_bool("setSpawnPoint", &[]);
        }
        _ => {}
    }
    ctx.idx + 1
}

/// Shared SSOT helper matching Grimlite-Li `CmdMoveToCell.cs`:
/// skips immediately if already in target cell, otherwise retries until cell matches.
pub fn move_to_cell_loop(ctx: &CommandContext<'_>, cell: &str, pad: &str) {
    if cell.is_empty() {
        return;
    }
    for _ in 0..6 {
        if !ctx.is_running() {
            break;
        }
        if ctx.bot.player_snapshot().cell.eq_ignore_ascii_case(cell) {
            break;
        }
        let _ = ctx
            .flash
            .call_bool("moveToCell", &[FlashArg::from(cell), FlashArg::from(pad)]);
        ctx.sleep(500);
    }
}

/// Shared SSOT helper matching Grimlite-Li `CmdJoin.cs`:
/// - Skips re-joining if already in target map (only moves cell if needed + sets spawn).
/// - Exits combat to `Blank, Spawn` before transferring maps.
/// - Waits for `Player.Map == base_map` (5s) and `!isMapLoading()` (40s), then moves cell & sets spawn.
pub fn ensure_in_map_and_cell(ctx: &CommandContext<'_>, map: &str, cell: &str, pad: &str) {
    let m = normalize_join_map(&ctx.resolve(map));
    let c = ctx.resolve(cell);
    let p = ctx.resolve(pad);
    let base_map = m.split('-').next().unwrap_or(&m);

    let snap = ctx.bot.player_snapshot();
    if snap.map.eq_ignore_ascii_case(base_map) {
        if !c.is_empty() && !snap.cell.eq_ignore_ascii_case(&c) {
            move_to_cell_loop(ctx, &c, &p);
        }
        let _ = ctx.flash.call_bool("setSpawnPoint", &[]);
        return;
    }

    ctx.exit_combat_to_blank(1500);
    join_map_wait(ctx, &m, &c, &p, 3);

    if !c.is_empty() && !ctx.bot.player_snapshot().cell.eq_ignore_ascii_case(&c) {
        move_to_cell_loop(ctx, &c, &p);
    }
    let _ = ctx.flash.call_bool("setSpawnPoint", &[]);
}

fn join_map_wait(ctx: &CommandContext<'_>, map: &str, cell: &str, pad: &str, tries: usize) {
    let base_map = map.split('-').next().unwrap_or(map);
    for _ in 0..tries.max(1) {
        if !ctx.is_running() {
            break;
        }
        let _ = ctx.flash.call_bool(
            "joinMap",
            &[
                FlashArg::from(map),
                FlashArg::from(cell),
                FlashArg::from(pad),
            ],
        );
        let arrived = ctx.wait_until(5000, 200, || {
            ctx.bot.player_snapshot().map.eq_ignore_ascii_case(base_map)
        });
        ctx.wait_until(40_000, 250, || {
            !ctx.flash.call_bool("isMapLoading", &[]).unwrap_or(false)
        });
        if arrived {
            break;
        }
    }
}

fn pseudo_rand_u32() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(42424)
}

/// Converts non-numeric private room suffixes (e.g. `spellcraft-a`) into random private room numbers.
pub fn normalize_join_map(map: &str) -> String {
    if let Some((name, room)) = map.split_once('-')
        && !room.is_empty()
        && room.parse::<u32>().is_err()
    {
        let room_num = 10000 + (pseudo_rand_u32() % 89999);
        return format!("{name}-{room_num}");
    }
    map.to_string()
}
