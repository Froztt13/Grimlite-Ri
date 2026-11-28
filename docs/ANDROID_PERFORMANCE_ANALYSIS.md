# Analisis Mendalam: Build APK Android Ruffle (`turu`) vs Masalah Performa & Drop FPS (`Grimlite-Ri`)

> **Ringkasan Temuan Utama:** Submodule `ruffle` (`ruffle-aqw`) di `Grimlite-Ri` (`882183aa`, tag `aqw-2.0.0-1`) **sudah setara** (bahkan 1 commit di depan `turu` `d0e5bc01`) dan sudah membawa seluruh patch performa Mugetsu 2.0.0 + Turu. Artinya, performa buruk dan **drop FPS berulang saat berjalan meski counter FPS in-game terbaca stabil 24** di `Grimlite-Ri` **bukan disebabkan oleh engine Ruffle**, melainkan oleh **bug fatal pada anchor waktu frame-pacing (`next_frame`) di `Grimlite-Ri/android/src/lib.rs`**, ditambah beberapa celah konfigurasi runtime, input, dan manajemen memori GPU dibanding `turu`.

---

## 1. Bagaimana `turu` Melakukan Build APK Android dengan Ruffle & Improvement-nya

### 1.1 Pipeline Build APK (`Cargo` + `cargo-ndk` + `Gradle`)

- **Orkestrasi Gradle → Rust otomatis** (`turu/android/app/build.gradle.kts:154-183`):
  Task `preBuild` otomatis menjalankan `cargoNdk`:
  ```bash
  cargo ndk --platform 26 -o app/src/main/jniLibs -t arm64-v8a build -p turu_android --release
  ```
  - Native library **selalu** dibangun dengan `--release` (bahkan pada `assembleDebug`), dan secara eksplisit menargetkan `--platform 26` agar selaras dengan `minSdk = 26`.
  - Menggunakan Android NDK `27.3.13750724` (`.github/workflows/release.yml:342`), `rustls` (`ring` provider) + `rustls-platform-verifier` (dari submodule `android/vendor/rustls-platform-verifier`) untuk verifikasi sertifikat TLS sistem Android.
  - Pada build release, **R8 aktif penuh** (`isMinifyEnabled = true`, `isShrinkResources = true` dengan `proguard-rules.pro` yang menjaga kelas JNI `com.muktiarafi.turu.**`, `GameActivity`, dan `org.rustls.platformverifier.**`), memangkas ukuran DEX dari ~10 MB menjadi ~2.6 MB.
- **Patch Workspace Ruffle-AQW** (`turu/Cargo.toml:77-83`):
  6 crate di-override lewat `[patch.crates-io]` ke `ruffle/patches/`:
  - `wgpu-core` (hilangkan Pre-Pass command buffer redundan, drop command buffer selesai di background thread, toleransi `device_lost`)
  - `wgpu-hal` (optimasi Vulkan & GLES replay)
  - `gc-arena`
  - `lyon_path` & `lyon_algorithms` (fill-rule `Positive` untuk Flash-style edge AA tanpa MSAA)
  - `egui-wgpu`

### 1.2 Improvement Ruffle, Renderer & Pacing di `turu`

1. **8 Switch Speed-up Mugetsu 2.0.0** (`turu/frontend/src/host.rs:281-299`, diset sebelum Rayon/Tokio spawn di `turu/android/src/lib.rs:586-590`):
   - `RUFFLE_LEAN_SCRIPTS`, `RUFFLE_LEAN_FRAMES`, `RUFFLE_LEAN_RENDER` (skip subtree display list yang tidak berubah)
   - `RUFFLE_PAGE_BLENDS`, `RUFFLE_SHAPE_UPLOADS`, `RUFFLE_PICK_BOUNDS`, `RUFFLE_SHARP_SNAPSHOTS`, `RUFFLE_EARLY_SCRIPTS`
2. **Tuning Thread-Local Player untuk AQW** (`turu/android/src/lib.rs:644-652`):
   - `ruffle_core::set_show_all_apis(true)`
   - `ruffle_core::set_shape_wait(Some(Duration::from_millis(12)))`
   - `ruffle_core::set_forgiving_removal(true)`
   - `ruffle_render::tessellator::set_flash_aa(true)` (1× sample tanpa MSAA — sangat krusial untuk GPU tiled mobile seperti Mali/Adreno)
3. **Optimasi OpenGL ES di `wgpu-hal` + Pilihan Backend** (`turu/docs/GL_RENDERER_PROPOSAL.md`):
   - **G0**: Guard `DownlevelFlags::BASE_VERTEX` agar GLES 3.0/3.1 bisa menggambar scene secara utuh.
   - **G2a & G2b** (`RUFFLE_GL_LEVERS`): hanya detach attachment FBO yang terpakai + shadow state GL selama satu submit → memangkas GL calls dari ~20.7 menjadi ~11.2 per draw (~50% reduksi).
   - **Opsi Pengguna**: `Options > Application > Graphics` (Auto / OpenGL ES / Vulkan), **Performance Mode** (skala render 85% Balanced / 70% Fast), **Frame Interpolation**, dan **Safe Drawing**.
4. **Frame Pacing Presisi & Default 30 FPS**:
   - Di `turu/frontend/src/engine.rs:86-95`, deadline frame berikutnya dihitung dari **`now` (waktu sebelum `tick()` dan `render()` dijalankan)**:
     ```rust
     let now = Instant::now();
     player.tick(FloatDuration::from_std(now.duration_since(time)));
     let next_step = step(&mut player, Step::Ticked { frame_ms }); // render + draw + present
     let next_frame = now + player.time_til_next_frame();
     ```
     Dengan cara ini, waktu eksekusi `tick + render + present` otomatis masuk ke dalam budget frame dan tidak menyebabkan drift.
   - **Host SWF default 30 FPS** (`turu/host/src/main.rs:41-45`): 30 FPS membagi rata layar 60 Hz (2 refresh/frame), 90 Hz (3 refresh/frame), dan 120 Hz (4 refresh/frame) — menghindari *3:2 pulldown judder* khas 24 FPS di layar 60 Hz.
   - Membaca refresh rate layar aktual (`DisplayManager.DisplayListener` → `refresh_millihertz`), meminta refresh rate tinggi saat Set FPS > 60 (`preferRefreshRate`), dan mendukung **Frame Interpolation** (`turu_frontend::pacing::Pacing`).
5. **Arsitektur Multi-Proses & UI Native Jetpack Compose**:
   - `TuruActivity` hanya berfungsi sebagai shell UI menggunakan **Jetpack Compose** (`NativeRail.kt`, `ShellScreens.kt`, dst.) yang terpisah dari loop render game.
   - Setiap tab game berjalan di proses Android tersendiri (`Slot1Service`..`Slot9Service` pada `:c1`..`:c9` di `turu/android/app/src/main/AndroidManifest.xml`), sehingga punya konteks GPU, thread engine, dan kuota RAM terpisah.
6. **Manajemen GPU Memory Proaktif (`turu/android/src/memory.rs`)**:
   - `ruffle_render_wgpu::set_memory_pressure(true)` (cap pool tekstur idle 64 MB).
   - Setiap **10 detik**: `device.poll(wgpu::PollType::Poll)` agar command buffer GPU yang sudah selesai segera dilepas.
   - Setiap **60 detik**: `ruffle_core::evict_unused_shapes(300)`.
   - Memantau `/proc/meminfo` (`MemAvailable < 15%`), budget RSS proses (`max(1 GB, 20% RAM)` / >30.000 mappings), event Android `onTrimMemory`, serta detach window → memanggil `evict_unused_shapes(120)` + `renderer.aqw_trim_memory()`.
   - Fitur manual `Free Memory Now`: memanggil `player.free_unused_libraries_now()` + `evict_unused_shapes(30)` + `aqw_trim_memory()`.
   - Saat window tidak tampil: `set_mouse_in_stage(false)` + `MouseLeave` untuk menghentikan hit-test display list Ruffle di background.
7. **Instrumentasi Performa (`turu/android/src/frame_perf.rs`)**:
   - Mengukur `game fps`, `rendered/s`, `skipped (game behind)`, `hitches` (gap ≥ 1.5× frame time), serta breakdown `Frames`, `Render`, `UI`, `Acquire`, dan `Present` per detik.

---

## 2. Akar Masalah Performa di `Grimlite-Ri` (FPS In-Game Stabil 24, Tapi Terasa Drop Berulang Saat Berjalan)

### 2.1 Penyebab Utama (Smoking Gun): Bug Anchor `next_frame` di `Grimlite-Ri/android/src/lib.rs:1765`

Pada `Grimlite-Ri/android/src/lib.rs:1710-1766`:

```rust
let now = Instant::now(); // t0: sebelum tick & render
...
let dt = FloatDuration::from_std(now.duration_since(last_tick));
last_tick = now;
...
p.tick(dt);
...
if (ran_frame && p.needs_render()) || needs_game_render {
    let frame_dur = Duration::from_secs_f64(1.0 / p.frame_rate().max(1.0));
    let behind = p.is_playing()
        && !needs_game_render
        && last_picture.elapsed() < MAX_DRAW_GAP
        && p.time_til_next_frame() * 2 < frame_dur;
    if !behind && surface.is_some() {
        p.render(); // misal makan 22 ms saat karakter berjalan
        last_picture = Instant::now();
        needs_game_render = false;
        should_present = true;
    }
}
next_frame = Instant::now() + p.time_til_next_frame(); // <-- BUG: pakai Instant::now(), bukan `now`!
```

Bandingkan dengan `turu/frontend/src/engine.rs:94` dan desktop `Grimlite-Ri/app/src/app.rs:1678`, yang keduanya memakai **`now + player.time_til_next_frame()`**.

#### Pembuktian Matematis pada 24 FPS (`frame_dur = 41.67 ms`)

Misalkan saat karakter berjalan (animasi avatar + dirty regions + blend layers), `p.tick()` + `p.render()` memakan waktu **22 ms** (masih jauh di bawah budget 41.67 ms — seharusnya lancar 24/24 FPS):

| Frame | Waktu Bangun (`t`) | `dt` | Sisa `frame_accumulator` setelah `tick` | `time_til_next_frame()` | Cek `behind` (`time_til * 2 < 41.67`) | Render & Present? | `next_frame` yang dihitung (Bug) |
|---|---|---|---|---|---|---|---|
| **F0** | `0.00 ms` | `41.67 ms` | `0.00 ms` | `41.67 ms` | `false` | **YA** (`+22 ms`) | `22.00 + 41.67 = 63.67 ms` |
| **F1** | `63.67 ms` | `63.67 ms` | **`22.00 ms`** | **`19.67 ms`** | **`true` (39.34 < 41.67)** | **SKIP!** (`+4 ms`) | `67.67 + 19.67 = 87.34 ms` |
| **F2** | `87.34 ms` | `23.67 ms` | `4.00 ms` | `37.67 ms` | `false` | **YA** (`+22 ms`) | `109.34 + 37.67 = 147.01 ms` |
| **F3** | `147.01 ms` | `59.67 ms` | **`22.00 ms`** | **`19.67 ms`** | **`true` (39.34 < 41.67)** | **SKIP!** (`+4 ms`) | `151.01 + 19.67 = 170.68 ms` |

**Akibat langsung:**
1. **Visual FPS anjlok 50% menjadi ~12 FPS dalam pola gergaji berulang (render → skip → render → skip)** begitu biaya `tick + render` melewati ~20.8 ms (padahal budget aslinya 41.67 ms).
2. **Counter FPS in-game AQW tetap terbaca stabil 24**, karena counter AQW menghitung jumlah `Event.ENTER_FRAME` di dalam `Player::tick()`, **bukan** jumlah frame yang benar-benar dirender dan dipresentasikan ke layar.
3. Bahkan saat `tick + render` di bawah 20.8 ms (tidak sampai men-trigger `behind`), setiap fluktuasi waktu render langsung menggeser `next_frame`, memicu **frame-pacing jitter** (interval antar gambar berubah-ubah, misal 50 ms → 32 ms → 54 ms).

### 2.1b Temuan Tambahan Saat Uji Emulator: False-Positive `behind` Akibat AS3 `Timer`

Saat verifikasi langsung di `emulator-5554` (Apple M5, `tick 0.3 ms`, `render 4.7 ms`, `draw 4.2 ms`), perf logger menemukan **16–17 frame tetap di-skip setiap 5 detik (~3.4 drop/detik secara periodik!)** meskipun total kerja per frame hanya ~9.2 ms dari budget 33.3 ms.

**Penyebabnya** ada pada `ruffle/core/src/player.rs:680-682`:
```rust
if let Some(time_til_next_timer) = self.time_til_next_timer {
    time_til_next = time_til_next.min(FloatDuration::from_millis(time_til_next_timer));
}
```
Setiap kali `flash.utils.Timer` / `setInterval` internal AQW kebetulan jatuh tempo dalam `< frame_dur / 2`, `Player::time_til_next_frame()` mengembalikan sisa waktu timer tersebut — bukan sisa waktu video frame! Akibatnya cek `til_next * 2 < frame_dur` **keliru mengira GPU tertinggal** dan secara periodik membuang 3–4 frame render tiap detik padahal `frame_accumulator == 0`.

**Solusi:** cek `p.time_til_next_timer()` untuk mendeteksi apakah `til_next` sedang ter-clamp oleh AS3 timer; jika ya, hanya anggap `behind` bila `dt >= 1.5 * frame_dur`.

### 2.2 Penyebab Penyerta Lainnya

1. **Default 24 FPS di Layar 60 Hz (*3:2 Pulldown Judder*) tanpa Frame Interpolation**
   - `Grimlite-Ri/host/src/main.rs:17` (`FRAME_RATE = 24`) dan `Grimlite-Ri/bot/src/engine/options.rs:116` (`fps: 24`).
   - Pada layar 60 Hz (16.67 ms/refresh), 24 FPS tampil bergantian 2 refresh (33.3 ms) dan 3 refresh (50 ms). Saat karakter berjalan kontinu, pola ini terlihat jelas sebagai getaran/drop periodik.
2. **OpenGL ES Dipaksa Jadi Default Tanpa Opsi Vulkan**
   - `Grimlite-Ri/android/src/lib.rs:716` meng-hardcode `[wgpu::Backends::GL, wgpu::Backends::VULKAN]`.
   - Overhead GLES pada wgpu (`submit` ~1.45× lebih lambat dari Vulkan, ~400 `eglMakeCurrent`/detik, tidak ada pipeline warm-up untuk kompilasi shader GLSL pertama kali, serta 2× full-screen blit per frame) membuat waktu render sangat mudah melampaui ambang 20.8 ms dan memicu Bug #1 terus-menerus.
3. **Touch `Move` Tidak Di-Coalesce (Flooding `MouseMove` Hit-Test 60–240 Hz)**
   - `Grimlite-Ri/android/src/lib.rs:1531-1533` & `1935-1970` mengirim `PlayerEvent::MouseMove` untuk **setiap** event `MotionAction::Move` dari Android.
   - Setiap `MouseMove` memicu hit-test seluruh DisplayList (`run_mouse_pick`) + event AS3, memperberat CPU persis ketika jari menahan/menggeser layar saat berjalan.
4. **Tidak Ada GPU Poll Berkala & Memory Trimming (`aqw_trim_memory` / `onTrimMemory`)**
   - `Grimlite-Ri/android/src/lib.rs` tidak pernah memanggil `device.poll(wgpu::PollType::Poll)`, tidak pernah memanggil `renderer.aqw_trim_memory()`, tidak memantau `/proc/meminfo`, dan tidak menangani `onTrimMemory` dari Android.
   - Command buffer selesai & tekstur idle menumpuk → tekanan memori Android (`kswapd`/zram) memicu stutter periodik setelah beberapa menit bermain.
5. **Overhead Per-Frame `egui` + `TabPlayerGuard` + AS3 `Main.as`**
   - `draw_screen()` membangun ulang `Vec<TabDescriptor>` (meng-clone `PlayerSnapshot` tiap tab) + mengeksekusi full `egui` UI & tessellasi di **setiap** frame game, lalu mengunci `TabPlayerGuard` di tengah render pass (`android/src/lib.rs:1239`) hanya untuk mengambil `movie.bind_group`.
   - `host/as3/grimlite/host/Main.as:290-326` memanggil `getPlayerState()` → `JSON.stringify()` → `ExternalInterface.call("playerState")` setiap 12 frame (0.5 detik) saat state berubah.
   - `scripts/build-android-apk.sh` belum melewatkan `--platform 26` ke `cargo ndk`, dan `android/app/build.gradle.kts` masih `isMinifyEnabled = false`.

---

## 3. Tabel Perbandingan Singkat: `turu` vs `Grimlite-Ri`

| Aspek | `turu` | `Grimlite-Ri` | Dampak Performa |
|---|---|---|---|
| **Submodule Ruffle** | `ruffle-aqw` (`d0e5bc01`) | `ruffle-aqw` (`882183aa` / `aqw-2.0.0-1`) | Setara (engine bukan bottleneck) |
| **Anchor `next_frame`** | `now + time_til_next_frame()` | `Instant::now() + time_til_next_frame()` | **Kritis** — siklus drop 24→12 FPS |
| **Default FPS Host** | `30 FPS` + Frame Interpolation | `24 FPS`, tanpa Frame Interpolation | *3:2 pulldown judder* di 60 Hz |
| **Graphics API** | Pilihan Auto / GLES / Vulkan + Perf Mode | Hardcode GLES dulu, tanpa pilihan | Overhead GLES memicu drop frame |
| **Touch `Move`** | `GameTouches`, skip egui move di game | Langsung `MouseMove` per event (60–240 Hz) | Flooding hit-test saat drag/jalan |
| **GPU & Memory Trim** | `device.poll(Poll)` 10s + `aqw_trim_memory` + `onTrimMemory` | Hanya `evict_unused_shapes(300)` tiap 60s | RAM & command buffer menumpuk |
| **Arsitektur UI/Tab** | Compose Shell + multi-proses (`:c1`..`:c9`) | 1 proses + `egui` di tiap frame game | Beban CPU & RAM lebih tinggi |
| **Build NDK / Gradle** | `--platform 26`, R8 minify & shrink | Tanpa `--platform 26`, R8 off | DEX lebih besar, startup lebih berat |
