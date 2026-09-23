//! macOS system monitor — user-space event collection without Endpoint Security.
//!
//! Lightweight by design: polling-based process snapshots, FSEvents-free
//! directory scans, and `netstat` TCP polling. No system extension, no
//! Endpoint Security entitlement, no approval dialogs — the agent stays a
//! plain user-space binary with a small idle footprint.

use osfm_edm_common::events::{FileOperation, NetworkProtocol, SystemEvent};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;

use super::MonitorConfig;

/// Poll interval for process snapshots (seconds).
const PROCESS_POLL_SECS: u64 = 5;
/// Poll interval for network table snapshots (seconds).
const NETWORK_POLL_SECS: u64 = 10;
/// Cap on files walked per directory scan (DoS/IO guard).
const MAX_FILES_PER_SCAN: usize = 5000;

/// Run the macOS system monitor.
pub async fn run_monitor(config: MonitorConfig, tx: mpsc::Sender<Vec<SystemEvent>>) {
    tracing::info!(
        paths = ?config.monitor_paths,
        interval = config.batch_interval_secs,
        categories = ?config.collect,
        "Starting macOS system monitor (user-space polling)"
    );

    let batch_interval = Duration::from_secs(config.batch_interval_secs.max(1));
    let (event_tx, mut event_rx) = mpsc::channel::<SystemEvent>(1024);

    if config.collect.iter().any(|c| c == "process") {
        let ptx = event_tx.clone();
        tokio::spawn(async move {
            poll_processes(ptx).await;
        });
    }
    if config.collect.iter().any(|c| c == "file") {
        let ftx = event_tx.clone();
        let paths = config.monitor_paths.clone();
        tokio::spawn(async move {
            poll_files(ftx, &paths).await;
        });
    }
    if config.collect.iter().any(|c| c == "network") {
        let ntx = event_tx.clone();
        tokio::spawn(async move {
            poll_network(ntx).await;
        });
    }
    drop(event_tx);

    let mut batch: Vec<SystemEvent> = Vec::new();
    let mut interval = tokio::time::interval(batch_interval);
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(e) => batch.push(e),
                    None => {
                        if !batch.is_empty() {
                            let _ = tx.send(std::mem::take(&mut batch)).await;
                        }
                        tracing::warn!("All macOS monitor sources exited");
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                if !batch.is_empty() {
                    let events = std::mem::take(&mut batch);
                    if tx.send(events).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}

/// Snapshot of a running process.
struct ProcInfo {
    pid: u32,
    ppid: u32,
    path: String,
    cmdline: String,
}

/// List processes via sysinfo (no Endpoint Security dependency).
fn list_processes() -> Vec<ProcInfo> {
    let sys = sysinfo::System::new_all();
    sys.processes()
        .iter()
        .map(|(pid, p)| ProcInfo {
            pid: pid.as_u32(),
            ppid: p.parent().map(|x| x.as_u32()).unwrap_or(0),
            path: p
                .exe()
                .map(|x| x.to_string_lossy().to_string())
                .unwrap_or_default(),
            cmdline: p.cmd().join(" "),
        })
        .collect()
}

async fn poll_processes(tx: mpsc::Sender<SystemEvent>) {
    let mut known: HashSet<u32> = HashSet::new();
    for p in list_processes() {
        known.insert(p.pid);
    }
    let mut interval = tokio::time::interval(Duration::from_secs(PROCESS_POLL_SECS));
    loop {
        interval.tick().await;
        let mut current = HashSet::new();
        let mut infos = Vec::new();
        for p in list_processes() {
            current.insert(p.pid);
            infos.push(p);
        }
        let timestamp = chrono::Utc::now().timestamp();
        for p in &infos {
            if !known.contains(&p.pid) {
                let event = SystemEvent::ProcessStarted {
                    pid: p.pid,
                    ppid: p.ppid,
                    path: p.path.clone(),
                    cmdline: p.cmdline.clone(),
                    user: None,
                    timestamp,
                };
                if tx.send(event).await.is_err() {
                    return;
                }
            }
        }
        for pid in known.difference(&current) {
            let event = SystemEvent::ProcessExited {
                pid: *pid,
                exit_code: -1,
                timestamp,
            };
            if tx.send(event).await.is_err() {
                return;
            }
        }
        known = current;
    }
}

/// File watcher: periodic mtime/size snapshot of watched roots.
async fn poll_files(tx: mpsc::Sender<SystemEvent>, paths: &[String]) {
    use std::collections::HashMap;
    let mut known: HashMap<String, (u64, u64)> = HashMap::new();
    let mut interval = tokio::time::interval(Duration::from_secs(PROCESS_POLL_SECS * 2));
    loop {
        interval.tick().await;
        let mut current: HashMap<String, (u64, u64)> = HashMap::new();
        for root in paths {
            walk_dir(std::path::Path::new(root), &mut current, 0);
            if current.len() >= MAX_FILES_PER_SCAN {
                break;
            }
        }
        let timestamp = chrono::Utc::now().timestamp();
        for (path, meta) in &current {
            match known.get(path) {
                None => {
                    let event = SystemEvent::FileAccessed {
                        pid: 0,
                        path: path.clone(),
                        operation: FileOperation::Create,
                        timestamp,
                    };
                    if tx.send(event).await.is_err() {
                        return;
                    }
                }
                Some(prev) if prev != meta => {
                    let event = SystemEvent::FileAccessed {
                        pid: 0,
                        path: path.clone(),
                        operation: FileOperation::Write,
                        timestamp,
                    };
                    if tx.send(event).await.is_err() {
                        return;
                    }
                }
                _ => {}
            }
        }
        for path in known.keys() {
            if !current.contains_key(path) {
                let event = SystemEvent::FileAccessed {
                    pid: 0,
                    path: path.clone(),
                    operation: FileOperation::Delete,
                    timestamp,
                };
                if tx.send(event).await.is_err() {
                    return;
                }
            }
        }
        known = current;
    }
}

fn walk_dir(
    dir: &std::path::Path,
    out: &mut std::collections::HashMap<String, (u64, u64)>,
    depth: usize,
) {
    if depth > 6 || out.len() >= MAX_FILES_PER_SCAN {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip the heaviest Apple system trees to keep scans cheap.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if matches!(
                    name,
                    "System"
                        | "Library"
                        | ".DocumentRevisions-V100"
                        | ".Spotlight-V100"
                        | ".Trashes"
                ) {
                    continue;
                }
            }
            walk_dir(&path, out, depth + 1);
        } else if let Ok(md) = entry.metadata() {
            let mtime = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.insert(path.to_string_lossy().to_string(), (mtime, md.len()));
        }
        if out.len() >= MAX_FILES_PER_SCAN {
            return;
        }
    }
}

/// Network watcher: `netstat -an -p tcp` snapshot, new-connection events.
async fn poll_network(tx: mpsc::Sender<SystemEvent>) {
    let mut known: HashSet<String> = HashSet::new();
    let mut interval = tokio::time::interval(Duration::from_secs(NETWORK_POLL_SECS));
    loop {
        interval.tick().await;
        let current = tcp_connections();
        let timestamp = chrono::Utc::now().timestamp();
        for key in &current {
            if !known.contains(key) {
                let mut parts = key.splitn(2, "->");
                let src = parts.next().unwrap_or("").to_string();
                let dst = parts.next().unwrap_or("").to_string();
                let event = SystemEvent::NetworkConnected {
                    pid: 0,
                    src,
                    dst,
                    protocol: NetworkProtocol::Tcp,
                    timestamp,
                };
                if tx.send(event).await.is_err() {
                    return;
                }
            }
        }
        known = current;
    }
}

fn tcp_connections() -> HashSet<String> {
    let mut out = HashSet::new();
    let output = std::process::Command::new("netstat")
        .args(["-an", "-p", "tcp"])
        .output();
    let Ok(output) = output else { return out };
    if !output.status.success() {
        return out;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 || parts[0] != "tcp4" && parts[0] != "tcp6" && parts[0] != "tcp" {
            continue;
        }
        if parts[5] != "ESTABLISHED" {
            continue;
        }
        out.insert(format!("{}->{}", parts[3], parts[4]));
    }
    out
}
