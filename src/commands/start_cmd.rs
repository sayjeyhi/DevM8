use crate::shared::errors::{AppError, FriendlyError};

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn start_command() -> Result<(), AppError> {
    Err(AppError::Friendly(FriendlyError::with_hint(
        "DevM8 requires macOS or Linux".to_string(),
        "Supported: launchd (macOS) and systemd (Linux).".to_string(),
    )))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub async fn start_command() -> Result<(), AppError> {
    use std::time::{Duration, Instant};

    use serde_json::json;

    use crate::config::loader::{config_exists, load_config, write_config};
    use crate::config::wizard::run_wizard;
    use crate::daemon::pid::read_pid;
    use crate::daemon::{agent_status, load_agent, unload_agent, write_service_file};
    use crate::logger::{append_to_log_file, Level};
    use crate::shared::paths::PATHS;

    let paths = &*PATHS;

    // Ensure the service directory exists.
    tokio::fs::create_dir_all(&paths.service_dir).await?;

    // Load (or create) configuration.
    let config = if config_exists(None) {
        load_config(None)?
    } else {
        println!("No configuration found. Running setup wizard…\n");
        let config = run_wizard(None)?;
        write_config(&config, None)?;
        config
    };

    // Verify the AI tool binary is executable.
    use crate::config::schema::AiTool;
    let (binary, tool_name) = match config.ai_tool {
        AiTool::Claude => {
            let path = config
                .claude
                .as_ref()
                .map(|c| c.binary_path.as_str())
                .unwrap_or("");
            (path.to_string(), "Claude")
        }
        AiTool::Kiro => {
            let path = config
                .kiro
                .as_ref()
                .map(|k| k.binary_path.as_str())
                .unwrap_or("");
            (path.to_string(), "Kiro")
        }
    };
    let meta = std::fs::metadata(&binary).map_err(|_| {
        AppError::Friendly(FriendlyError::with_hint(
            format!("{tool_name} binary not found at '{binary}'"),
            "Set the correct path with `devm8 config`.",
        ))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return Err(AppError::Friendly(FriendlyError::with_hint(
                format!("{tool_name} binary at '{binary}' is not executable"),
                "Run `chmod +x <path>` to fix this.",
            )));
        }
    }
    let _ = meta;

    // On Linux with Claude, verify bubblewrap is available when sandbox mode is enabled.
    #[cfg(target_os = "linux")]
    if config.ai_tool == AiTool::Claude {
        if config.claude.as_ref().map(|c| c.sandbox).unwrap_or(false) {
            use std::process::Command as StdCommand;
            if StdCommand::new("bwrap").arg("--version").output().is_err() {
                return Err(AppError::Friendly(FriendlyError::with_hint(
                    "bubblewrap (bwrap) not found — required for sandbox mode".to_string(),
                    "Install with: apt install bubblewrap  \
                     (or set claude.sandbox = false in your config to disable isolation)"
                        .to_string(),
                )));
            }
        }
    }

    // Stop the daemon if already running.
    let current_status = agent_status().await;
    if current_status.running {
        println!("Stopping existing daemon before restart…");
        let _ = unload_agent().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Resolve the current binary path.
    let exe_path = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(p).ok())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "devm8".to_string());

    // Write the service file and start the agent.
    write_service_file(&exe_path, None).await?;
    load_agent().await?;

    #[cfg(target_os = "linux")]
    println!("Tip: run `loginctl enable-linger $USER` to keep the service running after logout.");

    // Wait up to 5 seconds for the daemon to come up (poll every 200 ms).
    let deadline = Instant::now() + Duration::from_millis(5000);
    let mut final_status = agent_status().await;

    while !final_status.running && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
        final_status = agent_status().await;
    }

    if final_status.running {
        let pid = final_status.pid.unwrap_or_else(|| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(read_pid(None))
                    .ok()
                    .flatten()
                    .unwrap_or(0)
            })
        });

        append_to_log_file(
            &paths.log_file,
            Level::Info,
            "service started",
            Some(&json!({ "pid": pid })),
        );

        println!("devm8 started (PID {})", pid);
    } else {
        let code = final_status.exit_code.unwrap_or(-1);
        eprintln!("devm8 failed to start (last exit code: {code})");
        eprintln!("hint: Check the logs with `devm8 logs` for details.");
        std::process::exit(1);
    }

    Ok(())
}
