//! Job executor — runs dispatched jobs and streams output back to the server.

use osfm_edm_common::jobs::{JobPayload, PackageManager, ShellType};
use osfm_edm_common::protocol::AgentMessage;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use uuid::Uuid;

const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Execute a job payload and stream output back via the outbound channel.
pub async fn execute_job(
    job_id: Uuid,
    payload: JobPayload,
    outbound_tx: mpsc::Sender<AgentMessage>,
) {
    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel();
    super::registry::register(job_id, kill_tx);

    match payload {
        JobPayload::RunScript { shell, script } => {
            execute_script(
                job_id,
                shell,
                script,
                Some(DEFAULT_TIMEOUT_SECS),
                kill_rx,
                outbound_tx,
            )
            .await;
        }
        JobPayload::InstallPackage {
            manager, package, ..
        } => {
            let (shell, cmd) = package_cmd(&manager, "install", &package);
            execute_script(job_id, shell, cmd, Some(600), kill_rx, outbound_tx).await;
        }
        JobPayload::UninstallPackage { manager, package } => {
            let (shell, cmd) = package_cmd(&manager, "remove", &package);
            execute_script(job_id, shell, cmd, Some(600), kill_rx, outbound_tx).await;
        }
        JobPayload::PushFile {
            destination,
            content_b64,
            permissions,
        } => {
            if cfg!(target_os = "windows") {
                push_file_windows(job_id, &destination, &content_b64, outbound_tx).await;
            } else {
                // Decode base64 content and write to destination. All interpolated
                // values are single-quote escaped so a `'` in the path cannot
                // break out of the quoting (command injection).
                let dest = shell_quote(&destination);
                let data = shell_quote(&content_b64);
                let cmd = if let Some(perms) = permissions {
                    let perms = shell_quote(&perms);
                    format!("echo {data} | base64 -d > {dest} && chmod {perms} {dest}",)
                } else {
                    format!("echo {data} | base64 -d > {dest}")
                };
                execute_script(
                    job_id,
                    ShellType::Bash,
                    cmd,
                    Some(DEFAULT_TIMEOUT_SECS),
                    kill_rx,
                    outbound_tx,
                )
                .await;
            }
        }
        JobPayload::Reboot { delay_seconds } => {
            let (shell, cmd) = reboot_cmd(delay_seconds);
            execute_script(job_id, shell, cmd, Some(30), kill_rx, outbound_tx).await;
        }
        JobPayload::CollectInventory => {
            super::registry::remove(&job_id);
            let software = crate::telemetry::software::collect_software();
            let patches = crate::telemetry::patches::collect_patches();
            let _ = outbound_tx
                .send(AgentMessage::InventoryReport { software, patches })
                .await;
            let _ = outbound_tx
                .send(AgentMessage::JobCompleted {
                    job_id,
                    exit_code: 0,
                })
                .await;
        }
        JobPayload::RunPatchUpdate { patch_ids } => {
            let (shell, cmd) = patch_cmd(&patch_ids);
            execute_script(job_id, shell, cmd, Some(600), kill_rx, outbound_tx).await;
        }
    }
}

/// Windows PushFile: decode base64 in-process and write the file directly.
/// No shell interpolation, so no injection surface. POSIX perms are ignored.
async fn push_file_windows(
    job_id: Uuid,
    destination: &str,
    content_b64: &str,
    outbound_tx: mpsc::Sender<AgentMessage>,
) {
    use base64::Engine as _;
    super::registry::remove(&job_id);
    let fail = |line: String| {
        let tx = outbound_tx.clone();
        async move {
            let _ = tx
                .send(AgentMessage::JobLog {
                    job_id,
                    line,
                    stream: "stderr".to_string(),
                })
                .await;
            let _ = tx
                .send(AgentMessage::JobCompleted {
                    job_id,
                    exit_code: -1,
                })
                .await;
        }
    };
    let bytes = match base64::engine::general_purpose::STANDARD.decode(content_b64.trim()) {
        Ok(b) => b,
        Err(e) => {
            fail(format!("base64 decode failed: {e}")).await;
            return;
        }
    };
    if let Some(parent) = std::path::Path::new(destination).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                fail(format!("create parent dir failed: {e}")).await;
                return;
            }
        }
    }
    match std::fs::write(destination, &bytes) {
        Ok(()) => {
            let _ = outbound_tx
                .send(AgentMessage::JobLog {
                    job_id,
                    line: format!("wrote {} bytes to {destination}", bytes.len()),
                    stream: "stdout".to_string(),
                })
                .await;
            let _ = outbound_tx
                .send(AgentMessage::JobCompleted {
                    job_id,
                    exit_code: 0,
                })
                .await;
        }
        Err(e) => fail(format!("write failed: {e}")).await,
    }
}

/// Single-quote a value for safe interpolation into a POSIX shell command.
/// A literal `'` becomes `'"'"'` (close quote, escaped quote, reopen quote).
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Build a package manager command plus the interpreter that can run it.
/// Windows managers use Cmd/Powershell; Unix managers use Bash. Package names
/// are allow-listed to [A-Za-z0-9._-] to block shell injection.
fn package_cmd(manager: &PackageManager, action: &str, package: &str) -> (ShellType, String) {
    let pkg = sanitize_package(package);
    match manager {
        PackageManager::Apt => (ShellType::Bash, format!("apt-get {action} -y {pkg} 2>&1")),
        PackageManager::Dnf => (ShellType::Bash, format!("dnf {action} -y {pkg} 2>&1")),
        PackageManager::Pacman => {
            let flag = if action == "install" {
                "-S --noconfirm"
            } else {
                "-R --noconfirm"
            };
            (ShellType::Bash, format!("pacman {flag} {pkg} 2>&1"))
        }
        PackageManager::Homebrew => (ShellType::Bash, format!("brew {action} {pkg} 2>&1")),
        PackageManager::Winget => (
            ShellType::Cmd,
            format!("winget {action} --accept-source-agreements {pkg}"),
        ),
        PackageManager::Chocolatey => (ShellType::Cmd, format!("choco {action} -y {pkg}")),
    }
}

/// Reboot command for the current platform.
fn reboot_cmd(delay_seconds: u32) -> (ShellType, String) {
    if cfg!(target_os = "windows") {
        (ShellType::Cmd, format!("shutdown /r /t {delay_seconds}"))
    } else {
        (
            ShellType::Bash,
            format!("shutdown -r +{}", delay_seconds / 60),
        )
    }
}

/// Patch-install command for the current platform.
fn patch_cmd(patch_ids: &[String]) -> (ShellType, String) {
    let ids: Vec<String> = patch_ids.iter().map(|p| sanitize_package(p)).collect();
    if cfg!(target_os = "windows") {
        (
            ShellType::Cmd,
            format!(
                "winget upgrade --accept-source-agreements {} 2>&1",
                ids.join(" ")
            ),
        )
    } else if cfg!(target_os = "macos") {
        (
            ShellType::Bash,
            format!("brew upgrade {} 2>&1", ids.join(" ")),
        )
    } else {
        (
            ShellType::Bash,
            format!("apt-get install -y {} 2>&1", ids.join(" ")),
        )
    }
}

/// Allow-list package names to alphanumerics plus . _ - so they cannot break
/// out of the shell command.
fn sanitize_package(package: &str) -> String {
    let clean: String = package
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    if clean.is_empty() {
        "invalid".to_string()
    } else {
        clean
    }
}

/// Run a script with the given interpreter and stream stdout/stderr lines back.
async fn execute_script(
    job_id: Uuid,
    shell: ShellType,
    script: String,
    timeout_secs: Option<u64>,
    kill_rx: tokio::sync::oneshot::Receiver<()>,
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
                .send(AgentMessage::JobCompleted {
                    job_id,
                    exit_code: -1,
                })
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

    let timeout = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    let exit_code = tokio::select! {
        status = child.wait() => match status {
            Ok(s) => s.code().unwrap_or(-1),
            Err(e) => {
                tracing::error!(job_id = %job_id, error = %e, "Process wait error");
                -1
            }
        },
        _ = kill_rx => {
            tracing::warn!(job_id = %job_id, "Job cancelled");
            let _ = child.kill().await;
            -4
        }
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(timeout)) => {
            tracing::warn!(job_id = %job_id, "Job timed out after {timeout}s");
            let _ = child.kill().await;
            -2
        }
    };
    super::registry::remove(&job_id);

    let _ = stdout_task.await;
    let _ = stderr_task.await;

    tracing::info!(job_id = %job_id, exit_code, "Job execution completed");

    let _ = outbound_tx
        .send(AgentMessage::JobCompleted { job_id, exit_code })
        .await;
}

#[cfg(test)]
mod tests {
    use super::shell_quote;

    #[test]
    fn shell_quote_plain_value() {
        assert_eq!(shell_quote("/tmp/file.txt"), "'/tmp/file.txt'");
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        // An attacker-controlled path containing a quote must not break out.
        // Verified against bash: the quoted form evaluates back to the exact
        // original string.
        assert_eq!(
            shell_quote("/tmp/x'; rm -rf /; echo '"),
            r##"'/tmp/x'"'"'; rm -rf /; echo '"'"''"##
        );
    }

    #[test]
    fn shell_quote_empty() {
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn shell_quote_handles_spaces_and_dollars() {
        // Spaces and $ are safely inert inside single quotes.
        assert_eq!(shell_quote("/tmp/my dir/$(id)"), "'/tmp/my dir/$(id)'");
    }

    #[test]
    fn sanitize_package_blocks_injection() {
        assert_eq!(super::sanitize_package("nginx"), "nginx");
        assert_eq!(super::sanitize_package("a; rm -rf /"), "arm-rf");
        assert_eq!(super::sanitize_package(""), "invalid");
    }

    #[test]
    fn package_cmd_uses_native_shell() {
        use osfm_edm_common::jobs::PackageManager;
        let (shell, cmd) = super::package_cmd(&PackageManager::Winget, "install", "Git.Git");
        assert!(matches!(shell, super::ShellType::Cmd));
        assert!(cmd.contains("Git.Git"));
        let (_, cmd) = super::package_cmd(&PackageManager::Apt, "install", "a;id");
        assert!(!cmd.contains(';'));
    }
}
