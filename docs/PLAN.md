# Grimlite Ri — Project Plan & Roadmap

**Grimlite Ri** adalah **revamp dari Grimlite-Li** untuk AdventureQuest Worlds yang berjalan di atas **Ruffle (`muktiarafi/ruffle-aqw`)** dan dapat dijalankan secara **multiplatform** (macOS, Windows, Linux, Android, serta Headless Mode) menggunakan **egui**.

- **Aturan Utama Behavior Bot (Grimlite-Li Parity)**:
  - **Behavior bot tetap mengikuti Grimlite-Li** — seluruh eksekusi command (`Quest`, `Map`, `Combat`, `Item`, `Misc`, `Statements`), format `.gbot` (`$type` + `$values`), skill rotation (`SkillSet` + Safe Skill), dan mekanisme combat/navigation/quest mengacu langsung pada `Grimlite-Li/Grimoire/Botting/Commands/`.
  - Yang di-revamp adalah **runtime** (dari Adobe Flash / Windows-only C# menjadi Rust + Ruffle) dan **UI** (antarmuka modern lintas platform dengan `egui`).
- **Target Pengembangan**:
  - **MVP & Core**: **macOS App** (Metal via `wgpu`) + **Headless Mode** (`grimlite-headless`, 0% GPU, 2 FPS, multi-account).
  - **Ekspansi Multiplatform**: Windows, Linux, dan Android memakai core engine (`bot/`, `frontend/`, `egui_ui/`) yang sama persis tanpa refactor logic.

---

## Prinsip Engineering & Standar Kode

Setiap modul baru wajib mengikuti prinsip berikut agar codebase tetap bersih, scalable, dan mudah dirawat:

1. **Grimlite-Li Behavior Parity**:
   - Setiap command baru atau pembaruan command di `bot/src/commands/` harus diverifikasi terhadap implementasi referensi di `Grimlite-Li/Grimoire/Botting/Commands/` agar file `.gbot` lama maupun baru berjalan identik.
2. **SSOT (Single Source of Truth)**:
   - Setiap state hanya punya **satu pemilik tunggal**: `GameState` untuk data player/room/inventory/bank/quests/drops, `OptionsStore` untuk opsi runtime, `BotRunner` untuk `BotConfig` aktif, `CaptureProxy` untuk lalu lintas paket. UI (`egui_ui`) hanya membaca snapshot dan mengirim aksi.
   - Helper bersama (`map::ensure_in_map_and_cell`, `quest::accept_quest`, `quest::complete_quest`, `CommandContext::wait_until`, `CommandContext::exit_combat_*`, `theme::*`) ditulis sekali dan dipakai ulang lintas desktop & headless.
3. **Struktur Modular Mengikuti Domain Grimlite-Li**:
   - Hindari *god file*. Command bot dipisah per kategori di `bot/src/commands/{mod,combat,map,item,quest,misc,statements}.rs`, UI dipisah per fitur di `egui_ui/src/{lib,theme,bot_manager,command_editor}.rs` dan `egui_ui/src/tools/{mod,travels,loaders,hotkeys,packets}.rs`.
4. **Separation of Concerns Antar Crate**:
   - `bot/` tidak boleh bergantung pada `ruffle`, `egui`, atau `winit`.
   - `frontend/` tidak boleh bergantung pada `egui`, `winit`, atau `rfd`.
   - `egui_ui/` tidak boleh bergantung pada `winit` atau `rfd` (file dialog disuntikkan via callback dari `app/` / `headless/`).
5. **Testability**:
   - Setiap fitur baru di `bot/` (parser, command, packet handler, interceptor) dilengkapi unit test tanpa butuh GPU/Flash.

---

## Status Progres

### Fase 0: Fondasi Workspace & macOS Bootable Client ✅
- [x] Inisialisasi repo git `Grimlite-Ri` & workspace Cargo (`host`, `bot`, `frontend`, `egui_ui`, `app`, `headless`).
- [x] Tambah submodule `ruffle/` (`muktiarafi/ruffle-aqw`, branch `aqw`) + `[patch.crates-io]` dan `rust-toolchain.toml` (`1.99.1`).
- [x] Buat crate `host/` (`grimlite_host`) untuk kompilasi `host/as3/grimlite/host/Main.as` → `bot/assets/grimlite.swf` memakai `asc.jar`.
- [x] Build & verifikasi binary macOS (`grimlite-ri`) dengan top bar `egui` (termasuk quick-jump Cell/Pad, Start Bot, Auto Attack, Bank) dan native detached multi-viewport windows.

### Fase 1: Flash Bridge, Direct Socket Capture/Injection & Packet Tools ✅
- [x] Marshalling `FlashHost` (Rust → Ruffle `Player::call_internal_interface`) via `RuffleEvent::GrimliteFlashCall` dengan proteksi anti-deadlock UI thread.
- [x] `CaptureNavigator` + `CaptureProxy` in-process: baca, ubah live, blokir, zlib inflate/deflate, serta **injeksi paket langsung dua arah** (`send_to_server` & `send_to_client`).
- [x] State game live (`PlayerSnapshot`, `GameState`, `StatePacketInterceptor`: inventory, temp inventory, bank, quests, drops, cells) & `OptionsManager`.
- [x] Panel `egui` di dropdown `Tools`: **Fast Travels**, **Loaders**, **Hotkeys**, **Packet Logger**, **Packet Spammer & Sender**, dan **Packet Tamperer / Interceptor**.

### Fase 2: Engine `.gbot` & Grimlite-Li Command Parity ✅
- [x] Parser & serializer `.gbot` kompatibel Grimlite-Li (`$type` + `$values`) di `bot/src/gbot.rs`.
- [x] `BotRunner` (`bot/src/runner.rs`): eksekusi command, resolver `[var]` / `{PLAYERNAME}` + ekspresi matematika, auto-pickup drop, Start/Pause/Stop.
- [x] Penyelarasan behavior command dengan **Grimlite-Li**:
  - **Quest**: `AcceptQuest` (cek `isQuestInProgress`, lazy load `questTree`, skip non-repeatable, retry max 2x), `CompleteQuest` (cek `canCompleteQuest`, `exit_combat_before_quest`, cooldown turn-in).
  - **Map**: `Join` (skip jika sudah di map, keluar combat ke `Blank, Spawn`, tunggu map & `!isMapLoading`, pindah cell + `setSpawnPoint`), `MoveToCell` (skip jika sudah di cell, retry loop), `Walk`, `SetSpawn`, `Yulgar`.
  - **Combat**: `Kill` (active `SkillSet`, Safe Skill HP/MP `<=`/`>=`/`==`, boss priority, `cancelTarget`), `KillFor` (multi-item), `ShortHunt` (multi-cell rotation), `QuestHunt`, `Attack`, `UseSkill` (`Monster`/`Wait`/`Force`), `SkillSet`, `Rest`, `CancelTarget`.
  - **Item & Misc**: `GetDrop`, `LoadShop`, `Buy`, `Sell`, `Equip` (keluar combat + verifikasi `isEquipped`), `BankTransfer`, `MapItem` (1500ms), `Whitelist` (runtime drops/pickup), `Packet` (`{ROOM_ID}`, `{ROOM_NUMBER}`, `PLAYERNAME`, `{GETMAP}`), `Delay`, `Label`, `GotoLabel`, `Index`, `Int`, `SetVar`, `Log`, `Provoke`, `SetFPS`, `Stop`, `Restart`, dan 37 Statement kondisional.
- [x] Unit test `grimlite_bot` (`7/7 passed`).

### Fase 3: Bot Manager Redesign, Headless Multi-Account, Ultra Boss & Maid Remake (Sedang Berjalan)
- [x] **Design System & Reusable Theme** (`egui_ui/src/theme.rs`): modern dark palette, card/group box, segmented switch, primary/toggle/danger buttons untuk desktop & headless.
- [x] **Bot Manager Redesign & Command Editor** (`egui_ui/src/bot_manager.rs`, `command_editor.rs`): layout 2 kolom, multi-select, copy/cut/paste `.gbot` JSON via OS clipboard, search, double-click Command Editor, dan tab lengkap (`Combat`, `Skills`, `Map`, `Item`, `Quest`, `Misc`, `Misc 2`, `Options`, `Bots`, `Info`).
- [x] **Headless Mode — Multi-Account Dashboard** (`headless/`): Ruffle `NullRenderer` (0% GPU, 2 FPS), multi-tab akun, `Main` / `Inventory` (Inv, Temp, Bank, Drops) / `Quests`, Live Packets bertimestamp.
- [ ] **Ultra Boss Packet Handlers & `TauntCycle`**: Ultra Dage, Ultra Speaker, Champion Drakath, Ultra Gramiel, Dark Carnax, Queen Iona, Astral Empyrean, Vordred + koneksi ke `CmdSpecialHandler` & `CmdTauntcycle`.
- [ ] **Maid Remake**: engine support/follower bot + panel UI `egui`.
- [ ] **Utilitas Lanjutan (Opsional)**: Skill Presets, Cosmetics, Bank/Inventory Viewer di mode Desktop.

---

## Ekspansi Multiplatform

Menggunakan core crate yang sama (`bot/`, `host/`, `frontend/`, `egui_ui/`):

1. **macOS Desktop** ✅ — Metal backend (`wgpu`), native detached viewports.
2. **Headless Mode** ✅ — `grimlite-headless` (0% GPU, 2 FPS, multi-account).
3. **Windows & Linux Desktop** — Reuse crate `app/` dengan backend DX12/Vulkan, packaging & CI build.
4. **Android (arm64)** — Crate `android/` (`cdylib` + `GameActivity` + Vulkan + `ForegroundService`), `egui_ui` otomatis fallback ke `EmbeddedWindow` dengan adaptasi layar sentuh.
