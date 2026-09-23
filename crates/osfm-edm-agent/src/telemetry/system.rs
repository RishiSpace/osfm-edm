//! System telemetry — collects CPU, RAM, disk, and uptime metrics.
//!
//! Lightweight: one reusable sysinfo handle with minimum refresh intervals so
//! a 60s heartbeat does not re-walk the whole process table or disk list.

use osfm_edm_common::protocol::TelemetrySnapshot;
use std::sync::Mutex;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

/// Reused across heartbeats; sysinfo keeps per-CPU state for accurate deltas.
static SYS: Mutex<Option<System>> = Mutex::new(None);

fn refresh_kind() -> RefreshKind {
    RefreshKind::new()
        .with_cpu(CpuRefreshKind::everything())
        .with_memory(MemoryRefreshKind::everything())
}

/// Collect a point-in-time system telemetry snapshot.
pub fn collect_snapshot() -> TelemetrySnapshot {
    let cpu_pct = {
        let mut guard = SYS.lock().expect("telemetry lock");
        let sys = guard.get_or_insert_with(|| System::new_with_specifics(refresh_kind()));
        sys.refresh_specifics(refresh_kind());
        // Second sample after the minimum interval so cpu_usage() is a real delta.
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        sys.global_cpu_info().cpu_usage() as f64
    };

    let (ram_total_mb, ram_used_mb) = {
        let guard = SYS.lock().expect("telemetry lock");
        let sys = guard.as_ref().expect("telemetry init");
        (
            sys.total_memory() / (1024 * 1024),
            sys.used_memory() / (1024 * 1024),
        )
    };

    // Sum all disk space.
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let (disk_total, disk_used) = disks.iter().fold((0u64, 0u64), |(total, used), d| {
        (
            total + d.total_space(),
            used + (d.total_space() - d.available_space()),
        )
    });
    let disk_total_gb = disk_total as f64 / (1024.0 * 1024.0 * 1024.0);
    let disk_used_gb = disk_used as f64 / (1024.0 * 1024.0 * 1024.0);

    let uptime_secs = System::uptime();

    let timestamp = chrono::Utc::now().timestamp();

    TelemetrySnapshot {
        cpu_pct,
        ram_used_mb,
        ram_total_mb,
        disk_used_gb,
        disk_total_gb,
        uptime_secs,
        timestamp,
    }
}
