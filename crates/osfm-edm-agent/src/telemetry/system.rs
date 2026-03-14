//! System telemetry — collects CPU, RAM, disk, and uptime metrics.

use osfm_edm_common::protocol::TelemetrySnapshot;
use sysinfo::System;

/// Collect a point-in-time system telemetry snapshot.
pub fn collect_snapshot() -> TelemetrySnapshot {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_pct = sys.global_cpu_info().cpu_usage() as f64;

    let ram_total_mb = sys.total_memory() / (1024 * 1024);
    let ram_used_mb = sys.used_memory() / (1024 * 1024);

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
