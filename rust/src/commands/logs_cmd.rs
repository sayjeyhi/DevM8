use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use chrono::{DateTime, Local};
use serde_json::Value;

use crate::shared::errors::AppError;
use crate::shared::paths::PATHS;

// ---------------------------------------------------------------------------
// TTY detection
// ---------------------------------------------------------------------------

fn is_tty() -> bool {
    // Heuristic: respect NO_COLOR, CLICOLOR=0, TERM=dumb.
    // We don't add a hard atty dependency — fall back to assuming TTY
    // unless one of the opt-out signals is set.
    std::env::var("NO_COLOR").is_err()
        && std::env::var("CLICOLOR").as_deref() != Ok("0")
        && std::env::var("TERM").as_deref() != Ok("dumb")
}

// ---------------------------------------------------------------------------
// ANSI helpers
// ---------------------------------------------------------------------------

fn ansi_wrap(code: &str, s: &str) -> String {
    format!("\x1b[{}m{}\x1b[0m", code, s)
}

fn colored_level(level: &str, color: bool) -> String {
    let upper = level.to_uppercase();
    if !color {
        return upper;
    }
    match level {
        "error" => ansi_wrap("31", &upper),
        "warn"  => ansi_wrap("33", &upper),
        "debug" => ansi_wrap("2",  &upper),
        _       => ansi_wrap("36", &upper), // cyan for info
    }
}

// ---------------------------------------------------------------------------
// Line formatting
// ---------------------------------------------------------------------------

fn parse_time_part(ts: &str) -> String {
    ts.parse::<DateTime<Local>>()
        .map(|dt| format!("{}", dt.format("%H:%M:%S")))
        .unwrap_or_else(|_| ts.to_string())
}

fn build_meta_str(v: &Value) -> String {
    let skip = ["level", "ts", "msg"];
    if let Some(obj) = v.as_object() {
        let parts: Vec<String> = obj
            .iter()
            .filter(|(k, _)| !skip.contains(&k.as_str()))
            .map(|(k, val)| {
                let s = match val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                format!("{k}={s}")
            })
            .collect();
        parts.join(" ")
    } else {
        String::new()
    }
}

/// Parse a JSON log line and format it for human display.
fn format_line(raw: &str, color: bool) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let v = match serde_json::from_str::<Value>(trimmed) {
        Ok(v) => v,
        Err(_) => return trimmed.to_string(),
    };

    let level = v.get("level").and_then(|l| l.as_str()).unwrap_or("info");
    let msg   = v.get("msg").and_then(|m| m.as_str()).unwrap_or(trimmed);
    let ts    = v.get("ts").and_then(|t| t.as_str()).unwrap_or("");

    let time_part    = parse_time_part(ts);
    let level_str    = colored_level(level, color);
    let meta         = build_meta_str(&v);

    if meta.is_empty() {
        format!("{} [{}] {}", time_part, level_str, msg)
    } else {
        let meta_display = if color { ansi_wrap("2", &meta) } else { meta };
        format!("{} [{}] {}  {}", time_part, level_str, msg, meta_display)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_last_n_lines(path: &Path, n: usize) -> std::io::Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
    let start = lines.len().saturating_sub(n);
    Ok(lines[start..].to_vec())
}

// ---------------------------------------------------------------------------
// Public command entry point
// ---------------------------------------------------------------------------

pub async fn logs_command(tail: u32, follow: bool) -> Result<(), AppError> {
    let log_file = &PATHS.log_file;
    let color = is_tty();

    // Print last `tail` lines (if file exists).
    if log_file.exists() {
        let lines = read_last_n_lines(log_file, tail as usize).map_err(AppError::Io)?;
        for line in &lines {
            let formatted = format_line(line, color);
            if !formatted.is_empty() {
                println!("{}", formatted);
            }
        }
    }

    if !follow {
        return Ok(());
    }

    // ------------------------------------------------------------------
    // Follow mode: poll for new content every 300ms.
    // ------------------------------------------------------------------
    let mut file_size: u64 = log_file
        .exists()
        .then(|| std::fs::metadata(log_file).map(|m| m.len()).unwrap_or(0))
        .unwrap_or(0);

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        if !log_file.exists() {
            continue;
        }

        let new_size = std::fs::metadata(log_file)
            .map(|m| m.len())
            .unwrap_or(0);

        if new_size < file_size {
            // File was rotated — reset position.
            file_size = 0;
        }

        if new_size > file_size {
            let mut f = std::fs::File::open(log_file).map_err(AppError::Io)?;
            f.seek(SeekFrom::Start(file_size)).map_err(AppError::Io)?;
            let reader = BufReader::new(f);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        let formatted = format_line(&l, color);
                        if !formatted.is_empty() {
                            println!("{}", formatted);
                        }
                    }
                    Err(_) => break,
                }
            }
            file_size = new_size;
        }
    }
}
