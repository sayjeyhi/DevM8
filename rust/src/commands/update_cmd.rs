use std::io::Write as _;

use sha2::{Digest, Sha256};

use crate::daemon::launchd::agent_status;
use crate::daemon::launchd::{load_agent, unload_agent};
use crate::shared::errors::{AppError, FriendlyError};

const REPO: &str = "sayjeyhi/DevM8";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Binary naming
// ---------------------------------------------------------------------------

fn binary_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("devm8-macos-arm64"),
        ("macos", "x86_64")  => Some("devm8-macos-x64"),
        ("linux", "x86_64")  => Some("devm8-linux-x64"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Version fetching via GitHub redirect
// ---------------------------------------------------------------------------

async fn fetch_latest_version(repo: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let url = format!("https://github.com/{repo}/releases/latest");
    let resp = client.get(&url).send().await?;

    if resp.status() == 302 || resp.status() == 301 {
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let tag = location.split('/').last().unwrap_or("").to_string();
        if tag.starts_with('v') {
            return Ok(tag);
        }
    }

    anyhow::bail!("Could not determine latest version from GitHub redirect for {repo}")
}

// ---------------------------------------------------------------------------
// Simple semver comparison (handles "v0.1.2" and "0.1.2")
// ---------------------------------------------------------------------------

fn strip_v(s: &str) -> &str {
    s.strip_prefix('v').unwrap_or(s)
}

/// Returns `true` if `a` is strictly newer than `b`.
fn is_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u64, u64, u64) {
        let parts: Vec<u64> = strip_v(s)
            .splitn(3, '.')
            .map(|p| p.parse().unwrap_or(0))
            .collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(a) > parse(b)
}

// ---------------------------------------------------------------------------
// SHA256 verification
// ---------------------------------------------------------------------------

async fn verify_sha256(data: &[u8], checksums_text: &str, filename: &str) -> anyhow::Result<()> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hex::encode(hasher.finalize());

    for line in checksums_text.lines() {
        let parts: Vec<&str> = line.splitn(2, "  ").collect();
        if parts.len() == 2 && parts[1].trim() == filename {
            if parts[0].trim() == digest {
                return Ok(());
            } else {
                anyhow::bail!(
                    "SHA256 mismatch for {filename}: expected {}, got {}",
                    parts[0].trim(),
                    digest
                );
            }
        }
    }

    anyhow::bail!("SHA256 checksum for '{filename}' not found in checksums.txt")
}

// ---------------------------------------------------------------------------
// Public command
// ---------------------------------------------------------------------------

pub async fn update_command() -> Result<(), AppError> {
    // Dev builds skip update check.
    if CURRENT_VERSION == "0.0.0-dev" {
        println!("Skipping update check for dev build.");
        return Ok(());
    }

    let bin_name = binary_name().ok_or_else(|| {
        AppError::Friendly(FriendlyError::with_hint(
            "Unsupported platform".to_string(),
            "Pre-built binaries are available for macOS (arm64/x64) and Linux (x64)."
                .to_string(),
        ))
    })?;

    // ------------------------------------------------------------------
    // Fetch the latest tag from GitHub.
    // ------------------------------------------------------------------
    println!("Checking for updates…");
    let latest = fetch_latest_version(REPO)
        .await
        .map_err(|e| AppError::Other(e))?;

    if !is_newer(&latest, CURRENT_VERSION) {
        println!("Already up to date ({})", CURRENT_VERSION);
        return Ok(());
    }

    println!(
        "Update available: {} → {}",
        CURRENT_VERSION,
        strip_v(&latest)
    );

    // ------------------------------------------------------------------
    // Download the binary and checksum file.
    // ------------------------------------------------------------------
    let tag = &latest;
    let base_url = format!("https://github.com/{REPO}/releases/download/{tag}");
    let bin_url  = format!("{base_url}/{bin_name}");
    let sum_url  = format!("{base_url}/checksums.txt");

    println!("Downloading {}…", bin_name);
    let client = reqwest::Client::new();

    let bin_bytes = client
        .get(&bin_url)
        .send()
        .await
        .map_err(|e| AppError::Other(e.into()))?
        .error_for_status()
        .map_err(|e| AppError::Other(e.into()))?
        .bytes()
        .await
        .map_err(|e| AppError::Other(e.into()))?;

    let checksums = client
        .get(&sum_url)
        .send()
        .await
        .map_err(|e| AppError::Other(e.into()))?
        .error_for_status()
        .map_err(|e| AppError::Other(e.into()))?
        .text()
        .await
        .map_err(|e| AppError::Other(e.into()))?;

    // ------------------------------------------------------------------
    // Verify checksum.
    // ------------------------------------------------------------------
    verify_sha256(&bin_bytes, &checksums, bin_name)
        .await
        .map_err(|e| AppError::Other(e))?;

    println!("Checksum verified.");

    // ------------------------------------------------------------------
    // Stop the service if it is running on macOS.
    // ------------------------------------------------------------------
    #[cfg(target_os = "macos")]
    let was_running = {
        let s = agent_status().await;
        if s.running {
            println!("Stopping running daemon…");
            let _ = unload_agent().await;
        }
        s.running
    };

    #[cfg(not(target_os = "macos"))]
    let was_running = false;

    // ------------------------------------------------------------------
    // Overwrite the current binary atomically.
    // ------------------------------------------------------------------
    let exe_path = std::env::current_exe()
        .and_then(|p| std::fs::canonicalize(p))
        .map_err(|e| {
            AppError::Friendly(FriendlyError::with_hint(
                format!("Cannot determine current executable path: {e}"),
                "Please update manually.".to_string(),
            ))
        })?;

    let tmp_path = exe_path.with_extension("new");

    {
        let mut f = std::fs::File::create(&tmp_path).map_err(AppError::Io)?;
        f.write_all(&bin_bytes).map_err(AppError::Io)?;
    }

    // chmod 755
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
            .map_err(AppError::Io)?;
    }

    // Remove quarantine attribute on macOS.
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("xattr")
            .args(["-d", "com.apple.quarantine", &tmp_path.to_string_lossy()])
            .output();
    }

    std::fs::rename(&tmp_path, &exe_path).map_err(AppError::Io)?;

    println!("Updated to {}.", strip_v(&latest));

    // ------------------------------------------------------------------
    // Restart the service if it was running.
    // ------------------------------------------------------------------
    #[cfg(target_os = "macos")]
    if was_running {
        println!("Restarting daemon…");
        let _ = load_agent().await;
    }

    Ok(())
}
