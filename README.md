# Grimlite Ri

**Grimlite Ri** is a multiplatform revamp of **Grimlite-Li** for AdventureQuest Worlds, built in Rust on top of **Ruffle (`muktiarafi/ruffle-aqw`)** and **egui**.

It preserves **Grimlite-Li's bot behavior and `.gbot` compatibility** while replacing the legacy Windows-only Flash runtime and WinForms UI with a modern cross-platform engine supporting **Desktop (macOS, Windows, Linux)**, **Headless (0% GPU, 2 FPS multi-account)**, and **Android (planned)**.

## Highlights

- **Grimlite-Li Bot Behavior Parity**: `.gbot` JSON compatibility (`$type` + `$values`) and command execution aligned with Grimlite-Li (`Quest`, `Map`, `Combat`, `SkillSet` + Safe Skill, `Item`, `Misc`, and 37 conditional statements).
- **Powered by Ruffle (`ruffle-aqw`)**: Runs AQW natively without Adobe Flash Player, with in-process socket packet capture/injection (`CaptureNavigator`).
- **Multiplatform `egui` UI**: Modern dark UI with native detached tool windows on desktop (automatic embedded fallback on mobile) and a lightweight multi-account Headless dashboard.

## Workspace Architecture

- `ruffle/` — `muktiarafi/ruffle-aqw` submodule (Ruffle + Mugetsu + Turu patches)
- `host/` — `grimlite_host`: ActionScript 3 host SWF (`grimlite.swf`), compiled with Ruffle's `asc.jar`
- `bot/` — `grimlite_bot`: Core bot engine (`.gbot` runner, Grimlite-Li command executors, packet capture/handlers, state) with zero Ruffle/UI dependencies
- `frontend/` — `grimlite_frontend`: Ruffle `Player` ↔ `grimlite_bot` bridge (`FlashHost`, `ExternalInterface`, `CaptureNavigator`)
- `egui_ui/` — `grimlite_egui`: Shared cross-platform `egui` interface, reusable theme, Bot Manager, Command Editor, and Tools
- `app/` — `grimlite-ri`: Desktop application (`winit` + `wgpu`)
- `headless/` — `grimlite-headless`: Multi-account headless runner using Ruffle `NullRenderer` (0% GPU, 2 FPS)

## Build & Run (macOS)

Requires Rust and Java 17+ (for building `grimlite.swf`).

```bash
# 1. Build the embedded host SWF (bot/assets/grimlite.swf)
cargo run -p grimlite_host

# 2. Run Grimlite Ri Desktop (macOS)
cargo run -p grimlite-ri --no-default-features --features software_video,external_video,lzma,jpegxr,fontconfig-dlopen

# 3. Run Grimlite Ri Headless (multi-account, 0% GPU)
cargo run -p grimlite-headless
```

## Documentation

- [docs/PLAN.md](docs/PLAN.md) — Roadmap, Grimlite-Li parity principles, and platform plan
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Crate map and threading rules

## License

GPL-3.0-or-later (matching `ruffle-aqw` and Mugetsu).
