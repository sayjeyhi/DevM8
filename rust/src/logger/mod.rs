pub mod rotate;

use std::fs;
use std::io::Write as IoWrite;
use std::path::Path;
use chrono::Utc;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Log level
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug = 0,
    Info  = 1,
    Warn  = 2,
    Error = 3,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info  => "info",
            Level::Warn  => "warn",
            Level::Error => "error",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info  => "INFO ",
            Level::Warn  => "WARN ",
            Level::Error => "ERROR",
        }
    }
}

impl std::str::FromStr for Level {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "debug" => Ok(Level::Debug),
            "info"  => Ok(Level::Info),
            "warn"  => Ok(Level::Warn),
            "error" => Ok(Level::Error),
            other   => Err(format!("unknown log level: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Output mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Structured JSON, one object per line.
    Json,
    /// Human-readable TTY output.
    Tty,
}

// ---------------------------------------------------------------------------
// Logger trait
// ---------------------------------------------------------------------------

pub trait Logger: Send + Sync {
    fn info(&self, msg: &str, meta: Option<&Value>);
    fn error(&self, msg: &str, meta: Option<&Value>);
    fn warn(&self, msg: &str, meta: Option<&Value>);
    fn debug(&self, msg: &str, meta: Option<&Value>);
}

// ---------------------------------------------------------------------------
// FileLogger — writes to stdout (JSON or TTY) and optionally to a log file
// ---------------------------------------------------------------------------

pub struct FileLogger {
    min_level: Level,
    mode: OutputMode,
    log_file_path: Option<std::path::PathBuf>,
}

impl FileLogger {
    fn emit(&self, level: Level, msg: &str, meta: Option<&Value>) {
        if level < self.min_level {
            return;
        }

        let ts = Utc::now().to_rfc3339();

        // Build the JSON line (used for file output and json-mode stdout)
        let json_line = if let Some(m) = meta {
            // Merge meta fields into the top-level object
            let mut obj = json!({ "level": level.as_str(), "ts": ts, "msg": msg });
            if let Some(map) = m.as_object() {
                for (k, v) in map {
                    obj[k] = v.clone();
                }
            }
            obj.to_string()
        } else {
            json!({ "level": level.as_str(), "ts": ts, "msg": msg }).to_string()
        };

        // Write to log file if configured
        if let Some(path) = &self.log_file_path {
            append_line(path, &json_line);
        }

        // Write to stdout
        match self.mode {
            OutputMode::Json => {
                let _ = std::io::stdout().lock().write_all((json_line.clone() + "\n").as_bytes());
            }
            OutputMode::Tty => {
                let meta_part = meta
                    .filter(|m| m.as_object().map(|o| !o.is_empty()).unwrap_or(false))
                    .map(|m| format!("  {m}"))
                    .unwrap_or_default();
                let _ = std::io::stdout()
                    .lock()
                    .write_all(format!("[{}] {}{}\n", level.label(), msg, meta_part).as_bytes());
            }
        }
    }
}

impl Logger for FileLogger {
    fn info(&self, msg: &str, meta: Option<&Value>) { self.emit(Level::Info, msg, meta); }
    fn error(&self, msg: &str, meta: Option<&Value>) { self.emit(Level::Error, msg, meta); }
    fn warn(&self, msg: &str, meta: Option<&Value>) { self.emit(Level::Warn, msg, meta); }
    fn debug(&self, msg: &str, meta: Option<&Value>) { self.emit(Level::Debug, msg, meta); }
}

// ---------------------------------------------------------------------------
// Constructor
// ---------------------------------------------------------------------------

/// Create a logger.
///
/// - `level`         — minimum level to emit (e.g. `Level::Info`)
/// - `mode`          — `Some(OutputMode::Json)` / `Some(OutputMode::Tty)` or
///                     `None` to auto-detect from whether stdout is a TTY.
/// - `log_file_path` — optional path; the directory is created if needed.
pub fn create_logger(
    level: Level,
    mode: Option<OutputMode>,
    log_file_path: Option<impl AsRef<Path>>,
) -> FileLogger {
    let effective_mode = mode.unwrap_or_else(|| {
        // Heuristic: if NO_COLOR or non-TTY env is set, fall back to JSON.
        if std::env::var("NO_COLOR").is_ok()
            || std::env::var("CLICOLOR").as_deref() == Ok("0")
            || std::env::var("TERM").as_deref() == Ok("dumb")
        {
            OutputMode::Json
        } else {
            // We can't easily detect a real TTY in a portable way without
            // an extra crate; default to Tty and let callers override.
            OutputMode::Tty
        }
    });

    let log_file_path = log_file_path.map(|p| {
        let path = p.as_ref().to_path_buf();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        path
    });

    FileLogger { min_level: level, mode: effective_mode, log_file_path }
}

// ---------------------------------------------------------------------------
// Standalone append helper (mirrors TypeScript appendToLogFile)
// ---------------------------------------------------------------------------

/// Append a single JSON log line to a file, creating parent dirs as needed.
/// Errors are silently ignored (matches TypeScript behaviour).
pub fn append_to_log_file(log_file_path: impl AsRef<Path>, level: Level, msg: &str, meta: Option<&Value>) {
    let path = log_file_path.as_ref();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }

    let ts = Utc::now().to_rfc3339();
    let json_line = if let Some(m) = meta {
        let mut obj = json!({ "level": level.as_str(), "ts": ts, "msg": msg });
        if let Some(map) = m.as_object() {
            for (k, v) in map {
                obj[k] = v.clone();
            }
        }
        obj.to_string()
    } else {
        json!({ "level": level.as_str(), "ts": ts, "msg": msg }).to_string()
    };

    append_line(path, &json_line);
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

fn append_line(path: &Path, line: &str) {
    use std::fs::OpenOptions;
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all((line.to_string() + "\n").as_bytes());
    }
}
