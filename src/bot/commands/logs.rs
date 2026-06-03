use std::sync::Arc;

use anyhow::Result;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::bot::AppState;
use crate::channel::ChannelSender;
use crate::shared::PATHS;

const DEFAULT_LINES: usize = 50;
const MAX_LINES: usize = 200;
const CHUNK_SIZE: usize = 3900;

// ---------------------------------------------------------------------------
// Log line formatter
// ---------------------------------------------------------------------------

fn format_log_line(raw: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
        let level = val
            .get("level")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?")
            .to_uppercase();
        let ts = val
            .get("ts")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let msg = val
            .get("msg")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(raw);

        let time = if ts.len() >= 19 { &ts[11..19] } else { ts };

        let meta: Vec<String> = val
            .as_object()
            .map(|obj| {
                obj.iter()
                    .filter(|(k, _)| *k != "level" && *k != "ts" && *k != "msg")
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect()
            })
            .unwrap_or_default();

        if meta.is_empty() {
            format!("{} [{}] {}", time, level, msg)
        } else {
            format!("{} [{}] {} {{{}}}", time, level, msg, meta.join(", "))
        }
    } else {
        raw.to_string()
    }
}

fn format_audit_line(raw: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
        let ts = val
            .get("ts")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let time = if ts.len() >= 19 { &ts[11..19] } else { ts };
        let date = if ts.len() >= 10 { &ts[..10] } else { "" };
        let username = val
            .get("username")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let user_id = val
            .get("user_id")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".into());
        let action = val
            .get("action")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let detail = val
            .get("detail")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if detail.is_empty() {
            format!("{} {} [{}|{}] {}", date, time, username, user_id, action)
        } else {
            format!(
                "{} {} [{}|{}] {} \u{2014} {}",
                date, time, username, user_id, action, detail
            )
        }
    } else {
        raw.to_string()
    }
}

// ---------------------------------------------------------------------------
// Command handler
// ---------------------------------------------------------------------------

pub async fn handle_logs(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    _state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let n: usize = args
        .trim()
        .parse::<usize>()
        .unwrap_or(DEFAULT_LINES)
        .min(MAX_LINES);

    let log_path = &PATHS.log_file;

    let file = match File::open(log_path).await {
        Ok(f) => f,
        Err(_) => {
            sender
                .send(chat_id, "Log file not found or not accessible.")
                .await?;
            return Ok(());
        }
    };

    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut all_lines: Vec<String> = Vec::new();
    while let Ok(Some(line)) = lines.next_line().await {
        all_lines.push(line);
    }

    let tail: Vec<String> = all_lines
        .into_iter()
        .rev()
        .take(n)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|l| format_log_line(&l))
        .collect();

    if tail.is_empty() {
        sender.send(chat_id, "Log file is empty.").await?;
        return Ok(());
    }

    let full_text = tail.join("\n");
    let mut remaining = full_text.as_str();

    while !remaining.is_empty() {
        let chunk_len = remaining.len().min(CHUNK_SIZE);

        let split_at = if chunk_len < remaining.len() {
            remaining[..chunk_len].rfind('\n').unwrap_or(chunk_len)
        } else {
            chunk_len
        };

        let chunk = &remaining[..split_at];
        remaining = remaining[split_at..].trim_start_matches('\n');

        sender
            .send(chat_id, &format!("<pre>{}</pre>", sender.escape(chunk)))
            .await?;
    }

    Ok(())
}

pub async fn handle_audit_logs(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    _state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let n: usize = args
        .trim()
        .parse::<usize>()
        .unwrap_or(DEFAULT_LINES)
        .min(MAX_LINES);

    let log_path = &PATHS.audit_log_file;

    let file = match File::open(log_path).await {
        Ok(f) => f,
        Err(_) => {
            sender
                .send(chat_id, "Audit log file not found or not accessible.")
                .await?;
            return Ok(());
        }
    };

    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut all_lines: Vec<String> = Vec::new();
    while let Ok(Some(line)) = lines.next_line().await {
        all_lines.push(line);
    }

    let tail: Vec<String> = all_lines
        .into_iter()
        .rev()
        .take(n)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|l| format_audit_line(&l))
        .collect();

    if tail.is_empty() {
        sender.send(chat_id, "Audit log is empty.").await?;
        return Ok(());
    }

    let header = format!("\u{1f50d} Last {} audit entries:\n\n", tail.len());
    let full_text = header + &tail.join("\n");
    let mut remaining = full_text.as_str();

    while !remaining.is_empty() {
        let chunk_len = remaining.len().min(CHUNK_SIZE);
        let split_at = if chunk_len < remaining.len() {
            remaining[..chunk_len].rfind('\n').unwrap_or(chunk_len)
        } else {
            chunk_len
        };
        let chunk = &remaining[..split_at];
        remaining = remaining[split_at..].trim_start_matches('\n');
        sender
            .send(chat_id, &format!("<pre>{}</pre>", sender.escape(chunk)))
            .await?;
    }

    Ok(())
}
