use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::bot::utils::escape_html;
use crate::bot::AppState;
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

        // Extract time portion HH:MM:SS from ISO timestamp
        let time = if ts.len() >= 19 { &ts[11..19] } else { ts };

        // Collect remaining fields as meta
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

// ---------------------------------------------------------------------------
// Command handler
// ---------------------------------------------------------------------------

pub async fn handle_logs(
    bot: Bot,
    chat_id: ChatId,
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
            bot.send_message(chat_id, "Log file not found or not accessible.")
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
        bot.send_message(chat_id, "Log file is empty.").await?;
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

        bot.send_message(chat_id, format!("<pre>{}</pre>", escape_html(chunk)))
            .parse_mode(ParseMode::Html)
            .await?;
    }

    Ok(())
}
