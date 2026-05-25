use std::path::{Path, PathBuf};

use crate::shared::errors::AppError;
use crate::shared::paths::PATHS;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve(file_path: Option<&Path>) -> PathBuf {
    file_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PATHS.pid_file.clone())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Write `pid` to the PID file atomically (write tmp → rename).
pub async fn write_pid(pid: u32, file_path: Option<&Path>) -> Result<(), AppError> {
    let target = resolve(file_path);

    if let Some(dir) = target.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }

    let tmp = target.with_extension("pid.tmp");
    let content = format!("{}\n", pid);

    tokio::fs::write(&tmp, content.as_bytes()).await?;
    tokio::fs::rename(&tmp, &target).await?;

    Ok(())
}

/// Read the PID from the PID file.  Returns `None` if the file doesn't exist
/// or its content cannot be parsed as a `u32`.
pub async fn read_pid(file_path: Option<&Path>) -> Result<Option<u32>, AppError> {
    let path = resolve(file_path);

    let content = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Io(e)),
    };

    let pid = content.trim().parse::<u32>().ok();
    Ok(pid)
}

/// Remove the PID file, ignoring errors (file may not exist).
pub async fn remove_pid(file_path: Option<&Path>) -> Result<(), AppError> {
    let path = resolve(file_path);

    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(AppError::Io(e)),
    }

    Ok(())
}

/// Check whether a process with `pid` is running by sending signal 0.
///
/// Uses `kill -0 <pid>` via `std::process::Command` to avoid a direct
/// libc dependency.  Returns `true` if the process exists and is accessible,
/// `false` if `ESRCH` (no such process), and propagates other errors.
pub fn is_process_running(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Find the PIDs of every running `devm8 daemon` process by scanning /proc.
/// Excludes the calling process.
#[cfg(target_os = "linux")]
pub fn find_daemon_pids() -> Vec<u32> {
    let current = std::process::id();
    let mut pids = Vec::new();

    let Ok(entries) = std::fs::read_dir("/proc") else {
        return pids;
    };

    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == current {
            continue;
        }

        let Ok(data) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };

        // cmdline is null-terminated argv: "/path/to/devm8\0daemon\0..."
        let args: Vec<&[u8]> = data.split(|&b| b == 0).collect();
        let arg0 = std::str::from_utf8(args.first().copied().unwrap_or(b"")).unwrap_or("");
        let arg1 = std::str::from_utf8(args.get(1).copied().unwrap_or(b"")).unwrap_or("");

        if (arg0.ends_with("/devm8") || arg0 == "devm8") && arg1 == "daemon" {
            pids.push(pid);
        }
    }

    pids
}

/// Send SIGHUP to the daemon process recorded in the PID file, asking it to
/// reload its configuration.  Silently does nothing if the daemon is not
/// running or the platform does not support SIGHUP.
pub fn signal_daemon_reload() {
    #[cfg(unix)]
    {
        let pid_path = PATHS.pid_file.clone();
        if let Ok(content) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                if is_process_running(pid) {
                    let _ = std::process::Command::new("kill")
                        .args(["-HUP", &pid.to_string()])
                        .status();
                }
            }
        }
    }
}

/// Send SIGTERM to every running `devm8 daemon` process. Returns number killed.
#[cfg(target_os = "linux")]
pub fn kill_all_daemons() -> usize {
    let pids = find_daemon_pids();
    let count = pids.len();
    for pid in pids {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
    count
}
