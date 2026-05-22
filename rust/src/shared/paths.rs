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
    pub pid_file: PathBuf,
    pub plist_file: PathBuf,
    pub launch_agents_dir: PathBuf,
}

impl Paths {
    /// Build a `Paths` instance from the current user's home directory.
    /// Falls back to `/tmp` if the home directory cannot be determined.
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let config_dir = home.join(".config/devm8");
        let logs_dir = config_dir.join("logs");
        let launch_agents_dir = home.join("Library/LaunchAgents");

        Self {
            config_file: config_dir.join("config.toml"),
            restarts_file: config_dir.join("restarts.json"),
            slack_state_file: config_dir.join("slack-state.json"),
            log_file: logs_dir.join("app.log"),
            pid_file: config_dir.join("daemon.pid"),
            plist_file: launch_agents_dir.join("net.devm8.plist"),
            config_dir,
            logs_dir,
            launch_agents_dir,
        }
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}

/// Global lazy constant — use `PATHS.config_file` etc.
///
/// Because `std::sync::OnceLock` is stable since Rust 1.70 we use it here.
/// The TypeScript `PATHS` object was a module-level constant; this gives the
/// same ergonomics.
pub static PATHS: std::sync::LazyLock<Paths> = std::sync::LazyLock::new(Paths::new);
