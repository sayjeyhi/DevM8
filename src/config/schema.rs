use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Regex constants (mirrors TypeScript)
// ---------------------------------------------------------------------------

/// `^\\d+:[A-Za-z0-9_-]{20,}$`
pub const BOT_TOKEN_REGEX: &str = r"^\d+:[A-Za-z0-9_-]{20,}$";

/// `^[A-Z][A-Z0-9_]+$`
pub const PROJECT_KEY_REGEX: &str = r"^[A-Z][A-Z0-9_]+$";

/// Simple email pattern: `^[^\s@]+@[^\s@]+\.[^\s@]+$`
pub const EMAIL_REGEX: &str = r"^[^\s@]+@[^\s@]+\.[^\s@]+$";

// ---------------------------------------------------------------------------
// Sub-configs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    #[serde(default)]
    pub allowed_user_ids: Vec<i64>,
    /// Single admin user ID. Only this user can run add_project, logs, clone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_user_id: Option<i64>,
    /// Per-project access: project key → list of allowed user IDs.
    /// If a project key is absent, all allowed_user_ids can access it.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub project_access: HashMap<String, Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraConfig {
    pub base_url: String,
    pub api_token: String,
    pub email: String,
    pub project_keys: Vec<String>,
}

/// Per-user Jira credentials.
/// TOML key: [user_jira.<telegram_user_id>]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserJiraConfig {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
    /// Project keys this user can access on their Jira instance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_keys: Vec<String>,
    /// Status names the user wants to see in the "Filter by status" picker.
    /// Empty means show all statuses from the Jira instance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub favorite_statuses: Vec<String>,
}

fn default_sandbox() -> bool {
    cfg!(target_os = "linux")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeConfig {
    pub binary_path: String,
    pub api_key: Option<String>,
    pub timeout_ms: Option<u64>,
    /// Isolate each Claude subprocess with bubblewrap (Linux only).
    /// Defaults to true on Linux, false on macOS.
    #[serde(default = "default_sandbox")]
    pub sandbox: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppSettings {
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            log_level: LogLevel::Info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Debug,
    Error,
}

fn default_log_level() -> LogLevel {
    LogLevel::Info
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info => write!(f, "info"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "error" => Ok(LogLevel::Error),
            other => Err(format!("unknown log level: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    pub user_token: String,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
}

fn default_poll_interval_ms() -> u64 {
    30_000
}

// ---------------------------------------------------------------------------
// Top-level AppConfig
// ---------------------------------------------------------------------------

/// Mirrors `AppConfigSchema` from TypeScript.
/// Field names match the TOML keys (snake_case).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub telegram: TelegramConfig,
    /// Global Jira config is optional — users configure their own via /jira.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira: Option<JiraConfig>,
    pub claude: ClaudeConfig,

    /// Map from PROJECT_KEY → list of repo paths
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "repos")]
    pub projects: Option<HashMap<String, Vec<String>>>,

    #[serde(default)]
    pub app: AppSettings,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack: Option<SlackConfig>,

    /// Per-user Jira credential overrides. Key is Telegram user_id as a string.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub user_jira: HashMap<String, UserJiraConfig>,
}
