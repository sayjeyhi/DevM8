use thiserror::Error;

// ---------------------------------------------------------------------------
// Base "friendly" error (message + optional hint shown to user)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FriendlyError {
    pub message: String,
    pub hint: Option<String>,
}

impl FriendlyError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }
}

impl std::fmt::Display for FriendlyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(h) = &self.hint {
            write!(f, "\n  hint: {}", h)?;
        }
        Ok(())
    }
}

impl std::error::Error for FriendlyError {}

// ---------------------------------------------------------------------------
// Config-related errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("Config file not found at {config_path}. Run `devm8 config` to create it.\n  hint: Run `devm8 config` to set up your configuration.")]
pub struct ConfigMissingError {
    pub config_path: String,
}

impl ConfigMissingError {
    pub fn new(config_path: impl Into<String>) -> Self {
        Self {
            config_path: config_path.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// launchctl errors (macOS only)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
#[derive(Debug, Error)]
#[error("launchctl invocation failed\n  hint: {hint}")]
pub struct LaunchctlError {
    pub raw_output: String,
    pub hint: String,
}

#[cfg(target_os = "macos")]
impl LaunchctlError {
    pub fn new(stderr: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            raw_output: stderr.into(),
            hint: hint.into(),
        }
    }
}

/// Derive a human-readable hint from launchctl stderr output.
#[cfg(target_os = "macos")]
pub fn launchctl_hint(stderr: &str) -> String {
    let s = stderr.to_lowercase();
    if s.contains("no such file or directory") {
        return "Make sure you ran `devm8 start` first".to_string();
    }
    if s.contains("operation already in progress") {
        return "Daemon may already be running; check `devm8 status`".to_string();
    }
    if s.contains("permission denied") {
        return "Check file permissions on the plist".to_string();
    }
    "launchctl exited with a non-zero status".to_string()
}

// ---------------------------------------------------------------------------
// Jira errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum JiraError {
    #[error("Jira authentication failed")]
    Auth,

    #[error("Jira permission denied")]
    Permission,

    #[error("Issue {issue_key} not found")]
    NotFound { issue_key: String },

    #[error("Jira rate limit exceeded")]
    RateLimit { retry_after: Option<u64> },

    #[error("Jira server error: {status}")]
    Server { status: u16 },

    #[error("Jira request timed out")]
    Timeout,
}

// ---------------------------------------------------------------------------
// Transition errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("Invalid transition '{attempted}'. Available: {}", available.join(", "))]
pub struct InvalidTransitionError {
    pub attempted: String,
    pub available: Vec<String>,
}

impl InvalidTransitionError {
    pub fn new(attempted: impl Into<String>, available: Vec<String>) -> Self {
        Self {
            attempted: attempted.into(),
            available,
        }
    }
}

// ---------------------------------------------------------------------------
// Claude errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ClaudeError {
    #[error("Claude timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("Claude exited with code {exit_code}")]
    Exit { exit_code: i32, stderr: String },
}

// ---------------------------------------------------------------------------
// Top-level app error (collects all variants for easy use with `?`)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Friendly(#[from] FriendlyError),

    #[error("{0}")]
    ConfigMissing(#[from] ConfigMissingError),

    #[cfg(target_os = "macos")]
    #[error("{0}")]
    Launchctl(#[from] LaunchctlError),

    #[error("{0}")]
    Jira(#[from] JiraError),

    #[error("{0}")]
    InvalidTransition(#[from] InvalidTransitionError),

    #[error("{0}")]
    Claude(#[from] ClaudeError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
