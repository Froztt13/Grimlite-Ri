# Grimlite Ri

**Grimlite Ri** is a multiplatform Rust revamp of **Grimlite-Li** for AdventureQuest Worlds, built on top of **Ruffle (`muktiarafi/ruffle-aqw`)** and **egui**.

It preserves **Grimlite-Li's bot behavior and `.gbot` compatibility** while replacing the legacy Windows-only Flash runtime and WinForms UI with a modern cross-platform engine supporting **Desktop (macOS, Windows, Linux)**, **Headless**, and **Android (planned)**.

## Highlights

- **Grimlite-Li Bot Behavior Parity**: `.gbot` JSON compatibility (`$type` + `$values`) and command execution aligned with Grimlite-Li (`Combat`, `SkillSet` + Safe Skill, `Map`, `Item`, `Quest`, `Misc`, `Misc 2`, 37 conditional statements, Anti-Counter Attack, 15 Special Handlers, and Auto-Relogin).
- **Powered by Ruffle (`ruffle-aqw`)**: Runs AQW natively without Adobe Flash Player, with in-process socket packet capture/injection (`CaptureNavigator`).
- **Multiplatform `egui` UI**: Modern dark UI with native always-on-top detached tool windows on desktop and a lightweight multi-account **Headless** dashboard.

## Workspace Architecture

- `ruffle/` — [`muktiarafi/ruffle-aqw`](https://github.com/muktiarafi/ruffle-aqw) submodule (Ruffle + Mugetsu + Turu patches)
- `host/` — `grimlite_host`: ActionScript 3 host SWF (`grimlite.swf`), based on `grimlite-li-swf` and compiled with Ruffle's `asc.jar`
- `bot/` — `grimlite_bot`: Core bot engine (`.gbot` runner, command executors, packet capture/handlers, grabber, auto-relogin, state) with zero Ruffle/UI dependencies
- `frontend/` — `grimlite_frontend`: Ruffle `Player` ↔ `grimlite_bot` bridge (`FlashHost`, `ExternalInterface`, `CaptureNavigator`)
- `egui_ui/` — `grimlite_egui`: Shared cross-platform `egui` interface, reusable design system (`theme.rs`), modular Bot Manager (`bot_managers/`), Command Editor, and Tools
- `app/` — `grimlite-ri`: Desktop application (`winit` + `wgpu`) with native multi-viewport support
- `headless/` — `grimlite-headless`: Multi-account headless runner using Ruffle `NullRenderer` (0% GPU, 2 FPS)

## Build & Run

Requires **Rust (`1.99.1`, via `rust-toolchain.toml`)** and **Java 17+** (only if rebuilding `bot/assets/grimlite.swf`).

```bash
# Clone with the ruffle-aqw submodule
git clone --recursive https://github.com/Froztt13/Grimlite-Ri.git
cd Grimlite-Ri

# Optional: rebuild embedded host SWF (bot/assets/grimlite.swf)
cargo run -p grimlite_host
```

### macOS (Apple Silicon)

```bash
# Fast debug .app bundle (build & launch)
./scripts/build-macos-app.sh --run

# Release .app bundle (target/release/bundle/Grimlite Ri.app)
./scripts/build-macos-app.sh --release

# Headless multi-account client
cargo run -p grimlite-headless
```

### Windows (x64)

```powershell
# Desktop client
cargo build --release -p grimlite-ri --no-default-features --features software_video,external_video,lzma,jpegxr

# Headless multi-account client
cargo build --release -p grimlite-headless
```

Prebuilt **macOS** and **Windows** artifacts can also be generated on demand via **GitHub Actions** ([`.github/workflows/build.yml`](.github/workflows/build.yml)).

## Documentation

- [docs/PLAN.md](docs/PLAN.md) — Roadmap, Grimlite-Li parity principles, and platform plan
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Crate map and threading rules

## References & Credits

Grimlite Ri builds upon and draws reference from the following open-source projects:

- **[Grimlite-Li](https://github.com/Froztt13/Grimlite-Li)** — Base project.
- **[grimlite-li-swf](https://github.com/Froztt13/grimlite-li-swf)** — ActionScript 3 host SWF architecture (`grimoire.game.*`, `Externalizer`, `Caller`) adapted in `host/as3/`.
- **[ruffle-aqw](https://github.com/muktiarafi/ruffle-aqw)** (`aqw` branch, incorporating **Mugetsu** and **Turu** patches) — AQW-compatible Ruffle runtime and socket capture/injection hooks.
- **[Ruffle](https://github.com/ruffle-rs/ruffle)** — Open-source Flash Player emulator written in Rust (Copyright © Ruffle LLC and Ruffle contributors, dual-licensed under MIT / Apache-2.0).
- **[egui](https://github.com/emilk/egui)** & **[wgpu](https://github.com/gfx-rs/wgpu)** — Cross-platform immediate-mode GUI and GPU rendering stack (MIT / Apache-2.0).

## License & Copyright

- **Grimlite Ri** is licensed under the **GNU General Public License v3.0 or later (`GPL-3.0-or-later`)**, matching `ruffle-aqw` and Mugetsu.
- Copyright © 2026 Grimlite Ri contributors.
- Third-party components retain their respective copyrights and licenses (see `ruffle/LICENSE.md` for Ruffle's MIT / Apache-2.0 notices).

## Disclaimer

*AdventureQuest Worlds* (AQW) and its assets are trademarks and copyrights of **Artix Entertainment, LLC**. Grimlite Ri is an independent, unofficial open-source project and is not affiliated with, endorsed by, or supported by Artix Entertainment, LLC.
