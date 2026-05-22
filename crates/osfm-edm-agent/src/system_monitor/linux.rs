//! Linux system monitor — user-space event collection via procfs, netlink, and fanotify.
//!
//! Requires root/CAP_NET_ADMIN for proc connector and CAP_SYS_ADMIN for fanotify.
//!
//! ## Monitoring methods
//!
//! - **Process events**: Linux proc connector (`NETLINK_CONNECTOR` / `CN_IDX_PROC`)
//!   provides real-time fork/exec/exit notifications from the kernel without eBPF.
//!   Falls back to `/proc` scanning if the netlink socket cannot be opened.
//!
//! - **File events**: `fanotify` provides per-mount filesystem event notifications
//!   with PID attribution. More capable than inotify for whole-filesystem monitoring.
//!
//! - **Network events**: Periodic parsing of `/proc/net/tcp` and `/proc/net/tcp6`
//!   with PID→socket inode resolution via `/proc/<pid>/fd/`.

use osfm_edm_common::events::{FileOperation, NetworkProtocol, SystemEvent};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::time::Duration;
use tokio::sync::mpsc;

use super::MonitorConfig;

/// Main monitor loop — collects events from all enabled sources and flushes
/// batches to the provided sender at the configured interval.
pub async fn run_monitor(config: MonitorConfig, tx: mpsc::Sender<Vec<SystemEvent>>) {
    tracing::info!(
        paths = ?config.monitor_paths,
        interval = config.batch_interval_secs,
        categories = ?config.collect,
        "Starting Linux system monitor (user-space)"
    );

    let batch_interval = Duration::from_secs(config.batch_interval_secs);
    let (event_tx, mut event_rx) = mpsc::channel::<SystemEvent>(1024);

    // Spawn process monitor if requested.
    if config.collect.iter().any(|c| c == "process") {
        let ptx = event_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = monitor_processes(ptx).await {
                tracing::warn!(error = %e, "Process monitor exited — falling back to polling");
            }
        });
    }

    // Spawn file monitor if requested.
    if config.collect.iter().any(|c| c == "file") {
        let ftx = event_tx.clone();
        let paths = config.monitor_paths.clone();
        tokio::spawn(async move {
            if let Err(e) = monitor_files(ftx, &paths).await {
                tracing::warn!(error = %e, "File monitor exited");
            }
        });
    }

    // Spawn network monitor if requested.
    if config.collect.iter().any(|c| c == "network") {
        let ntx = event_tx.clone();
        tokio::spawn(async move {
            monitor_network(ntx).await;
        });
    }

    // Drop our copy so the channel closes when all producers exit.
    drop(event_tx);

    // Batch collector — accumulates events and flushes at the configured interval.
    let mut batch: Vec<SystemEvent> = Vec::new();
    let mut interval = tokio::time::interval(batch_interval);

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(e) => batch.push(e),
                    None => {
                        // All producers exited.
                        if !batch.is_empty() {
                            let _ = tx.send(std::mem::take(&mut batch)).await;
                        }
                        tracing::warn!("All monitor sources exited");
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                if !batch.is_empty() {
                    let events = std::mem::take(&mut batch);
                    tracing::debug!(count = events.len(), "Flushing system event batch");
                    if tx.send(events).await.is_err() {
                        tracing::error!("Event batch receiver dropped");
                        break;
                    }
                }
            }
        }
    }
}

// ─── Process Monitoring via Proc Connector ───────────────────────────────────

/// Monitor process events using the Linux proc connector (netlink).
///
/// The proc connector is a netlink-based mechanism that delivers real-time
/// notifications for process lifecycle events (fork, exec, exit) without
/// polling. Requires root or CAP_NET_ADMIN.
async fn monitor_processes(tx: mpsc::Sender<SystemEvent>) -> anyhow::Result<()> {
    use std::os::unix::io::AsRawFd;

    // Create a netlink socket for the proc connector.
    let sock = match create_proc_connector_socket() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Cannot open proc connector (need root) — falling back to /proc polling"
            );
            return poll_processes_fallback(tx).await;
        }
    };

    // Subscribe to proc events.
    if let Err(e) = subscribe_proc_events(sock.as_raw_fd()) {
        tracing::warn!(error = %e, "Cannot subscribe to proc events — falling back to /proc polling");
        return poll_processes_fallback(tx).await;
    }

    tracing::info!("Process monitor active via proc connector (netlink)");

    // Read events in a blocking thread (netlink recv is blocking).
    let fd = sock.as_raw_fd();
    let (read_tx, mut read_rx) = mpsc::channel::<ProcEvent>(256);

    // Spawn a blocking thread to read from the netlink socket.
    std::thread::spawn(move || {
        read_proc_events(fd, read_tx);
    });

    // Don't drop the socket while the reader thread is using it.
    let _sock_guard = sock;

    while let Some(proc_event) = read_rx.recv().await {
        let timestamp = chrono::Utc::now().timestamp();

        let event = match proc_event {
            ProcEvent::Exec { pid } => {
                let (path, cmdline, ppid, user) = read_process_info(pid);
                SystemEvent::ProcessStarted {
                    pid,
                    ppid,
                    path,
                    cmdline,
                    user,
                    timestamp,
                }
            }
            ProcEvent::Exit { pid, exit_code } => SystemEvent::ProcessExited {
                pid,
                exit_code,
                timestamp,
            },
        };

        if tx.send(event).await.is_err() {
            break;
        }
    }

    Ok(())
}

/// Internal proc event representation.
enum ProcEvent {
    Exec { pid: u32 },
    Exit { pid: u32, exit_code: i32 },
}

/// Create a netlink socket for the proc connector.
fn create_proc_connector_socket() -> anyhow::Result<std::os::unix::net::UnixDatagram> {
    use std::mem;

    // NETLINK_CONNECTOR = 11
    const NETLINK_CONNECTOR: libc::c_int = 11;

    let fd = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM,
            NETLINK_CONNECTOR,
        )
    };

    if fd < 0 {
        return Err(anyhow::anyhow!(
            "socket(AF_NETLINK, SOCK_DGRAM, NETLINK_CONNECTOR) failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    // Bind to CN_IDX_PROC group.
    let mut addr: libc::sockaddr_nl = unsafe { mem::zeroed() };
    addr.nl_family = libc::AF_NETLINK as u16;
    addr.nl_pid = std::process::id();
    addr.nl_groups = 0x1; // CN_IDX_PROC

    let ret = unsafe {
        libc::bind(
            fd,
            &addr as *const libc::sockaddr_nl as *const libc::sockaddr,
            mem::size_of::<libc::sockaddr_nl>() as u32,
        )
    };

    if ret < 0 {
        unsafe { libc::close(fd) };
        return Err(anyhow::anyhow!(
            "bind(netlink) failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    // Wrap in a UnixDatagram for ownership/drop semantics.
    // This is safe because we own the fd.
    let sock = unsafe { std::os::unix::net::UnixDatagram::from_raw_fd(fd) };
    Ok(sock)
}

use std::os::unix::io::FromRawFd;

/// Subscribe to proc connector events by sending PROC_CN_MCAST_LISTEN.
fn subscribe_proc_events(fd: std::os::unix::io::RawFd) -> anyhow::Result<()> {
    // The proc connector subscription message is a netlink message containing
    // a connector message (cn_msg) with a PROC_CN_MCAST_LISTEN operation.

    #[repr(C)]
    struct ProcCnMcastOp {
        nl_hdr: libc::nlmsghdr,
        cn_msg_id_idx: u32,
        cn_msg_id_val: u32,
        cn_msg_seq: u32,
        cn_msg_ack: u32,
        cn_msg_len: u16,
        cn_msg_flags: u16,
        mcast_op: u32,
    }

    let mut msg = std::mem::MaybeUninit::<ProcCnMcastOp>::zeroed();
    let msg = unsafe {
        let m = msg.as_mut_ptr();
        (*m).nl_hdr.nlmsg_len = std::mem::size_of::<ProcCnMcastOp>() as u32;
        (*m).nl_hdr.nlmsg_type = 0; // NLMSG_DONE
        (*m).nl_hdr.nlmsg_flags = 0;
        (*m).nl_hdr.nlmsg_seq = 0;
        (*m).nl_hdr.nlmsg_pid = std::process::id();
        (*m).cn_msg_id_idx = 0x1; // CN_IDX_PROC
        (*m).cn_msg_id_val = 0x1; // CN_VAL_PROC
        (*m).cn_msg_seq = 0;
        (*m).cn_msg_ack = 0;
        (*m).cn_msg_len = 4; // sizeof(mcast_op)
        (*m).cn_msg_flags = 0;
        (*m).mcast_op = 1; // PROC_CN_MCAST_LISTEN
        msg.assume_init()
    };

    let ret = unsafe {
        libc::send(
            fd,
            &msg as *const ProcCnMcastOp as *const libc::c_void,
            std::mem::size_of::<ProcCnMcastOp>(),
            0,
        )
    };

    if ret < 0 {
        return Err(anyhow::anyhow!(
            "send(PROC_CN_MCAST_LISTEN) failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

/// Read proc connector events from the netlink socket (blocking).
/// Runs in a dedicated OS thread.
fn read_proc_events(fd: std::os::unix::io::RawFd, tx: mpsc::Sender<ProcEvent>) {
    let mut buf = [0u8; 4096];

    loop {
        let len = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0) };

        if len <= 0 {
            tracing::debug!("Proc connector recv returned {len}, exiting reader");
            break;
        }

        let len = len as usize;
        if len < std::mem::size_of::<libc::nlmsghdr>() {
            continue;
        }

        // Parse the proc event from the netlink message.
        // The layout is: nlmsghdr + cn_msg header (20 bytes) + proc_event
        let hdr_size = std::mem::size_of::<libc::nlmsghdr>();
        let cn_msg_header_size = 20; // cb_id(8) + seq(4) + ack(4) + len(2) + flags(2)
        let proc_event_offset = hdr_size + cn_msg_header_size;

        if len < proc_event_offset + 4 {
            continue;
        }

        // proc_event.what is the first u32 of the proc_event struct.
        let what = u32::from_ne_bytes([
            buf[proc_event_offset],
            buf[proc_event_offset + 1],
            buf[proc_event_offset + 2],
            buf[proc_event_offset + 3],
        ]);

        // Event data starts after the `what` field + cpu(4) + timestamp(8) = 16 bytes
        let event_data_offset = proc_event_offset + 4 + 4 + 8;

        // PROC_EVENT_EXEC = 0x00000002
        // PROC_EVENT_EXIT = 0x80000000
        match what {
            0x00000002 => {
                // exec event: process_pid(4) + process_tgid(4)
                if len >= event_data_offset + 8 {
                    let pid = u32::from_ne_bytes([
                        buf[event_data_offset],
                        buf[event_data_offset + 1],
                        buf[event_data_offset + 2],
                        buf[event_data_offset + 3],
                    ]);
                    if tx.blocking_send(ProcEvent::Exec { pid }).is_err() {
                        break;
                    }
                }
            }
            0x80000000u32 => {
                // exit event: process_pid(4) + process_tgid(4) + exit_code(4) + exit_signal(4)
                if len >= event_data_offset + 16 {
                    let pid = u32::from_ne_bytes([
                        buf[event_data_offset],
                        buf[event_data_offset + 1],
                        buf[event_data_offset + 2],
                        buf[event_data_offset + 3],
                    ]);
                    let exit_code = i32::from_ne_bytes([
                        buf[event_data_offset + 8],
                        buf[event_data_offset + 9],
                        buf[event_data_offset + 10],
                        buf[event_data_offset + 11],
                    ]);
                    if tx
                        .blocking_send(ProcEvent::Exit { pid, exit_code })
                        .is_err()
                    {
                        break;
                    }
                }
            }
            _ => {
                // Ignore fork, uid, gid, sid, ptrace, coredump, comm events.
            }
        }
    }
}

/// Read process info from /proc/<pid>/ for enriching exec events.
fn read_process_info(pid: u32) -> (String, String, u32, Option<String>) {
    let proc_dir = format!("/proc/{pid}");

    // Read executable path.
    let path = std::fs::read_link(format!("{proc_dir}/exe"))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Read command line.
    let cmdline = std::fs::read_to_string(format!("{proc_dir}/cmdline"))
        .map(|s| s.replace('\0', " ").trim().to_string())
        .unwrap_or_default();

    // Read ppid and user from /proc/<pid>/status.
    let mut ppid = 0u32;
    let mut uid = 0u32;
    if let Ok(status) = std::fs::read_to_string(format!("{proc_dir}/status")) {
        for line in status.lines() {
            if let Some(val) = line.strip_prefix("PPid:\t") {
                ppid = val.trim().parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("Uid:\t") {
                uid = val
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            }
        }
    }

    // Resolve UID to username.
    let user = resolve_uid(uid);

    (path, cmdline, ppid, user)
}

/// Resolve a UID to a username by reading /etc/passwd.
fn resolve_uid(uid: u32) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    for line in passwd.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() >= 3 {
            if let Ok(entry_uid) = fields[2].parse::<u32>() {
                if entry_uid == uid {
                    return Some(fields[0].to_string());
                }
            }
        }
    }
    None
}

/// Fallback process monitor using /proc scanning (when proc connector is unavailable).
async fn poll_processes_fallback(tx: mpsc::Sender<SystemEvent>) -> anyhow::Result<()> {
    tracing::info!("Process monitor active via /proc polling (1s interval)");

    let mut known_pids: HashSet<u32> = HashSet::new();

    // Seed with current processes.
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
                known_pids.insert(pid);
            }
        }
    }

    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;

        let mut current_pids: HashSet<u32> = HashSet::new();

        if let Ok(entries) = std::fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
                    current_pids.insert(pid);
                }
            }
        }

        let timestamp = chrono::Utc::now().timestamp();

        // New processes.
        for &pid in current_pids.difference(&known_pids) {
            let (path, cmdline, ppid, user) = read_process_info(pid);
            let event = SystemEvent::ProcessStarted {
                pid,
                ppid,
                path,
                cmdline,
                user,
                timestamp,
            };
            if tx.send(event).await.is_err() {
                return Ok(());
            }
        }

        // Exited processes.
        for &pid in known_pids.difference(&current_pids) {
            let event = SystemEvent::ProcessExited {
                pid,
                exit_code: -1, // Unknown exit code with polling.
                timestamp,
            };
            if tx.send(event).await.is_err() {
                return Ok(());
            }
        }

        known_pids = current_pids;
    }
}

// ─── File Monitoring via fanotify ────────────────────────────────────────────

/// Monitor file events using fanotify.
///
/// fanotify provides notification for filesystem events (open, read, write, close)
/// with the PID of the accessing process. Requires CAP_SYS_ADMIN.
async fn monitor_files(
    tx: mpsc::Sender<SystemEvent>,
    paths: &[String],
) -> anyhow::Result<()> {
    // fanotify constants (from linux/fanotify.h).
    const FAN_CLASS_CONTENT: libc::c_uint = 0x04;
    const FAN_CLOEXEC: libc::c_uint = 0x01;
    const FAN_NONBLOCK: libc::c_uint = 0x02;

    const FAN_MARK_ADD: libc::c_uint = 0x01;
    const FAN_MARK_MOUNT: libc::c_uint = 0x10;

    const FAN_ACCESS: u64 = 0x01;
    const FAN_MODIFY: u64 = 0x02;
    const FAN_OPEN: u64 = 0x20;

    let fan_fd = unsafe {
        libc::syscall(
            libc::SYS_fanotify_init,
            FAN_CLASS_CONTENT | FAN_CLOEXEC | FAN_NONBLOCK,
            libc::O_RDONLY as libc::c_uint,
        )
    };

    if fan_fd < 0 {
        return Err(anyhow::anyhow!(
            "fanotify_init failed (need root/CAP_SYS_ADMIN): {}",
            std::io::Error::last_os_error()
        ));
    }

    let fan_fd = fan_fd as i32;

    // Mark each mount point for monitoring.
    let event_mask: u64 = FAN_ACCESS | FAN_MODIFY | FAN_OPEN;
    for path in paths {
        let c_path = std::ffi::CString::new(path.as_str())
            .map_err(|_| anyhow::anyhow!("Invalid path: {path}"))?;

        let ret = unsafe {
            libc::syscall(
                libc::SYS_fanotify_mark,
                fan_fd,
                FAN_MARK_ADD | FAN_MARK_MOUNT,
                event_mask,
                libc::AT_FDCWD,
                c_path.as_ptr(),
            )
        };

        if ret < 0 {
            tracing::warn!(
                path = path,
                error = %std::io::Error::last_os_error(),
                "fanotify_mark failed for path"
            );
        } else {
            tracing::info!(path = path, "Monitoring file events via fanotify");
        }
    }

    tracing::info!("File monitor active via fanotify");

    // Read events in a loop.
    let (read_tx, mut read_rx) = mpsc::channel::<SystemEvent>(256);
    let fan_fd_copy = fan_fd;

    std::thread::spawn(move || {
        read_fanotify_events(fan_fd_copy, read_tx);
    });

    while let Some(event) = read_rx.recv().await {
        if tx.send(event).await.is_err() {
            break;
        }
    }

    unsafe { libc::close(fan_fd) };
    Ok(())
}

/// Read fanotify events from the fd (blocking, runs in OS thread).
fn read_fanotify_events(fd: i32, tx: mpsc::Sender<SystemEvent>) {
    const FAN_ACCESS: u64 = 0x01;
    const FAN_MODIFY: u64 = 0x02;
    // FAN_OPEN = 0x20 (we treat open as read)

    #[repr(C)]
    struct FanotifyEventMetadata {
        event_len: u32,
        vers: u8,
        reserved: u8,
        metadata_len: u16,
        mask: u64,
        fd: i32,
        pid: i32,
    }

    let meta_size = std::mem::size_of::<FanotifyEventMetadata>();
    let mut buf = vec![0u8; 4096];

    loop {
        let len = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };

        if len < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::WouldBlock {
                // Non-blocking fd, sleep briefly and retry.
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            tracing::debug!(error = %err, "fanotify read error, exiting reader");
            break;
        }

        if (len as usize) < meta_size {
            continue;
        }

        let mut offset = 0usize;
        while offset + meta_size <= len as usize {
            let meta =
                unsafe { &*(buf.as_ptr().add(offset) as *const FanotifyEventMetadata) };

            if meta.event_len < meta_size as u32 {
                break;
            }

            // Resolve the file path from /proc/self/fd/<event_fd>.
            let file_path = if meta.fd >= 0 {
                let link = format!("/proc/self/fd/{}", meta.fd);
                let path = std::fs::read_link(&link)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                unsafe { libc::close(meta.fd) };
                path
            } else {
                String::new()
            };

            let operation = if meta.mask & FAN_MODIFY != 0 {
                FileOperation::Write
            } else if meta.mask & FAN_ACCESS != 0 {
                FileOperation::Read
            } else {
                FileOperation::Read // FAN_OPEN → treat as read
            };

            let event = SystemEvent::FileAccessed {
                pid: meta.pid as u32,
                path: file_path,
                operation,
                timestamp: chrono::Utc::now().timestamp(),
            };

            if tx.blocking_send(event).is_err() {
                return;
            }

            offset += meta.event_len as usize;
        }
    }
}

// ─── Network Monitoring via /proc/net/tcp ────────────────────────────────────

/// Monitor network connections by parsing /proc/net/tcp[6] periodically.
///
/// Maintains a set of known connections and only emits events for new ones.
/// Resolves PIDs by scanning /proc/<pid>/fd/ for matching socket inodes.
async fn monitor_network(tx: mpsc::Sender<SystemEvent>) {
    tracing::info!("Network monitor active via /proc/net/tcp polling (5s interval)");

    let mut known_connections: HashSet<String> = HashSet::new();
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        interval.tick().await;

        let mut current_connections: HashMap<String, TcpEntry> = HashMap::new();

        // Parse /proc/net/tcp and /proc/net/tcp6.
        for path in &["/proc/net/tcp", "/proc/net/tcp6"] {
            if let Ok(entries) = parse_proc_net_tcp(path) {
                for entry in entries {
                    // Only track established connections (state = 01).
                    if entry.state == 1 {
                        let key = format!("{}:{}", entry.local_addr, entry.remote_addr);
                        current_connections.insert(key, entry);
                    }
                }
            }
        }

        let timestamp = chrono::Utc::now().timestamp();

        // Emit events for new connections.
        for (key, entry) in &current_connections {
            if !known_connections.contains(key) {
                let pid = resolve_socket_pid(entry.inode).unwrap_or(0);
                let event = SystemEvent::NetworkConnected {
                    pid,
                    src: entry.local_addr.clone(),
                    dst: entry.remote_addr.clone(),
                    protocol: NetworkProtocol::Tcp,
                    timestamp,
                };
                if tx.send(event).await.is_err() {
                    return;
                }
            }
        }

        known_connections = current_connections.keys().cloned().collect();
    }
}

/// A parsed TCP connection entry from /proc/net/tcp.
struct TcpEntry {
    local_addr: String,
    remote_addr: String,
    state: u8,
    inode: u64,
}

/// Parse /proc/net/tcp or /proc/net/tcp6 into TcpEntry structs.
fn parse_proc_net_tcp(path: &str) -> anyhow::Result<Vec<TcpEntry>> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines().skip(1) {
        let line = line?;
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }

        let local = parse_hex_addr(fields[1]);
        let remote = parse_hex_addr(fields[2]);
        let state = u8::from_str_radix(fields[3], 16).unwrap_or(0);
        let inode = fields[9].parse::<u64>().unwrap_or(0);

        entries.push(TcpEntry {
            local_addr: local,
            remote_addr: remote,
            state,
            inode,
        });
    }

    Ok(entries)
}

/// Parse a hex-encoded address:port from /proc/net/tcp (e.g., "0100007F:1F90").
fn parse_hex_addr(hex: &str) -> String {
    let parts: Vec<&str> = hex.split(':').collect();
    if parts.len() != 2 {
        return hex.to_string();
    }

    let addr_hex = parts[0];
    let port = u16::from_str_radix(parts[1], 16).unwrap_or(0);

    if addr_hex.len() == 8 {
        // IPv4
        let addr = u32::from_str_radix(addr_hex, 16).unwrap_or(0);
        let bytes = addr.to_le_bytes();
        format!("{}.{}.{}.{}:{}", bytes[0], bytes[1], bytes[2], bytes[3], port)
    } else if addr_hex.len() == 32 {
        // IPv6 (simplified — show as hex groups)
        let mut groups = Vec::new();
        for i in (0..32).step_by(8) {
            // /proc/net/tcp6 stores each 32-bit word in host byte order
            let word = &addr_hex[i..i + 8];
            let val = u32::from_str_radix(word, 16).unwrap_or(0);
            let bytes = val.to_le_bytes();
            groups.push(format!("{:02x}{:02x}:{:02x}{:02x}", bytes[0], bytes[1], bytes[2], bytes[3]));
        }
        format!("[{}]:{}", groups.join(":"), port)
    } else {
        format!("{hex}")
    }
}

/// Resolve a socket inode to a PID by scanning /proc/<pid>/fd/.
fn resolve_socket_pid(inode: u64) -> Option<u32> {
    if inode == 0 {
        return None;
    }

    let target = format!("socket:[{inode}]");
    let proc_dir = std::fs::read_dir("/proc").ok()?;

    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let pid: u32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let fd_dir = format!("/proc/{pid}/fd");
        if let Ok(fds) = std::fs::read_dir(&fd_dir) {
            for fd_entry in fds.flatten() {
                if let Ok(link) = std::fs::read_link(fd_entry.path()) {
                    if link.to_string_lossy() == target {
                        return Some(pid);
                    }
                }
            }
        }
    }

    None
}
