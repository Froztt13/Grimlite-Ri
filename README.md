# Grimlite Ri

A ground-up Rust rebuild of **Grimlite-Li** for AdventureQuest Worlds, powered by **Ruffle + Mugetsu/Turu patches (`ruffle-aqw`)** and **egui**.

## Architecture

- `ruffle/` — `muktiarafi/ruffle-aqw` submodule (Ruffle + Mugetsu 2.0.0 + Turu patches)
- `host/` — `grimlite_host`: ActionScript 3 host SWF (`grimlite.swf`), built with Ruffle's `asc.jar`
- `bot/` — `grimlite_bot`: Core bot engine (`.gbot` runner, commands, packet capture/handlers, Maid Remake) with zero Ruffle/UI dependencies
- `frontend/` — `grimlite_frontend`: Ruffle `Player` ↔ `grimlite_bot` bridge (`FlashHost`, `ExternalInterface`, `CaptureNavigator` in-process socket interception)
- `egui_ui/` — `grimlite_egui`: Cross-platform `egui` interface
- `app/` — `grimlite-ri`: Desktop application (macOS-first; Windows & Android planned in separate phases)

## Build & Run (macOS)

Requires Rust and Java 17+ (for building `grimlite.swf`).

```bash
# 1. Build the embedded host SWF (bot/assets/grimlite.swf)
cargo run -p grimlite_host

# 2. Build and run Grimlite Ri on macOS
cargo run -p grimlite-ri --no-default-features --features software_video,external_video,lzma,jpegxr,fontconfig-dlopen
```

## License

GPL-3.0-or-later (matching `ruffle-aqw` and Mugetsu).
