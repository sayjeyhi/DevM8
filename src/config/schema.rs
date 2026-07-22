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
/// TOML key: [user_jira.<telegram_user_id_or_email>]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiTool {
    #[default]
    Claude,
    Kiro,
}

impl std::fmt::Display for AiTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiTool::Claude => write!(f, "claude"),
            AiTool::Kiro => write!(f, "kiro"),
        }
    }
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
    /// Extra absolute paths to bind and prepend to PATH inside the sandbox.
    /// Use this to expose tools (e.g. a specific nvm node version) that the
    /// daemon cannot auto-discover because they are not on the service PATH.
    /// Example: ["/home/jafar/.nvm/versions/node/v24.16.0/bin"]
    #[serde(default)]
    pub sandbox_extra_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiroConfig {
    pub binary_path: String,
    pub timeout_ms: Option<u64>,
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
    /// xoxp-… user token used by the legacy Slack poller (DM forwarding).
    pub user_token: String,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,

    // ---- Slack Bot (Socket Mode) ----
    /// xoxb-… Bot token.  Required for the interactive Slack bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,

    /// xapp-… App-level token.  Required for Socket Mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_token: Option<String>,

    /// Slack user IDs allowed to interact with the bot (e.g. ["U1234567"]).
    /// Empty list = everyone in the workspace is allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_user_ids: Vec<String>,

    /// Admin Slack user ID.  Only this user can run admin commands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_user_id: Option<String>,

    /// Per-project access: project key → list of allowed Slack user IDs.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub project_access: std::collections::HashMap<String, Vec<String>>,
}

fn default_poll_interval_ms() -> u64 {
    30_000
}

fn default_teams_port() -> u16 {
    3978
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsConfig {
    /// Azure Bot Registration App ID (from Azure Portal → Configuration tab).
    pub app_id: String,
    /// Azure Bot App Password / client secret.
    pub app_password: String,
    /// Azure AD Tenant ID — required for Single Tenant bots.
    /// Find it in Azure Portal → Microsoft Entra ID → Overview → Tenant ID.
    /// Leave unset only if you created a Multi Tenant bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// Local port the Teams webhook server listens on. Default: 3978.
    #[serde(default = "default_teams_port")]
    pub port: u16,
    /// Teams user AAD object IDs allowed to interact with the bot.
    /// Empty list = everyone is allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_user_ids: Vec<String>,
    /// Admin Teams user AAD object ID. Only this user can run admin commands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_user_id: Option<String>,
    /// Per-project access: project key → list of allowed Teams user AAD object IDs.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub project_access: std::collections::HashMap<String, Vec<String>>,
}

fn default_api_port() -> u16 {
    7887
}

/// Local API server for devm8-client. Disabled unless explicitly configured —
/// mirrors the opt-in pattern of `SlackConfig`/`TeamsConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_api_port")]
    pub port: u16,
    /// Interface to bind. Defaults to all interfaces — the real access boundary
    /// is the bearer token, not the bind address (see devm8-client docs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_addr: Option<String>,
    /// Optional TLS cert/key pair (e.g. from `tailscale cert`). When unset, the
    /// server speaks plain HTTP — safe over a Tailscale tailnet, since transport
    /// is already WireGuard-encrypted node-to-node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_cert_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_key_path: Option<String>,
}

impl SlackConfig {
    /// Returns true if the Slack bot (Socket Mode) is fully configured.
    pub fn bot_enabled(&self) -> bool {
        self.bot_token.is_some() && self.app_token.is_some()
    }
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
    /// Which AI tool to use. Defaults to Claude for backward compatibility.
    #[serde(default)]
    pub ai_tool: AiTool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<ClaudeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kiro: Option<KiroConfig>,

    /// Map from PROJECT_KEY → list of repo paths
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "repos")]
    pub projects: Option<HashMap<String, Vec<String>>>,

    #[serde(default)]
    pub app: AppSettings,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack: Option<SlackConfig>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<TeamsConfig>,

    /// Local API server for devm8-client. Unset = disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<ApiConfig>,

    /// Per-user Jira credential overrides. Key is a Telegram user_id or a
    /// devm8 email (for devm8-client's CLI-native setup), as a string.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub user_jira: HashMap<String, UserJiraConfig>,
}
