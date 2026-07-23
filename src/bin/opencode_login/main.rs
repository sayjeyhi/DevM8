//! Login-shell binary for the shared Tailscale-SSH `opencode` account.
//!
//! This is installed as the login shell of a single shared, low-privilege OS
//! account. Whatever the connecting client requests — an interactive session
//! or `ssh host somecommand` — sshd invokes this binary as the login shell,
//! and it *always* runs the flow below, ignoring `argv`/`$SSH_ORIGINAL_COMMAND`
//! entirely. That's intentional: identity and target worktree are derived
//! solely from `tailscale whois` on the connecting peer, so no client-supplied
//! input can redirect this into someone else's worktree.
//!
//! Flow: peer IP (from sshd's `SSH_CLIENT`/`SSH_CONNECTION`) -> `tailscale
//! whois` -> devm8 email -> paired chat identities (`user_channel_identities`)
//! -> the project that email most recently selected in `/ask`
//! (`user_active_project`, set the moment its worktree-backed session is
//! created) -> that project's worktree -> exec `opencode` inside the same
//! bwrap sandbox `/ask`'s CLI button already uses.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use devm8::claude::{ClaudeClient, ClaudeClientConfig};
use devm8::config::load_config;
use devm8::db::Db;
use devm8::git::GitClient;
use devm8::logger::{create_logger, Level};
use devm8::shared::PATHS;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("opencode-login: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let peer_ip = peer_ip_from_ssh_env().context(
        "could not determine the connecting peer's IP \
         (expected $SSH_CLIENT or $SSH_CONNECTION to be set by sshd)",
    )?;

    let email = tailscale_whois_login(&peer_ip)
        .await
        .with_context(|| format!("tailscale whois failed for peer {peer_ip}"))?;

    let db = Db::open(&PATHS.db_file).context("failed to open devm8 database")?;
    let mut identities = db.list_channel_identities_for_email(&email).await?;
    identities.retain(|(channel, _)| channel == "telegram" || channel == "slack");

    if identities.is_empty() {
        println!(
            "No devm8 chat identity is paired with {email} yet.\n\
             Message the bot at least once, have an admin run `devm8 migrate-users`,\n\
             then pair this account with `devm8-client login`, before opencode has a\n\
             worktree to attach to."
        );
        return Ok(());
    }

    let config = load_config(None).context("failed to load devm8 config")?;
    let projects = config.projects.clone().unwrap_or_default();

    let active_project = match db.get_active_project(&email).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("opencode-login: warning: failed to look up active project: {e:#}");
            None
        }
    };

    let worktree = find_worktree(&projects, &identities, active_project.as_deref());
    let Some(worktree) = worktree else {
        let msg = match active_project.as_deref() {
            Some(pkey) => format!(
                "No worktree found for project {pkey} (your most recently selected /ask \
                 project) — re-run /ask, select it again, then reconnect."
            ),
            None => format!(
                "No active /ask session found for {email} — start one in chat first\n\
                 (that's what creates the worktree opencode attaches to), then reconnect."
            ),
        };
        println!("{msg}");
        return Ok(());
    };

    if which("opencode").is_none() {
        bail!(
            "opencode is not installed on this server. Install it once \
             (see https://opencode.ai/docs) and reconnect."
        );
    }

    let claude_cfg = config
        .claude
        .as_ref()
        .context("no [claude] section configured — opencode reuses its api_key")?;

    let logger = Arc::new(create_logger(Level::Info, None, Some(&PATHS.log_file)));
    let client = ClaudeClient::new(
        ClaudeClientConfig {
            binary_path: claude_cfg.binary_path.clone(),
            timeout_ms: None,
            model: None,
            api_key: claude_cfg.api_key.clone(),
            sandbox_enabled: claude_cfg.sandbox,
            sandbox_extra_paths: claude_cfg.sandbox_extra_paths.clone(),
        },
        logger,
    );

    println!("Attaching opencode in {}", worktree.display());

    let mut cmd = client.sandboxed_sh_command(Some(&worktree.to_string_lossy()), "exec opencode");
    // Only needed on the non-sandboxed fallback path (e.g. macOS): the bwrap
    // path already injects this via --setenv inside build_bwrap_base.
    if let Some(key) = claude_cfg
        .api_key
        .clone()
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
    {
        cmd.env("ANTHROPIC_API_KEY", key);
    }
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let status = cmd.status().await.context("failed to launch opencode")?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Find the caller's worktree to attach `opencode` to.
///
/// When `active_project` is `Some`, only that project's repo paths are
/// considered — this is what makes the OpenCode button target the specific
/// project the user selected in `/ask`, rather than whichever project happens
/// to have the most recently modified worktree on disk. `active_project` is
/// `None` only for identities that predate the `user_active_project` table
/// (or never recorded one), in which case we fall back to the old
/// most-recently-modified-across-all-projects heuristic.
fn find_worktree(
    projects: &std::collections::HashMap<String, Vec<String>>,
    identities: &[(String, String)],
    active_project: Option<&str>,
) -> Option<PathBuf> {
    let scoped: Vec<&Vec<String>> = match active_project {
        Some(pkey) => projects
            .iter()
            .filter(|(key, _)| key.as_str() == pkey)
            .map(|(_, paths)| paths)
            .collect(),
        None => projects.values().collect(),
    };

    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
    for repo_paths in scoped {
        for repo_path in repo_paths {
            let git = GitClient::new(repo_path.clone());
            for (_, external_id) in identities {
                let wt = git.worktree_path(external_id);
                let Ok(meta) = std::fs::metadata(&wt) else {
                    continue;
                };
                let Ok(modified) = meta.modified() else {
                    continue;
                };
                if best.as_ref().map(|(_, t)| modified > *t).unwrap_or(true) {
                    best = Some((wt, modified));
                }
            }
        }
    }
    best.map(|(path, _)| path)
}

/// The connecting peer's IP, as sshd exposes it in the child process env —
/// `SSH_CLIENT="<ip> <port> <port>"` or `SSH_CONNECTION="<ip> <port> <ip> <port>"`.
fn peer_ip_from_ssh_env() -> Option<String> {
    for var in ["SSH_CLIENT", "SSH_CONNECTION"] {
        if let Ok(val) = std::env::var(var) {
            if let Some(ip) = val.split_whitespace().next() {
                return Some(ip.to_string());
            }
        }
    }
    None
}

/// Resolve a Tailscale peer IP to its tailnet login (email) via `tailscale whois`.
async fn tailscale_whois_login(peer_ip: &str) -> Result<String> {
    let output = tokio::process::Command::new("tailscale")
        .args(["whois", "--json", peer_ip])
        .output()
        .await
        .context("failed to run `tailscale whois` — is the tailscale CLI installed?")?;

    if !output.status.success() {
        bail!(
            "tailscale whois exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("failed to parse `tailscale whois --json` output")?;
    value
        .get("UserProfile")
        .and_then(|p| p.get("LoginName"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .context("tailscale whois response had no UserProfile.LoginName")
}

fn which(bin: &str) -> Option<String> {
    std::process::Command::new("which")
        .arg(bin)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}
