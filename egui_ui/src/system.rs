//! Lightweight, zero-dependency cross-platform process resource monitor (CPU %, RAM, Uptime).
//! Polls only on demand (throttled to 1 Hz when the About window is open).

use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct ResourceSnapshot {
    pub cpu_percent: f32,
    pub ram_mb: f32,
    pub peak_ram_mb: f32,
    pub uptime_secs: u64,
}

pub struct ResourceMonitor {
    started_at: Instant,
    last_poll: Option<Instant>,
    last_cpu_us: Option<u64>,
    peak_ram_mb: f32,
    latest: ResourceSnapshot,
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            last_poll: None,
            last_cpu_us: None,
            peak_ram_mb: 0.0,
            latest: ResourceSnapshot {
                cpu_percent: 0.0,
                ram_mb: 0.0,
                peak_ram_mb: 0.0,
                uptime_secs: 0,
            },
        }
    }
}

impl ResourceMonitor {
    /// Refreshes process CPU & RAM at most once per `interval` and returns the latest snapshot.
    pub fn sample(&mut self, interval: Duration) -> &ResourceSnapshot {
        let now = Instant::now();
        let should_poll = self
            .last_poll
            .is_none_or(|prev| now.duration_since(prev) >= interval);

        if should_poll {
            let ram_bytes = current_rss_bytes();
            let ram_mb = ram_bytes as f32 / (1024.0 * 1024.0);
            if ram_mb > self.peak_ram_mb {
                self.peak_ram_mb = ram_mb;
            }

            let cpu_us = current_cpu_time_us();
            let cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1) as f64;
            let cpu_percent = match (self.last_poll, self.last_cpu_us) {
                (Some(prev_t), Some(prev_cpu)) => {
                    let wall_us = now.duration_since(prev_t).as_micros() as u64;
                    if wall_us > 0 && cpu_us >= prev_cpu {
                        ((cpu_us - prev_cpu) as f64 / (wall_us as f64 * cores) * 100.0) as f32
                    } else {
                        self.latest.cpu_percent
                    }
                }
                _ => 0.0,
            };

            self.last_poll = Some(now);
            self.last_cpu_us = Some(cpu_us);
            self.latest = ResourceSnapshot {
                cpu_percent: cpu_percent.clamp(0.0, 100.0),
                ram_mb,
                peak_ram_mb: self.peak_ram_mb,
                uptime_secs: self.started_at.elapsed().as_secs(),
            };
        }

        &self.latest
    }
}

// ---- macOS implementation (mach task_info + getrusage) ----
#[cfg(target_os = "macos")]
fn current_rss_bytes() -> u64 {
    #[repr(C)]
    struct TimeValue {
        seconds: i32,
        microseconds: i32,
    }
    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: TimeValue,
        system_time: TimeValue,
        policy: i32,
        suspend_count: i32,
    }
    const MACH_TASK_BASIC_INFO: u32 = 20;

    unsafe extern "C" {
        static mach_task_self_: u32;
        fn task_info(
            target_task: u32,
            flavor: u32,
            task_info_out: *mut MachTaskBasicInfo,
            task_info_out_cnt: *mut u32,
        ) -> i32;
    }

    let mut info = MachTaskBasicInfo {
        virtual_size: 0,
        resident_size: 0,
        resident_size_max: 0,
        user_time: TimeValue {
            seconds: 0,
            microseconds: 0,
        },
        system_time: TimeValue {
            seconds: 0,
            microseconds: 0,
        },
        policy: 0,
        suspend_count: 0,
    };
    let mut count = (std::mem::size_of::<MachTaskBasicInfo>() / std::mem::size_of::<u32>()) as u32;
    let kr = unsafe { task_info(mach_task_self_, MACH_TASK_BASIC_INFO, &mut info, &mut count) };
    if kr == 0 { info.resident_size } else { 0 }
}

#[cfg(target_os = "macos")]
fn current_cpu_time_us() -> u64 {
    #[repr(C)]
    struct Timeval {
        tv_sec: i64,
        tv_usec: i32,
        _pad: i32,
    }
    #[repr(C)]
    struct Rusage {
        ru_utime: Timeval,
        ru_stime: Timeval,
        _rest: [i64; 14],
    }
    const RUSAGE_SELF: i32 = 0;

    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }

    let mut usage: Rusage = unsafe { std::mem::zeroed() };
    if unsafe { getrusage(RUSAGE_SELF, &mut usage) } == 0 {
        let user = (usage.ru_utime.tv_sec as u64)
            .saturating_mul(1_000_000)
            .saturating_add(usage.ru_utime.tv_usec.max(0) as u64);
        let sys = (usage.ru_stime.tv_sec as u64)
            .saturating_mul(1_000_000)
            .saturating_add(usage.ru_stime.tv_usec.max(0) as u64);
        user.saturating_add(sys)
    } else {
        0
    }
}

// ---- Windows implementation (Kernel32 GetProcessTimes + K32GetProcessMemoryInfo) ----
#[cfg(target_os = "windows")]
fn current_rss_bytes() -> u64 {
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut core::ffi::c_void,
            ppsmem_counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut pmc: ProcessMemoryCounters = unsafe { std::mem::zeroed() };
    pmc.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) };
    if ok != 0 {
        pmc.working_set_size as u64
    } else {
        0
    }
}

#[cfg(target_os = "windows")]
fn current_cpu_time_us() -> u64 {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn GetProcessTimes(
            process: *mut core::ffi::c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }

    let mut creation = FileTime { low: 0, high: 0 };
    let mut exit = FileTime { low: 0, high: 0 };
    let mut kernel = FileTime { low: 0, high: 0 };
    let mut user = FileTime { low: 0, high: 0 };
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    if ok != 0 {
        let k = ((kernel.high as u64) << 32) | (kernel.low as u64);
        let u = ((user.high as u64) << 32) | (user.low as u64);
        // FILETIME is in 100-nanosecond intervals -> divide by 10 for microseconds
        k.saturating_add(u) / 10
    } else {
        0
    }
}

// ---- Linux / Android implementation (/proc/self) ----
#[cfg(any(target_os = "linux", target_os = "android"))]
fn current_rss_bytes() -> u64 {
    let Ok(statm) = std::fs::read_to_string("/proc/self/statm") else {
        return 0;
    };
    let pages = statm
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    pages.saturating_mul(4096)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn current_cpu_time_us() -> u64 {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return 0;
    };
    let Some((_, after_comm)) = stat.rsplit_once(')') else {
        return 0;
    };
    let mut fields = after_comm.split_whitespace();
    let utime = fields
        .nth(11)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let stime = fields
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    // Standard Linux USER_HZ = 100 ticks/sec -> 10_000 us per tick
    utime.saturating_add(stime).saturating_mul(10_000)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux", target_os = "android")))]
fn current_rss_bytes() -> u64 {
    0
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux", target_os = "android")))]
fn current_cpu_time_us() -> u64 {
    0
}
