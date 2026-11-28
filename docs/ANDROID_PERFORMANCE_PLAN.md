# Rencana Perbaikan Performa Android (`Grimlite-Ri`)

Dokumen ini adalah rencana eksekusi bertahap berdasarkan hasil analisis di [ANDROID_PERFORMANCE_ANALYSIS.md](./ANDROID_PERFORMANCE_ANALYSIS.md). Target utamanya adalah menghilangkan drop FPS berulang saat berjalan, menyetarakan stabilitas frame pacing & manajemen memori GPU dengan `turu`, serta menyediakan metrik terukur untuk verifikasi di perangkat Android nyata.

---

## Target Keberhasilan (Definition of Done)

- **Rendered FPS == Game FPS** (misal 30/30 FPS di area normal, tanpa siklus sawtooth drop 50% ketika `tick + render` < budget frame).
- **Hitches ≈ 0** (gap antar frame ≥ 1.5× frame time hanya terjadi saat load map baru, bukan berulang saat berjalan).
- **Touch/drag responsif** tanpa lonjakan CPU hit-test di 60–240 Hz.
- **Memori stabil** dalam sesi panjang (GPU command buffer & pool tekstur idle rutin dibersihkan).
- **Build APK release bersih** dengan `--platform 26` dan R8 minification aktif tanpa crash JNI.

---

## Phase 1 (P0 — Kritis): Perbaiki Frame-Pacing Loop & Tambah Perf Logger

**Tujuan:** Menghilangkan akar penyebab utama drop FPS berulang (24 → 12 FPS visual) dan memasang alat ukur real-time di `logcat`.

### Perubahan
1. **[`android/src/lib.rs`](../android/src/lib.rs)** — Perbaiki anchor `next_frame` dan alur `tick` → `render` → `draw_screen`:
   - Ganti `next_frame = Instant::now() + p.time_til_next_frame();` (baris ~1765) menjadi:
     ```rust
     next_frame = now + p.time_til_next_frame();
     ```
     (diukur dari `now` **sebelum** `p.tick(dt)`, `p.render()`, dan `draw_screen()`, sama persis dengan `turu/frontend/src/engine.rs:94` dan `Grimlite-Ri/app/src/app.rs:1678`).
   - Pastikan `draw_screen()` dijalankan sebagai bagian dari langkah frame aktif sebelum menghitung sisa waktu tidur di inner loop, dan ketika `behind == true` (render di-skip), jangan sampai `ui_ready` langsung memicu `draw_screen()` tambahan yang mencuri slot vsync frame berikutnya.
   - Hindari `TabPlayerGuard::try_lock` ganda di dalam `draw_screen()` (baris ~1239): jangan menukar thread-local `ruffle_render_wgpu::segments` lewat Mutex hanya untuk membaca `movie.bind_group`.
2. **[`android/src/lib.rs`](../android/src/lib.rs)** (atau modul kecil `android/src/perf.rs`) — Tambah ringkasan performa per 5 detik ke logcat (tag `GrimliteRi`, meniru `turu/android/src/frame_perf.rs`):
   - Mencatat: `game_fps`, `rendered_fps`, `skipped_behind`, `hitches` (gap ≥ 1.5× frame dur), `worst_gap_ms`, serta rata-rata & max `tick_ms`, `render_ms`, `ui_ms`, `acquire_ms`, `present_ms`.

### Verifikasi Phase 1
- Build & jalankan di perangkat Android, pantau `adb logcat -s GrimliteRi | grep "perf:"`.
- Pastikan saat berjalan di map normal, `rendered_fps` menyamai `game_fps` dan `skipped_behind == 0`.

---

## Phase 2 (P0 — Kritis): Default 30 FPS & Touch Move Coalescing

**Tujuan:** Menghilangkan *3:2 pulldown judder* di layar 60 Hz dan menghentikan flooding hit-test Ruffle saat jari menahan/menggeser layar.

### Perubahan
1. **Default 30 FPS (menyamai `turu` / `skua.swf`)**:
   - [`host/src/main.rs`](../host/src/main.rs): ubah `const FRAME_RATE: u8 = 24;` → `30;`.
   - Rebuild embedded SWF: `cargo run -p grimlite_host` → memperbarui [`bot/assets/grimlite.swf`](../bot/assets/grimlite.swf).
   - [`bot/src/engine/options.rs`](../bot/src/engine/options.rs): ubah `BotOptions::default().fps` dari `24` → `30`.
   - [`egui_ui/src/lib.rs`](../egui_ui/src/lib.rs): pertahankan slider Stage FPS (pertimbangkan preset cepat `24 / 30 / 60`).
2. **Coalesce `TouchKind::Move` di [`android/src/lib.rs`](../android/src/lib.rs)**:
   - Jangan langsung memanggil `p.handle_event(PlayerEvent::MouseMove { x, y })` pada setiap `EngineMsg::Touch { kind: TouchKind::Move, .. }` di inner loop.
   - Simpan `pending_game_move: Option<(f64, f64)>` yang selalu ditimpa koordinat terbaru saat menguras `engine_rx`, lalu flush maksimal **1×** tepat sebelum `p.tick(dt)` (dan sebelum `TouchKind::Down` / `TouchKind::Up` agar urutan klik tetap akurat).

### Verifikasi Phase 2
- Berjalan menggunakan tap dan tahan/geser joystick: cek `tick_ms` tidak melonjak saat drag jari cepat, dan gerakan karakter di layar 60 Hz mulus tanpa judder 3:2.

---

## Phase 3 (P1 — Tinggi): Pilihan Graphics API (Vulkan / GLES), Render Scale, & Counter Rendered FPS

**Tujuan:** Memberi pengguna kontrol penuh seperti `turu` untuk memilih Vulkan di perangkat yang drivernya stabil (menghindari overhead CPU GLES ~1.45× dan 2× full-screen blit) serta mengatur skala resolusi render.

### Perubahan
1. **Persisted Graphics API Choice (`Auto` / `Vulkan` / `OpenGL ES`)**:
   - Simpan pilihan di `data_dir/graphics.txt` (default tetap aman; pengguna bisa memilih `Vulkan` untuk performa maksimal di GPU Adreno/Mali modern, atau `OpenGL ES` jika driver Vulkan bermasalah).
   - Baca pilihan di `create_mobile_gpu()` ([`android/src/lib.rs`](../android/src/lib.rs)) sebelum memilih backend.
   - Tambahkan selector Graphics API + Render Quality/Scale (`Native`, `660p (Balanced)`, `540p (Fast)`) + toggle **Show Rendered FPS** di `Game Options` ([`egui_ui/src/lib.rs`](../egui_ui/src/lib.rs)).
2. **Overlay Rendered FPS (opsional)**:
   - Tampilkan badge kecil `Rendered FPS` (jumlah gambar yang benar-benar dipresentasikan per detik) agar mudah dibandingkan dengan FPS in-game AQW.
3. **(Opsional Phase 3b) Frame Interpolation**:
   - Port `Pacing` dari `turu/frontend/src/pacing.rs`, aktifkan `ruffle_core::frame_interpolation`, dan kirim display refresh rate dari `MainActivity.kt` via `DisplayManager.DisplayListener` + JNI untuk animasi 60/90/120 Hz mulus.

### Verifikasi Phase 3
- A/B test `Vulkan` vs `OpenGL ES` di map ramai (misal Battleon / Yulgar) menggunakan perf log Phase 1: bandingkan `render_ms`, `present_ms`, dan `rendered_fps`.

---

## Phase 4 (P1 — Tinggi): Manajemen GPU Memory & Android Lifecycle (`onTrimMemory`)

**Tujuan:** Mencegah stutter jangka panjang, penumpukan command buffer GPU, dan OOM kill Android.

### Perubahan
1. **Periodic GPU Poll & Memory Watch di [`android/src/lib.rs`](../android/src/lib.rs)**:
   - Setiap **10 detik**: panggil `desc.device.poll(wgpu::PollType::Poll)` (non-blocking) agar command buffer GPU yang selesai dilepas.
   - Setiap **10 detik**: baca `/proc/meminfo`; jika `MemAvailable < 15%` (dengan cooldown 30 detik), panggil `evict_unused_shapes(120)` + `renderer.aqw_trim_memory()`.
   - Pada `EngineMsg::DetachWindow`: selain `evict_unused_shapes(120)`, tambahkan `renderer.aqw_trim_memory()` untuk membebaskan pool tekstur idle saat ke background.
2. **Hook `onTrimMemory` dari Kotlin → Rust**:
   - [`android/app/src/main/java/com/grimlite/ri/MainActivity.kt`](../android/app/src/main/java/com/grimlite/ri/MainActivity.kt) & [`GrimliteService.kt`](../android/app/src/main/java/com/grimlite/ri/GrimliteService.kt): override `onTrimMemory(level)` dan kirim `nativeTrimMemory(level)` ketika `level >= TRIM_MEMORY_RUNNING_LOW`.
   - Rust menangani `EngineMsg::TrimMemory(level)` dengan `evict_unused_shapes(120)` + `renderer.aqw_trim_memory()`.
3. **Tombol "Free Memory Now" di Game Options**:
   - Memicu `player.free_unused_libraries_now()` + `evict_unused_shapes(30)` + `renderer.aqw_trim_memory()`.

### Verifikasi Phase 4
- Uji dengan `adb shell am send-trim-memory com.grimlite.ri RUNNING_LOW` dan sesi farming/pindah map >15 menit; pastikan RSS stabil dan tidak ada stutter swap.

---

## Phase 5 (P2 — Menengah): Optimasi Build APK, Pengurangan Overhead UI/AS3 & Evaluasi Jangka Panjang

### Perubahan
1. **Build Script & Gradle Hardening**:
   - [`scripts/build-android-apk.sh`](../scripts/build-android-apk.sh): tambahkan `--platform 26` pada perintah `cargo ndk`.
   - [`android/app/build.gradle.kts`](../android/app/build.gradle.kts): aktifkan `isMinifyEnabled = true` dan `isShrinkResources = true` pada `release`, disertai `android/app/proguard-rules.pro` yang menjaga seluruh JNI entry points (`com.grimlite.ri.**`, `com.google.androidgamesdk.**`, `org.rustls.platformverifier.**`).
2. **Kurangi Overhead Rutin per Frame**:
   - Cache `Vec<TabDescriptor>` atau hindari cloning `PlayerSnapshot` dua kali per frame di `draw_screen()` ([`android/src/lib.rs`](../android/src/lib.rs)).
   - Evaluasi frekuensi `pushPlayerStateIfChanged()` di [`host/as3/grimlite/host/Main.as`](../host/as3/grimlite/host/Main.as) agar tidak melakukan `JSON.stringify` penuh jika tidak ada perubahan properti penting.
3. **Evaluasi Jangka Panjang (Opsional)**:
   - Jika multi-tab Android (2–4 akun) menjadi prioritas utama dan terbentur batas RAM/GPU 1 proses, adopsi pola multi-proses `SlotService` (`:c1`..`:c4`) milik `turu`.

### Verifikasi Phase 5
- Jalankan `./scripts/build-android-apk.sh --release --abi=arm64-v8a`.
- Verifikasi APK release terpasang, seluruh callback JNI (`openDocument`, `saveDocument`, `updateBotService`, `nativeTrimMemory`, `nativeStopAllBots`, TLS `rustls-platform-verifier`) berjalan normal tanpa `ClassNotFoundException` / `NoSuchMethodError` setelah R8.

---

## Urutan Eksekusi yang Disarankan

1. **Batch 1 (Langsung terasa efeknya):** Phase 1 + Phase 2 (Fix `next_frame` + perf log + default 30 FPS + touch move coalescing).
2. **Batch 2 (Stabilitas & fleksibilitas GPU):** Phase 3 + Phase 4 (Pilihan Vulkan/GLES + GPU poll + `aqw_trim_memory` + `onTrimMemory`).
3. **Batch 3 (Polish build & efisiensi):** Phase 5 (`--platform 26`, R8 + ProGuard rules, optimasi `draw_screen`/AS3).
