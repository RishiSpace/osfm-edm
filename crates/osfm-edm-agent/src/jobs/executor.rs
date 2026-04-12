//! Job executor — runs dispatched jobs and streams output back to the server.

use osfm_edm_common::jobs::{JobPayload, ShellType, PackageManager};
use osfm_edm_common::protocol::AgentMessage;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Execute a job payload and stream output back via the outbound channel.
pub async fn execute_job(
    job_id: Uuid,
    payload: JobPayload,
    outbound_tx: mpsc::Sender<AgentMessage>,
) {
    match payload {
        JobPayload::RunScript { shell, script } => {
            execute_script(job_id, shell, script, None, outbound_tx).await;
        }
        JobPayload::InstallPackage { manager, package, .. } => {
            let cmd = package_cmd(&manager, "install", &package);
            execute_script(job_id, ShellType::Bash, cmd, None, outbound_tx).await;
        }
        JobPayload::UninstallPackage { manager, package } => {
            let cmd = package_cmd(&manager, "remove", &package);
            execute_script(job_id, ShellType::Bash, cmd, None, outbound_tx).await;
        }
        JobPayload::PushFile { destination, content_b64, permissions } => {
            // Decode base64 content and write to destination.
            let cmd = if let Some(perms) = permissions {
                format!(
                    "echo '{}' | base64 -d > '{}' && chmod {} '{}'",
                    content_b64, destination, perms, destination
                )
            } else {
                format!("echo '{}' | base64 -d > '{}'", content_b64, destination)
            };
            execute_script(job_id, ShellType::Bash, cmd, None, outbound_tx).await;
        }
        JobPayload::Reboot { delay_seconds } => {
            let cmd = format!("shutdown -r +{}", delay_seconds / 60);
            execute_script(job_id, ShellType::Bash, cmd, None, outbound_tx).await;
        }
        JobPayload::CollectInventory => {
            // Trigger an inventory collection — handled separately.
            let _ = outbound_tx
                .send(AgentMessage::JobCompleted { job_id, exit_code: 0 })
                .await;
        }
        JobPayload::RunPatchUpdate { patch_ids } => {
            let cmd = format!("apt-get install -y {} 2>&1", patch_ids.join(" "));
            execute_script(job_id, ShellType::Bash, cmd, None, outbound_tx).await;
        }
    }
}

/// Build a package manager command string.
fn package_cmd(manager: &PackageManager, action: &str, package: &str) -> String {
    match manager {
        PackageManager::Apt => format!("apt-get {action} -y {package} 2>&1"),
        PackageManager::Dnf => format!("dnf {action} -y {package} 2>&1"),
        PackageManager::Pacman => {
            let flag = if action == "install" { "-S --noconfirm" } else { "-R --noconfirm" };
            format!("pacman {flag} {package} 2>&1")
        }
        PackageManager::Homebrew => format!("brew {action} {package} 2>&1"),
        PackageManager::Winget => format!("winget {action} --accept-source-agreements {package}"),
        PackageManager::Chocolatey => format!("choco {action} -y {package}"),
    }
}

/// Run a script with the given interpreter and stream stdout/stderr lines back.
async fn execute_script(
    job_id: Uuid,
    shell: ShellType,
    script: String,
    timeout_secs: Option<u64>,
    outbound_tx: mpsc::Sender<AgentMessage>,
) {
    let (program, args) = match shell {
        ShellType::Bash => ("bash", vec!["-c".to_string(), script]),
        ShellType::Sh => ("sh", vec!["-c".to_string(), script]),
        ShellType::Powershell => ("powershell", vec!["-Command".to_string(), script]),
        ShellType::Cmd => ("cmd", vec!["/C".to_string(), script]),
    };

    tracing::info!(job_id = %job_id, interpreter = ?shell, "Starting job execution");

    let result = Command::new(program)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match result {
        Ok(child) => child,
        Err(e) => {
            tracing::error!(job_id = %job_id, error = %e, "Failed to spawn process");
            let _ = outbound_tx
                .send(AgentMessage::JobLog {
                    job_id,
                    line: format!("Failed to spawn: {e}"),
                    stream: "stderr".to_string(),
                })
                .await;
            let _ = outbound_tx
                .send(AgentMessage::JobCompleted { job_id, exit_code: -1 })
                .await;
            return;
        }
    };

    // Stream stdout.
    let stdout_tx = outbound_tx.clone();
    let stdout = child.stdout.take();
    let stdout_task = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = stdout_tx
                    .send(AgentMessage::JobLog {
                        job_id,
                        line,
                        stream: "stdout".to_string(),
                    })
                    .await;
            }
        }
    });

    // Stream stderr.
    let stderr_tx = outbound_tx.clone();
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = stderr_tx
                    .send(AgentMessage::JobLog {
                        job_id,
                        line,
                        stream: "stderr".to_string(),
                    })
                    .await;
            }
        }
    });

    // Wait for completion with optional timeout.
    let exit_code = if let Some(secs) = timeout_secs {
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(secs),
            child.wait(),
        )
        .await
        {
            Ok(Ok(status)) => status.code().unwrap_or(-1),
            Ok(Err(e)) => {
                tracing::error!(job_id = %job_id, error = %e, "Process wait error");
                -1
            }
            Err(_) => {
                tracing::warn!(job_id = %job_id, "Job timed out after {}s", secs);
                let _ = child.kill().await;
                -2
            }
        }
    } else {
        match child.wait().await {
            Ok(status) => status.code().unwrap_or(-1),
            Err(e) => {
                tracing::error!(job_id = %job_id, error = %e, "Process wait error");
                -1
            }
        }
    };

    let _ = stdout_task.await;
    let _ = stderr_task.await;

    tracing::info!(job_id = %job_id, exit_code, "Job execution completed");

    let _ = outbound_tx
        .send(AgentMessage::JobCompleted { job_id, exit_code })
        .await;
}
