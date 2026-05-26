use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::config::schema::{AppConfig, UserJiraConfig};
use crate::shared::errors::{AppError, ConfigMissingError, FriendlyError};
use crate::shared::paths::PATHS;

/// Guards the read-modify-write cycle so concurrent Telegram handlers
/// (e.g. two users finishing /jira setup simultaneously) cannot clobber
/// each other's credentials.
static CONFIG_WRITE_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_path(config_path: Option<&Path>) -> PathBuf {
    config_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PATHS.config_file.clone())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load and validate the configuration file.
///
/// - Returns `ConfigMissingError` when the file does not exist.
/// - Returns `FriendlyError` for TOML parse / validation failures.
pub fn load_config(config_path: Option<&Path>) -> Result<AppConfig, AppError> {
    let path = resolve_path(config_path);

    let raw = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::ConfigMissing(ConfigMissingError::new(path.display().to_string()))
        } else {
            AppError::Io(e)
        }
    })?;

    let config: AppConfig = toml::from_str(&raw).map_err(|e| {
        AppError::Friendly(FriendlyError::with_hint(
            format!("Failed to parse config file at {}: {e}", path.display()),
            "Run `devm8 config` to recreate or fix the configuration.",
        ))
    })?;

    validate_config(&config)?;

    Ok(config)
}

/// Returns `true` if a config file exists at the given path (or the default).
pub fn config_exists(config_path: Option<&Path>) -> bool {
    resolve_path(config_path).exists()
}

/// Write `config` to `config_path` (or the default) atomically with mode 0o600.
///
/// Writes to a `.tmp` file first, then renames — so readers never see a
/// partial write.
pub fn write_config(config: &AppConfig, config_path: Option<&Path>) -> Result<(), AppError> {
    let path = resolve_path(config_path);

    // Ensure parent directory exists.
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    let toml_str = toml::to_string_pretty(config)?;

    // Write to a temp file in the same directory so the rename is atomic.
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, &toml_str)?;

    // Set restrictive permissions on Unix before moving into place.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
    }

    fs::rename(&tmp_path, &path)?;

    Ok(())
}

/// Atomically update (or remove) a single user's Jira credentials.
///
/// Holds `CONFIG_WRITE_LOCK` for the entire read → mutate → write cycle so
/// concurrent calls from different Telegram handlers cannot race each other.
pub fn update_user_jira(user_id: i64, cfg: Option<&UserJiraConfig>) -> Result<(), AppError> {
    let _guard = CONFIG_WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut config = load_config(None)?;
    let key = user_id.to_string();
    match cfg {
        Some(c) => {
            config.user_jira.insert(key, c.clone());
        }
        None => {
            config.user_jira.remove(&key);
        }
    }
    write_config(&config, None)
}

// ---------------------------------------------------------------------------
// Internal validation (business-rule checks beyond serde)
// ---------------------------------------------------------------------------

fn validate_config(config: &AppConfig) -> Result<(), AppError> {
    use crate::config::validators;

    // Telegram
    if let Some(msg) = validators::validate_bot_token(&config.telegram.bot_token) {
        return Err(AppError::Friendly(FriendlyError::with_hint(
            format!("telegram.bot_token: {msg}"),
            "Check your Telegram bot token format.",
        )));
    }

    // Jira
    if let Some(msg) = validators::validate_jira_base_url(&config.jira.base_url) {
        return Err(AppError::Friendly(FriendlyError::with_hint(
            format!("jira.base_url: {msg}"),
            "The Jira base URL must start with https://",
        )));
    }
    if let Some(msg) = validators::validate_email(&config.jira.email) {
        return Err(AppError::Friendly(FriendlyError::with_hint(
            format!("jira.email: {msg}"),
            "Provide a valid email address.",
        )));
    }
    if let Some(msg) = validators::validate_api_token(&config.jira.api_token) {
        return Err(AppError::Friendly(FriendlyError::with_hint(
            format!("jira.api_token: {msg}"),
            "The API token must not be empty.",
        )));
    }
    for key in &config.jira.project_keys {
        if let Some(msg) = validators::validate_project_key(key) {
            return Err(AppError::Friendly(FriendlyError::with_hint(
                format!("jira.project_keys: {msg}"),
                "Project keys must match ^[A-Z][A-Z0-9_]+$",
            )));
        }
    }

    // Per-user Jira configs
    for (uid, ucfg) in &config.user_jira {
        if let Some(msg) = validators::validate_jira_base_url(&ucfg.base_url) {
            return Err(AppError::Friendly(FriendlyError::with_hint(
                format!("user_jira.{uid}.base_url: {msg}"),
                "The Jira base URL must start with https://",
            )));
        }
        if let Some(msg) = validators::validate_email(&ucfg.email) {
            return Err(AppError::Friendly(FriendlyError::with_hint(
                format!("user_jira.{uid}.email: {msg}"),
                "Provide a valid email address.",
            )));
        }
        if let Some(msg) = validators::validate_api_token(&ucfg.api_token) {
            return Err(AppError::Friendly(FriendlyError::with_hint(
                format!("user_jira.{uid}.api_token: {msg}"),
                "The API token must not be empty.",
            )));
        }
        for key in &ucfg.project_keys {
            if let Some(msg) = validators::validate_project_key(key) {
                return Err(AppError::Friendly(FriendlyError::with_hint(
                    format!("user_jira.{uid}.project_keys: {msg}"),
                    "Project keys must match ^[A-Z][A-Z0-9_]+$",
                )));
            }
        }
    }

    // Claude
    if let Some(msg) = validators::validate_binary_path(&config.claude.binary_path) {
        return Err(AppError::Friendly(FriendlyError::with_hint(
            format!("claude.binary_path: {msg}"),
            "Make sure the Claude binary exists at the given path.",
        )));
    }

    Ok(())
}
