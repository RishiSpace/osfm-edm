//! Interactive shell via a real PTY (`portable-pty`).

use osfm_edm_common::protocol::AgentMessage;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct ShellManager {
    sessions: HashMap<Uuid, ShellSession>,
    outbound_tx: mpsc::Sender<AgentMessage>,
}

struct ShellSession {
    stdin_tx: std_mpsc::Sender<String>,
    kill_tx: std_mpsc::Sender<()>,
}

impl ShellManager {
    pub fn new(outbound_tx: mpsc::Sender<AgentMessage>) -> Self {
        Self {
            sessions: HashMap::new(),
            outbound_tx,
        }
    }

    pub fn open_session(&mut self, session_id: Uuid) {
        if self.sessions.contains_key(&session_id) {
            return;
        }
        let (stdin_tx, stdin_rx) = std_mpsc::channel::<String>();
        let (kill_tx, kill_rx) = std_mpsc::channel::<()>();
        let outbound = self.outbound_tx.clone();
        std::thread::spawn(move || run_pty_blocking(session_id, stdin_rx, kill_rx, outbound));
        self.sessions
            .insert(session_id, ShellSession { stdin_tx, kill_tx });
    }

    pub async fn send_input(&self, session_id: Uuid, data: String) {
        if let Some(session) = self.sessions.get(&session_id) {
            let _ = session.stdin_tx.send(data);
        }
    }

    pub fn close_session(&mut self, session_id: Uuid) {
        if let Some(session) = self.sessions.remove(&session_id) {
            let _ = session.kill_tx.send(());
        }
    }
}

fn run_pty_blocking(
    session_id: Uuid,
    stdin_rx: std_mpsc::Receiver<String>,
    kill_rx: std_mpsc::Receiver<()>,
    outbound: mpsc::Sender<AgentMessage>,
) {
    let finish = |code: Option<i32>| {
        let _ = outbound.blocking_send(AgentMessage::ShellClosed {
            session_id,
            exit_code: code,
        });
    };

    let system = NativePtySystem::default();
    let pair = match system.openpty(PtySize {
        rows: 32,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "openpty failed");
            finish(None);
            return;
        }
    };

    let cmd = if cfg!(target_os = "windows") {
        CommandBuilder::new("cmd.exe")
    } else {
        let mut c = CommandBuilder::new("/bin/sh");
        c.arg("-i");
        c
    };

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "spawn shell failed");
            finish(None);
            return;
        }
    };
    drop(pair.slave);

    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "pty reader");
            finish(None);
            return;
        }
    };
    let mut writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            tracing::error!(error = %e, "pty writer");
            finish(None);
            return;
        }
    };

    let out = outbound.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if out
                        .blocking_send(AgentMessage::ShellOutput { session_id, data })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    loop {
        if kill_rx.try_recv().is_ok() {
            let _ = child.kill();
            break;
        }
        match stdin_rx.recv_timeout(std::time::Duration::from_millis(80)) {
            Ok(data) => {
                let _ = writer.write_all(data.as_bytes());
                let _ = writer.flush();
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
            Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if let Ok(Some(status)) = child.try_wait() {
            finish(Some(status.exit_code() as i32));
            return;
        }
    }
    let _ = child.kill();
    let code = child.wait().ok().map(|s| s.exit_code() as i32);
    finish(code);
}
