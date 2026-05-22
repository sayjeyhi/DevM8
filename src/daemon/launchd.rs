#[cfg(target_os = "macos")]
use std::path::Path;
use std::process::Command;

use crate::shared::errors::{launchctl_hint, AppError, FriendlyError, LaunchctlError};
use crate::shared::paths::PATHS;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct AgentStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
}

// ---------------------------------------------------------------------------
// Environment keys forwarded into the launchd agent
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
const FORWARDED_ENV_KEYS: &[&str] = &[
    "HOME",
    "PATH",
    "USER",
    "ANTHROPIC_API_KEY",
    "CLAUDE_CONFIG_DIR",
];

// ---------------------------------------------------------------------------
// Plist generation
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Generate a macOS launchd plist XML string for the devm8 daemon.
#[cfg(target_os = "macos")]
pub fn generate_plist(binary_path: &str) -> String {
    let env_entries = FORWARDED_ENV_KEYS
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (*k, v)))
        .map(|(k, v)| {
            format!(
                "        <key>{}</key>\n        <string>{}</string>",
                k,
                xml_escape(&v)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let env_block = if env_entries.is_empty() {
        String::new()
    } else {
        format!(
            "\n    <key>EnvironmentVariables</key>\n    <dict>\n{}\n    </dict>",
            env_entries
        )
    };

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>net.devm8</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>daemon</string>
    </array>{env}
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>RunAtLoad</key>
    <false/>
</dict>
</plist>
"#,
        binary = xml_escape(binary_path),
        env = env_block,
    )
}

/// Write the plist to disk atomically (tmp → rename).
#[cfg(target_os = "macos")]
pub async fn write_plist(binary_path: &str, file_path: Option<&Path>) -> Result<(), AppError> {
    let target = file_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PATHS.plist_file.clone());

    // Ensure parent directory exists.
    if let Some(dir) = target.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }

    let content = generate_plist(binary_path);
    let tmp = target.with_extension("plist.tmp");

    tokio::fs::write(&tmp, content.as_bytes()).await?;
    tokio::fs::rename(&tmp, &target).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// launchctl helpers
// ---------------------------------------------------------------------------

struct LaunchctlOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

fn run_launchctl(args: &[&str]) -> Result<LaunchctlOutput, AppError> {
    let output = Command::new("launchctl").args(args).output().map_err(|e| {
        AppError::Friendly(FriendlyError::with_hint(
            format!("Failed to run launchctl: {e}"),
            "Make sure you are on macOS.",
        ))
    })?;

    Ok(LaunchctlOutput {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Load the launchd agent (`launchctl load -w <plist>`).
#[cfg(target_os = "macos")]
pub async fn load_agent() -> Result<(), AppError> {
    let plist = PATHS.plist_file.to_string_lossy().into_owned();
    let out = run_launchctl(&["load", "-w", &plist])?;
    if out.exit_code != 0 {
        let hint = launchctl_hint(&out.stderr);
        return Err(AppError::Launchctl(LaunchctlError::new(out.stderr, hint)));
    }
    Ok(())
}

/// Unload the launchd agent (`launchctl unload -w <plist>`).
pub async fn unload_agent() -> Result<(), AppError> {
    let plist = PATHS.plist_file.to_string_lossy().into_owned();
    let out = run_launchctl(&["unload", "-w", &plist])?;
    if out.exit_code != 0 {
        let hint = launchctl_hint(&out.stderr);
        return Err(AppError::Launchctl(LaunchctlError::new(out.stderr, hint)));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Status parsing
// ---------------------------------------------------------------------------

/// Parse the output of `launchctl print gui/<uid>/net.devm8`.
fn parse_print_output(output: &str) -> AgentStatus {
    let mut status = AgentStatus::default();

    for line in output.lines() {
        let line = line.trim();

        if let Some(rest) = line.strip_prefix("pid = ") {
            if let Ok(pid) = rest.trim().parse::<u32>() {
                status.pid = Some(pid);
                status.running = true;
            }
        } else if let Some(rest) = line.strip_prefix("state = ") {
            let state = rest.trim();
            if state == "running" {
                status.running = true;
            }
        } else if let Some(rest) = line.strip_prefix("last exit code = ") {
            if let Ok(code) = rest.trim().parse::<i32>() {
                status.exit_code = Some(code);
            }
        }
    }

    status
}

/// Parse the output of `launchctl list net.devm8`.
/// The output format is: `pid\texitcode\tlabel`
fn parse_list_output(output: &str) -> AgentStatus {
    let mut status = AgentStatus::default();

    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() == 3 && parts[2].trim() == "net.devm8" {
            let pid_str = parts[0].trim();
            let exit_str = parts[1].trim();

            if let Ok(pid) = pid_str.parse::<u32>() {
                status.pid = Some(pid);
                status.running = true;
            }
            if let Ok(code) = exit_str.parse::<i32>() {
                status.exit_code = Some(code);
            }
        }
    }

    status
}

/// Query the launchd agent status.
///
/// Tries `launchctl print gui/<uid>/net.devm8` first; falls back to
/// `launchctl list net.devm8`.  Returns `{ running: false }` if neither
/// yields useful output.
pub async fn agent_status() -> AgentStatus {
    // Attempt the richer `print` subcommand first.
    let uid = libc_uid();
    let service = format!("gui/{uid}/net.devm8");

    if let Ok(out) = run_launchctl(&["print", &service]) {
        if out.exit_code == 0 && !out.stdout.is_empty() {
            return parse_print_output(&out.stdout);
        }
    }

    // Fall back to `list`.
    if let Ok(out) = run_launchctl(&["list", "net.devm8"]) {
        if out.exit_code == 0 && !out.stdout.is_empty() {
            return parse_list_output(&out.stdout);
        }
    }

    AgentStatus::default()
}

/// Get the current user's UID via libc or by parsing `id -u`.
fn libc_uid() -> u32 {
    // Use `id -u` to avoid a libc dependency here.
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(501)
}
