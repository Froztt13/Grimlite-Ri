# Grimlite Ri — Architecture & Code Map

## Workspace Layout

| Path | Crate | Peran |
|---|---|---|
| `ruffle/` | *(submodule)* | `muktiarafi/ruffle-aqw` (branch `aqw`): Ruffle + Mugetsu 2.0.0 + patch Turu (`wgpu-core`, `wgpu-hal`, `gc-arena`, `lyon_*`, `egui-wgpu`). |
| `host/` | `grimlite_host` | Source AS3 (`host/as3/grimlite/host/Main.as`) & compiler SWF via `asc.jar` → menghasilkan `bot/assets/grimlite.swf`. |
| `bot/` | `grimlite_bot` | Core engine bot (tanpa dependensi Ruffle/UI — dapat di-unit test cepat dengan fake host). |
| `frontend/` | `grimlite_frontend` | Jembatan Ruffle ↔ `grimlite_bot` (`CaptureNavigator`, `GrimliteExternalInterface`, `RuffleFlashHost`). Tanpa `egui`/`winit`/`rfd`. |
| `egui_ui/` | `grimlite_egui` | Seluruh UI `egui` lintas platform (`BotManagerWindow`, `ToolsWindows`, Options, Packet Logger/Spammer/Tamperer, Logs). |
| `app/` | `grimlite-ri` | Binary desktop (`winit` + `wgpu` Metal/DX12/Vulkan + dialog native `rfd`). |

## Peta File Penting

- **Definisi Command & Format `.gbot`**: `bot/src/gbot.rs` (`BotConfig`, `BotCommand`, `StatementKind`, `from_gbot_json`, `to_gbot_json`)
- **Bot Runner & Variable/Math Resolver**: `bot/src/runner.rs` (`BotRunner`, `BotVars`)
- **Eksekutor Command (Modular mengikuti `Grimoire/Botting/Commands/`)**:
  - `bot/src/commands/mod.rs` — `CommandContext`, dispatcher `execute_command`, shared helpers (`sleep_interruptible`, `handle_drops`)
  - `bot/src/commands/combat.rs` — `Kill`, `KillFor`, `ShortHunt`, `QuestHunt`, `Attack`, `UseSkill`, `Rest`, `CancelTarget`
  - `bot/src/commands/map.rs` — `Join`, `MoveToCell`, `Walk`, `SetSpawn`, `Yulgar`, `ensure_in_map_and_cell`, `normalize_join_map`
  - `bot/src/commands/item.rs` — `GetDrop`, `LoadShop`, `Buy`, `Sell`, `Equip`, `BankTransfer`, `MapItem`, `Whitelist`
  - `bot/src/commands/quest.rs` — `AcceptQuest`, `CompleteQuest`
  - `bot/src/commands/misc.rs` — `Delay`, `Label`, `GotoLabel`, `Index`, `Packet`, `Int`, `SetVar`, `Log`, `Provoke`, `SetFPS`, `Stop`, `Restart`, `Blank`
  - `bot/src/commands/statements.rs` — `evaluate` (37 conditional statements)
- **Packet Capture, Interceptor & Direct Socket Injection**:
  - `bot/src/capture.rs` (`CaptureProxy`, `Stream`, `Interceptor`, `PacketSink`)
  - `frontend/src/capture.rs` (`CaptureNavigator`, `RuffleSocketSink`)
  - `bot/src/tamper.rs` (`TamperEngine`), `bot/src/spammer.rs` (`PacketSpammer`), `bot/src/dps.rs` (`DpsMeter`)
- **Game State & Options**: `bot/src/state.rs`, `bot/src/options.rs`, `bot/src/lib.rs`
- **Host SWF AS3 Callbacks**: `host/as3/grimlite/host/Main.as`
- **UI Windows**: `egui_ui/src/lib.rs`, `egui_ui/src/bot_manager.rs`, `egui_ui/src/tools.rs`

## Aturan Threading Kritis

1. **Ruffle `Player` berjalan hanya di engine/main thread**: semua `FlashHost::call` dari bot/worker thread di-marshal via `RuffleEvent::GrimliteFlashCall` (`frontend/src/ruffle_host.rs`).
2. **UI thread tidak boleh memanggil Flash secara sinkron**: aksi UI (Fast Travel, Loaders, dll.) selalu dipanggil lewat `Bot::spawn_flash_action(...)`.
3. **Callback `ExternalInterface` inbound (`Bot::on_flash_call`) wajib non-blocking**: tidak boleh mengunci atau memanggil balik Flash di dalamnya.
