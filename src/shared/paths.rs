use std::path::PathBuf;

/// All well-known paths used by devm8, derived from the user's home directory.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub restarts_file: PathBuf,
    pub slack_state_file: PathBuf,
    pub logs_dir: PathBuf,
    pub log_file: PathBuf,
    pub audit_log_file: PathBuf,
    pub pid_file: PathBuf,
    /// Platform-specific service unit file (launchd plist on macOS, systemd .service on Linux).
    pub service_file: PathBuf,
    /// Directory that contains the service unit file.
    pub service_dir: PathBuf,
}

impl Paths {
    /// Build a `Paths` instance from the current user's home directory.
    /// Falls back to `/tmp` if the home directory cannot be determined.
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let config_dir = home.join(".config/devm8");
        let logs_dir = config_dir.join("logs");

        #[cfg(target_os = "macos")]
        let service_dir = home.join("Library/LaunchAgents");
        #[cfg(target_os = "macos")]
        let service_file = service_dir.join("net.devm8.plist");

        #[cfg(target_os = "linux")]
        let service_dir = home.join(".config/systemd/user");
        #[cfg(target_os = "linux")]
        let service_file = service_dir.join("devm8.service");

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let service_dir = config_dir.clone();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let service_file = config_dir.join("devm8.service");

        Self {
            config_file: config_dir.join("config.toml"),
            restarts_file: config_dir.join("restarts.json"),
            slack_state_file: config_dir.join("slack-state.json"),
            log_file: logs_dir.join("app.log"),
            audit_log_file: logs_dir.join("audit.log"),
            pid_file: config_dir.join("daemon.pid"),
            service_file,
            service_dir,
            config_dir,
            logs_dir,
        }
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}

/// Global lazy constant — use `PATHS.config_file` etc.
pub static PATHS: std::sync::LazyLock<Paths> = std::sync::LazyLock::new(Paths::new);

/// Expand a leading `~/` or bare `~` to the user's home directory.
pub fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.to_string_lossy().into_owned();
        }
    }
    path.to_string()
}
