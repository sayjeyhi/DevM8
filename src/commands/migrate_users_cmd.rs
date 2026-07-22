use inquire::{Confirm, Text};

use crate::config::loader::load_config;
use crate::config::schema::AppConfig;
use crate::config::validators::validate_email;
use crate::db::{migrate::backfill_from_config, Db};
use crate::shared::errors::{AppError, FriendlyError};
use crate::shared::paths::PATHS;

/// Interactive one-time (repeatable) migration: attach a real email to every
/// channel identity (Telegram/Slack/Teams user ID) that devm8 has only ever
/// seen as a raw platform ID, and optionally issue a devm8-client pairing code.
pub async fn migrate_users_command() -> Result<(), AppError> {
    let config = load_config(None).map_err(|e| {
        if let AppError::ConfigMissing(_) = &e {
            AppError::Friendly(FriendlyError::with_hint(
                "No config found.".to_string(),
                "Run `devm8 config` first to set up your configuration.".to_string(),
            ))
        } else {
            e
        }
    })?;

    let db = Db::open(&PATHS.db_file).map_err(|e| {
        AppError::Friendly(FriendlyError::new(format!("Failed to open database: {e}")))
    })?;

    backfill_from_config(&db, &config).await.map_err(|e| {
        AppError::Friendly(FriendlyError::new(format!(
            "Migration backfill failed: {e}"
        )))
    })?;

    let placeholders = db.list_placeholder_users().await.map_err(|e| {
        AppError::Friendly(FriendlyError::new(format!("Failed to read database: {e}")))
    })?;

    if placeholders.is_empty() {
        println!("No unmapped channel identities found — every known user already has an email.");
    } else {
        println!(
            "Found {} channel identit{} without a real email attached:\n",
            placeholders.len(),
            if placeholders.len() == 1 { "y" } else { "ies" }
        );

        for (channel, external_id, placeholder_email) in &placeholders {
            println!("  {channel}: {external_id}");
            let email = Text::new(&format!("  Email for this {channel} user (blank to skip):"))
                .prompt_skippable()
                .map_err(|e| AppError::Friendly(FriendlyError::new(format!("Prompt failed: {e}"))))?
                .unwrap_or_default();

            let email = email.trim();
            if email.is_empty() {
                continue;
            }
            if let Some(msg) = validate_email(email) {
                println!("  Skipping — {msg}");
                continue;
            }

            db.rename_or_merge_user_email(placeholder_email, email)
                .await
                .map_err(|e| {
                    AppError::Friendly(FriendlyError::new(format!("Failed to update user: {e}")))
                })?;
            println!("  Mapped {channel}:{external_id} -> {email}\n");
        }
    }

    let issue_code = Confirm::new("Issue a devm8-client pairing code now?")
        .with_default(false)
        .prompt()
        .unwrap_or(false);

    if issue_code {
        let email = Text::new("Email to pair:")
            .prompt()
            .map_err(|e| AppError::Friendly(FriendlyError::new(format!("Prompt failed: {e}"))))?;
        let email = email.trim();
        if let Some(msg) = validate_email(email) {
            return Err(AppError::Friendly(FriendlyError::new(msg)));
        }
        let code = db.create_pairing_code(email, 10).await.map_err(|e| {
            AppError::Friendly(FriendlyError::new(format!(
                "Failed to create pairing code: {e}"
            )))
        })?;

        let readiness = ensure_api_server_running(&config).await;

        println!(
            "\nPairing code for {email}: {code}\n\
             Valid for 10 minutes. On the client machine, run:\n\
             \n  devm8-client login --server {} --code {code}\n\
             \n{}\n",
            readiness.server_url, readiness.status_line
        );
    }

    Ok(())
}

/// Result of [`ensure_api_server_running`]: a human-readable status line to
/// print alongside the pairing instructions, and the best guess at a
/// `--server` URL to hand the user (falls back to a placeholder when it
/// can't be determined).
struct ApiReadiness {
    status_line: String,
    server_url: String,
}

impl ApiReadiness {
    fn placeholder(status_line: String) -> Self {
        Self {
            status_line,
            server_url: "<your-server-url>".to_string(),
        }
    }
}

/// Best-effort Tailscale MagicDNS name (falls back to the first Tailscale IP)
/// for this machine, or `None` if the `tailscale` CLI isn't installed or the
/// backend isn't running. Used to prefill `--server` in the login hint, since
/// `devm8-client` is expected to reach the server over a tailnet (see
/// README.md's devm8-client section).
async fn tailscale_self_address() -> Option<String> {
    let output = tokio::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await
        .ok()?;

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    if value.get("BackendState").and_then(|v| v.as_str()) != Some("Running") {
        return None;
    }

    let slf = value.get("Self")?;
    let dns_name = slf
        .get("DNSName")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_end_matches('.').to_string())
        .filter(|s| !s.is_empty());
    if let Some(name) = dns_name {
        return Some(name);
    }

    slf.get("TailscaleIPs")?
        .as_array()?
        .first()?
        .as_str()
        .map(|s| s.to_string())
}

/// Make sure the devm8 daemon (and therefore the devm8-client API it hosts,
/// see `src/api/mod.rs`) is actually up before we hand out a pairing code —
/// starting the daemon if it isn't, and confirming the API port is reachable
/// rather than trusting the daemon's fire-and-forget spawn of it (see
/// `src/bot/polling.rs`). Also checks Tailscale so the printed login command
/// can carry a real `--server` address instead of a placeholder.
#[cfg(any(target_os = "macos", target_os = "linux"))]
async fn ensure_api_server_running(config: &AppConfig) -> ApiReadiness {
    use std::path::PathBuf;
    use std::time::Duration;

    use serde_json::json;
    use tokio::net::TcpStream;
    use tokio::time::timeout;

    use crate::daemon::agent_status;
    use crate::logger::{append_to_log_file, Level};

    let Some(api_config) = config.api.as_ref() else {
        append_to_log_file(
            &PATHS.log_file,
            Level::Warn,
            "migrate-users: no [api] section configured, devm8-client cannot connect",
            None,
        );
        return ApiReadiness::placeholder(
            "Server: not started — no [api] section in config, so devm8-client has \
             nothing to connect to. Add one with `devm8 config`."
                .to_string(),
        );
    };

    let mut status = agent_status().await;
    if !status.running {
        println!("devm8 daemon is not running — starting it now…");
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .unwrap_or_else(|| PathBuf::from("devm8"));

        match tokio::process::Command::new(&exe)
            .arg("start")
            .status()
            .await
        {
            Ok(s) if s.success() => status = agent_status().await,
            Ok(s) => {
                append_to_log_file(
                    &PATHS.log_file,
                    Level::Error,
                    "migrate-users: devm8 start failed",
                    Some(&json!({ "exit_code": s.code() })),
                );
                return ApiReadiness::placeholder(format!(
                    "Server: failed to start (exit code {:?}) — run `devm8 logs` to investigate.",
                    s.code()
                ));
            }
            Err(e) => {
                append_to_log_file(
                    &PATHS.log_file,
                    Level::Error,
                    "migrate-users: failed to spawn devm8 start",
                    Some(&json!({ "error": e.to_string() })),
                );
                return ApiReadiness::placeholder(format!(
                    "Server: failed to start ({e}) — run `devm8 start` manually."
                ));
            }
        }
    }

    if !status.running {
        append_to_log_file(
            &PATHS.log_file,
            Level::Warn,
            "migrate-users: daemon still not running after start attempt",
            None,
        );
        return ApiReadiness::placeholder(
            "Server: still not running after a start attempt — run `devm8 status` / \
             `devm8 logs` to investigate."
                .to_string(),
        );
    }

    let probe_host = match api_config.bind_addr.as_deref() {
        Some(addr) if addr != "0.0.0.0" => addr,
        _ => "127.0.0.1",
    };
    let addr = format!("{probe_host}:{}", api_config.port);
    let reachable = timeout(Duration::from_secs(2), TcpStream::connect(&addr))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);

    let pid = status.pid.unwrap_or(0);

    let tailscale_addr = tailscale_self_address().await;
    let scheme = if api_config.tls_cert_path.is_some() {
        "https"
    } else {
        "http"
    };
    let server_url = tailscale_addr
        .as_ref()
        .map(|host| format!("{scheme}://{host}:{}", api_config.port));

    let tailscale_note = if server_url.is_none() {
        " Tailscale doesn't appear to be running, so `--server` above is a placeholder — \
         start it (`tailscale up`) or fill in the address yourself."
    } else {
        ""
    };

    if reachable {
        append_to_log_file(
            &PATHS.log_file,
            Level::Info,
            "migrate-users: confirmed api server reachable",
            Some(&json!({ "pid": pid, "addr": addr, "tailscale": tailscale_addr })),
        );
        ApiReadiness {
            status_line: format!(
                "Server: running (PID {pid}), API listening on {addr}.{tailscale_note}"
            ),
            server_url: server_url.unwrap_or_else(|| "<your-server-url>".to_string()),
        }
    } else {
        append_to_log_file(
            &PATHS.log_file,
            Level::Warn,
            "migrate-users: api port not reachable",
            Some(&json!({ "pid": pid, "addr": addr, "tailscale": tailscale_addr })),
        );
        ApiReadiness {
            status_line: format!(
                "Server: daemon running (PID {pid}) but API port {addr} isn't reachable yet — \
                 check `devm8 logs`.{tailscale_note}"
            ),
            server_url: server_url.unwrap_or_else(|| "<your-server-url>".to_string()),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
async fn ensure_api_server_running(_config: &AppConfig) -> ApiReadiness {
    ApiReadiness::placeholder(
        "Server: daemon management isn't supported on this platform (macOS/Linux only)."
            .to_string(),
    )
}
