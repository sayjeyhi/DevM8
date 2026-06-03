use std::path::Path;
use std::process::Stdio;

use crate::daemon::pid::{is_process_running, read_pid, write_pid};
use crate::shared::errors::{AppError, FriendlyError};
use crate::shared::paths::PATHS;

#[derive(Debug, Clone, Default)]
pub struct AgentStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
}

pub fn generate_service(binary_path: &str) -> String {
    format!(
        r#"[Unit]
Description=DevM8 — Jira + Claude + Telegram assistant
After=network.target

[Service]
Type=simple
ExecStart={binary} daemon
Restart=on-failure
RestartSec=10
StandardOutput=journal
StandardError=journal
PassEnvironment=GH_TOKEN GITHUB_TOKEN ANTHROPIC_API_KEY CLAUDE_CONFIG_DIR

[Install]
WantedBy=default.target
"#,
        binary = binary_path
    )
}

pub async fn write_service_file(
    binary_path: &str,
    file_path: Option<&Path>,
) -> Result<(), AppError> {
    let target = file_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PATHS.service_file.clone());

    if let Some(dir) = target.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }

    let content = generate_service(binary_path);
    let tmp = target.with_extension("service.tmp");

    tokio::fs::write(&tmp, content.as_bytes()).await?;
    tokio::fs::rename(&tmp, &target).await?;

    Ok(())
}

fn run_systemctl(args: &[&str]) -> Result<std::process::Output, AppError> {
    std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| {
            AppError::Friendly(FriendlyError::with_hint(
                format!("Failed to run systemctl: {e}"),
                "Make sure systemd user services are available on this system.",
            ))
        })
}

/// Returns true when the systemd user session bus is reachable.
/// Cheap check — runs `systemctl --user status` and inspects stderr.
fn user_bus_available() -> bool {
    match std::process::Command::new("systemctl")
        .args(["--user", "status"])
        .output()
    {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            !stderr.contains("Failed to connect to") && !stderr.contains("Operation not permitted")
        }
        Err(_) => false,
    }
}

/// Spawn `devm8 daemon` directly as a detached background process.
/// Used when the systemd user bus is not available.
async fn spawn_direct() -> Result<(), AppError> {
    // Kill any stale daemon processes before starting a fresh one.
    let stale = crate::daemon::pid::kill_all_daemons();
    if stale > 0 {
        println!("Stopped {stale} stale daemon process(es).");
        // Brief pause so processes exit before we write the new PID file.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(p).ok())
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("cannot resolve current binary path")))?;

    let log_path = &PATHS.log_file;
    if let Some(dir) = log_path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| AppError::Other(anyhow::anyhow!("cannot open log file: {e}")))?;
    let log_file2 = log_file
        .try_clone()
        .map_err(|e| AppError::Other(anyhow::anyhow!("cannot clone log file handle: {e}")))?;

    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("daemon")
        .stdin(Stdio::null())
        .stdout(log_file)
        .stderr(log_file2);

    // Detach from the current session so the daemon survives terminal close.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        extern "C" {
            fn setsid() -> i32;
        }
        unsafe {
            cmd.pre_exec(|| {
                setsid();
                Ok(())
            });
        }
    }

    let child = cmd
        .spawn()
        .map_err(|e| AppError::Other(anyhow::anyhow!("failed to spawn daemon: {e}")))?;

    let pid = child.id();
    // Forget the handle so Drop doesn't kill the child when start_command returns.
    std::mem::forget(child);

    write_pid(pid, None).await?;
    Ok(())
}

pub async fn load_agent() -> Result<(), AppError> {
    if !user_bus_available() {
        println!("Note: systemd user bus unavailable — starting daemon directly.");
        return spawn_direct().await;
    }

    let reload = run_systemctl(&["daemon-reload"])?;
    if !reload.status.success() {
        let stderr = String::from_utf8_lossy(&reload.stderr).into_owned();
        return Err(AppError::Friendly(FriendlyError::with_hint(
            format!("systemctl daemon-reload failed: {stderr}"),
            "Run `systemctl --user daemon-reload` to diagnose.",
        )));
    }

    let out = run_systemctl(&["enable", "--now", "devm8"])?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(AppError::Friendly(FriendlyError::with_hint(
            format!("Failed to enable/start devm8 service: {stderr}"),
            "Check `journalctl --user -u devm8` for details.",
        )));
    }

    Ok(())
}

pub async fn unload_agent() -> Result<(), AppError> {
    if !user_bus_available() {
        // Kill ALL daemon processes — not just the one in the PID file.
        let killed = crate::daemon::pid::kill_all_daemons();
        if killed == 0 {
            // Fallback: try PID file in case /proc scan missed something.
            if let Ok(Some(pid)) = read_pid(None).await {
                if is_process_running(pid) {
                    let _ = std::process::Command::new("kill")
                        .args(["-TERM", &pid.to_string()])
                        .status();
                }
            }
        }
        return Ok(());
    }
    let _ = run_systemctl(&["stop", "devm8"]);
    let _ = run_systemctl(&["disable", "devm8"]);
    Ok(())
}

pub async fn agent_status() -> AgentStatus {
    let mut status = AgentStatus::default();

    if !user_bus_available() {
        // No user bus — scan /proc for all daemon processes.
        let pids = crate::daemon::pid::find_daemon_pids();
        if let Some(&first) = pids.first() {
            status.running = true;
            status.pid = Some(first);
        }
        return status;
    }

    let is_active = std::process::Command::new("systemctl")
        .args(["--user", "is-active", "devm8"])
        .output();

    if let Ok(o) = is_active {
        if o.status.success() {
            status.running = true;
            if let Ok(show) = std::process::Command::new("systemctl")
                .args(["--user", "show", "devm8", "--property=MainPID"])
                .output()
            {
                let out = String::from_utf8_lossy(&show.stdout);
                if let Some(pid_str) = out.trim().strip_prefix("MainPID=") {
                    if let Ok(pid) = pid_str.trim().parse::<u32>() {
                        if pid > 0 {
                            status.pid = Some(pid);
                        }
                    }
                }
            }
        } else if let Ok(show) = std::process::Command::new("systemctl")
            .args(["--user", "show", "devm8", "--property=ExecMainStatus"])
            .output()
        {
            let out = String::from_utf8_lossy(&show.stdout);
            if let Some(code_str) = out.trim().strip_prefix("ExecMainStatus=") {
                if let Ok(code) = code_str.trim().parse::<i32>() {
                    status.exit_code = Some(code);
                }
            }
        }
    }

    status
}
