use regex::Regex;
use std::sync::OnceLock;

use crate::config::schema::{BOT_TOKEN_REGEX, EMAIL_REGEX, PROJECT_KEY_REGEX};

// ---------------------------------------------------------------------------
// Compiled regex helpers
// ---------------------------------------------------------------------------

fn bot_token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(BOT_TOKEN_REGEX).unwrap())
}

fn project_key_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(PROJECT_KEY_REGEX).unwrap())
}

fn email_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(EMAIL_REGEX).unwrap())
}

fn slack_token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^xoxp-").unwrap())
}

// ---------------------------------------------------------------------------
// Individual validators
// Each returns `None` on success, `Some(error_message)` on failure.
// ---------------------------------------------------------------------------

/// Validate a Telegram bot token (`123456:ABCxxx…`).
pub fn validate_bot_token(v: &str) -> Option<String> {
    if bot_token_re().is_match(v) {
        None
    } else {
        Some("Bot token must match <id>:<secret> (e.g. 123456:ABCdef…)".to_string())
    }
}

/// Validate a comma-separated list of positive Telegram user IDs.
/// Requires at least one valid ID.
pub fn validate_allowed_user_ids(v: &str) -> Option<String> {
    let ids: Vec<&str> = v.split(',').map(str::trim).collect();
    if ids.is_empty() || ids.iter().all(|s| s.is_empty()) {
        return Some("At least one user ID is required".to_string());
    }
    for id in &ids {
        if id.is_empty() {
            continue;
        }
        match id.parse::<i64>() {
            Ok(n) if n > 0 => {}
            _ => return Some(format!("'{id}' is not a positive integer")),
        }
    }
    None
}

/// Validate a Jira base URL — must start with `https://`.
pub fn validate_jira_base_url(v: &str) -> Option<String> {
    if v.starts_with("https://") {
        None
    } else {
        Some("Jira base URL must start with https://".to_string())
    }
}

/// Validate an API token — must be non-empty.
pub fn validate_api_token(v: &str) -> Option<String> {
    if v.trim().is_empty() {
        Some("API token must not be empty".to_string())
    } else {
        None
    }
}

/// Validate an email address.
pub fn validate_email(v: &str) -> Option<String> {
    if email_re().is_match(v) {
        None
    } else {
        Some(format!("'{v}' is not a valid email address"))
    }
}

/// Validate a single Jira project key (e.g. `MYPROJ`).
pub fn validate_project_key(v: &str) -> Option<String> {
    if project_key_re().is_match(v) {
        None
    } else {
        Some(format!(
            "'{v}' is not a valid project key (must match ^[A-Z][A-Z0-9_]+$)"
        ))
    }
}

/// Validate a comma-separated list of project keys.
pub fn validate_project_keys(v: &str) -> Option<String> {
    let keys: Vec<&str> = v
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if keys.is_empty() {
        return Some("At least one project key is required".to_string());
    }
    for k in &keys {
        if let Some(msg) = validate_project_key(k) {
            return Some(msg);
        }
    }
    None
}

/// Validate that a binary path refers to an existing, executable file.
pub fn validate_binary_path(v: &str) -> Option<String> {
    let path = std::path::Path::new(v);
    if !path.is_file() {
        return Some(format!("Binary not found at '{v}'"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.permissions().mode() & 0o111 == 0 {
                return Some(format!(
                    "Binary at '{v}' is not executable — run: chmod +x {v}"
                ));
            }
        }
    }
    None
}

/// Validate a newline- or comma-separated list of directory paths.
/// All paths must exist as directories.
pub fn validate_repo_paths(v: &str) -> Option<String> {
    let paths: Vec<&str> = v
        .split(['\n', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    if paths.is_empty() {
        return Some("At least one repository path is required".to_string());
    }

    for p in &paths {
        if !std::path::Path::new(p).is_dir() {
            return Some(format!("Directory not found: '{p}'"));
        }
    }
    None
}

/// Validate a Slack user token — must start with `xoxp-`.
pub fn validate_slack_user_token(v: &str) -> Option<String> {
    if slack_token_re().is_match(v) {
        None
    } else {
        Some("Slack user token must start with 'xoxp-'".to_string())
    }
}
