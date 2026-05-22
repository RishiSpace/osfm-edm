//! Shell session — manages an interactive PTY process and bridges it to the
//! WebSocket transport.
//!
//! On Linux/macOS, spawns a PTY via `openpty` + `fork` pattern using
//! `std::process::Command` with stdin/stdout/stderr piped.
//! On Windows, spawns `cmd.exe` with piped I/O.

use osfm_edm_common::protocol::AgentMessage;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Manages all active shell sessions for this agent.
pub struct ShellManager {
    /// Active sessions keyed by session_id.
    sessions: HashMap<Uuid, ShellSession>,
    /// Channel to send agent messages back to the WebSocket transport.
    outbound_tx: mpsc::Sender<AgentMessage>,
}

struct ShellSession {
    /// Handle to write stdin to the shell process.
    stdin_tx: mpsc::Sender<String>,
    /// Handle to kill the shell process.
    kill_tx: tokio::sync::oneshot::Sender<()>,
}

impl ShellManager {
    pub fn new(outbound_tx: mpsc::Sender<AgentMessage>) -> Self {
        Self {
            sessions: HashMap::new(),
            outbound_tx,
        }
    }

    /// Open a new interactive shell session.
    pub fn open_session(&mut self, session_id: Uuid) {
        if self.sessions.contains_key(&session_id) {
            tracing::warn!(session_id = %session_id, "Shell session already exists");
            return;
        }

        tracing::info!(session_id = %session_id, "Opening shell session");

        let (stdin_tx, stdin_rx) = mpsc::channel::<String>(256);
        let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();
        let outbound = self.outbound_tx.clone();

        // Spawn the shell process in a background task.
        tokio::spawn(async move {
            run_shell_process(session_id, stdin_rx, kill_rx, outbound).await;
        });

        self.sessions.insert(
            session_id,
            ShellSession { stdin_tx, kill_tx },
        );
    }

    /// Send input (stdin) to an active shell session.
    pub async fn send_input(&self, session_id: Uuid, data: String) {
        if let Some(session) = self.sessions.get(&session_id) {
            if session.stdin_tx.send(data).await.is_err() {
                tracing::warn!(session_id = %session_id, "Shell session stdin channel closed");
            }
        } else {
            tracing::warn!(session_id = %session_id, "No shell session found for input");
        }
    }

    /// Close (terminate) an active shell session.
    pub fn close_session(&mut self, session_id: Uuid) {
        if let Some(session) = self.sessions.remove(&session_id) {
            tracing::info!(session_id = %session_id, "Closing shell session");
            let _ = session.kill_tx.send(());
        } else {
            tracing::warn!(session_id = %session_id, "No shell session found to close");
        }
    }
}

/// Spawn a shell process and bridge its I/O to the WebSocket transport.
async fn run_shell_process(
    session_id: Uuid,
    mut stdin_rx: mpsc::Receiver<String>,
    kill_rx: tokio::sync::oneshot::Receiver<()>,
    outbound: mpsc::Sender<AgentMessage>,
) {
    // Choose the shell based on the platform.
    let (shell, args): (&str, &[&str]) = if cfg!(target_os = "windows") {
        ("cmd.exe", &["/Q"])
    } else {
        // Use the user's default shell, or fall back to /bin/sh.
        ("/bin/sh", &["-i"])
    };

    let mut child: Child = match Command::new(shell)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            tracing::error!(session_id = %session_id, error = %e, "Failed to spawn shell");
            let _ = outbound
                .send(AgentMessage::ShellClosed {
                    session_id,
                    exit_code: None,
                })
                .await;
            return;
        }
    };

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Spawn stdout reader.
    let out_tx = outbound.clone();
    let out_sid = session_id;
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut buf = vec![0u8; 4096];
        loop {
            use tokio::io::AsyncReadExt;
            match reader.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if out_tx
                        .send(AgentMessage::ShellOutput {
                            session_id: out_sid,
                            data,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!(session_id = %out_sid, error = %e, "Shell stdout read error");
                    break;
                }
            }
        }
    });

    // Spawn stderr reader.
    let err_tx = outbound.clone();
    let err_sid = session_id;
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = vec![0u8; 4096];
        loop {
            use tokio::io::AsyncReadExt;
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if err_tx
                        .send(AgentMessage::ShellOutput {
                            session_id: err_sid,
                            data,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!(session_id = %err_sid, error = %e, "Shell stderr read error");
                    break;
                }
            }
        }
    });

    // Main loop: forward stdin and handle kill.
    tokio::pin!(kill_rx);

    loop {
        tokio::select! {
            input = stdin_rx.recv() => {
                match input {
                    Some(data) => {
                        if stdin.write_all(data.as_bytes()).await.is_err() {
                            break;
                        }
                        let _ = stdin.flush().await;
                    }
                    None => break, // stdin channel closed
                }
            }
            _ = &mut kill_rx => {
                tracing::info!(session_id = %session_id, "Shell session killed by request");
                let _ = child.kill().await;
                break;
            }
            status = child.wait() => {
                let exit_code = status.ok().and_then(|s| s.code());
                tracing::info!(session_id = %session_id, exit_code = ?exit_code, "Shell process exited");
                let _ = outbound.send(AgentMessage::ShellClosed { session_id, exit_code }).await;

                stdout_task.abort();
                stderr_task.abort();
                return;
            }
        }
    }

    // Clean up.
    let _ = child.kill().await;
    let exit_code = child.wait().await.ok().and_then(|s| s.code());

    stdout_task.abort();
    stderr_task.abort();

    let _ = outbound
        .send(AgentMessage::ShellClosed {
            session_id,
            exit_code,
        })
        .await;
}
