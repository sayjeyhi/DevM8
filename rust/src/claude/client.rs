#![allow(dead_code)]

use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::process::Child;
use tokio::time::{interval, timeout};

use crate::shared::errors::{AppError, ClaudeError};

use super::types::{AskOptions, ClaudeClientConfig};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const PROGRESS_INTERVAL_MS: u64 = 2_000;
const SIGTERM_GRACE_MS: u64 = 2_000;

pub struct ClaudeClient {
    config: ClaudeClientConfig,
}

impl ClaudeClient {
    pub fn new(config: ClaudeClientConfig) -> Self {
        Self { config }
    }

    /// Ask the Claude CLI a question and return the full response text.
    ///
    /// The subprocess is spawned with `--print --verbose --dangerously-skip-permissions
    /// --output-format stream-json` (plus an optional `--model`).  The prompt is written
    /// to stdin; stdout is parsed as newline-delimited JSON events.
    pub async fn ask(&self, prompt: &str, opts: AskOptions) -> Result<String, AppError> {
        let timeout_ms = opts
            .timeout_ms
            .or(self.config.timeout_ms)
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        let model = opts.model.as_deref().or(self.config.model.as_deref());

        // ----------------------------------------------------------------
        // Build the command
        // ----------------------------------------------------------------
        let mut cmd = Command::new(&self.config.binary_path);
        cmd.args(["--print", "--verbose", "--dangerously-skip-permissions",
                  "--output-format", "stream-json"]);

        if let Some(m) = model {
            cmd.args(["--model", m]);
        }

        // Pipe stdin/stdout/stderr
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Remove CLAUDECODE env var so we don't accidentally nest sessions.
        cmd.env_remove("CLAUDECODE");

        if let Some(ref cwd) = opts.cwd {
            cmd.current_dir(cwd);
        }

        // ----------------------------------------------------------------
        // Spawn
        // ----------------------------------------------------------------
        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::Other(anyhow::anyhow!("failed to spawn claude: {}", e)))?;

        // Write prompt to stdin then close it.
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .map_err(|e| AppError::Other(e.into()))?;
            // stdin is dropped here, closing the pipe.
        }

        // ----------------------------------------------------------------
        // Stream stdout + run progress timer, all within the global timeout
        // ----------------------------------------------------------------
        let result = timeout(
            Duration::from_millis(timeout_ms),
            Self::stream_output(child, opts.on_progress),
        )
        .await;

        match result {
            Ok(Ok((text, exit_code, stderr_text))) => {
                if exit_code != 0 {
                    Err(AppError::Claude(ClaudeError::Exit {
                        exit_code,
                        stderr: stderr_text,
                    }))
                } else {
                    Ok(text)
                }
            }
            Ok(Err(e)) => Err(e),
            Err(_elapsed) => {
                // timeout — process was already killed inside stream_output if
                // we returned early, but timeout wraps the future so the child
                // may still be running; nothing we can do without the handle.
                Err(AppError::Claude(ClaudeError::Timeout { timeout_ms }))
            }
        }
    }

    // -----------------------------------------------------------------------
    // Internal: stream stdout lines, parse events, drive progress callback.
    // -----------------------------------------------------------------------

    async fn stream_output(
        mut child: Child,
        on_progress: Option<super::types::ProgressCallback>,
    ) -> Result<(String, i32, String), AppError> {
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let mut lines_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut text_lines: Vec<String> = Vec::new();
        let mut result_text: Option<String> = None;

        // Progress ticker
        let mut progress_ticker = interval(Duration::from_millis(PROGRESS_INTERVAL_MS));
        progress_ticker.tick().await; // consume the immediate first tick

        // Drain stderr concurrently using a separate task.
        let stderr_handle = tokio::spawn(async move {
            let mut collected = Vec::<String>::new();
            while let Ok(Some(line)) = stderr_reader.next_line().await {
                collected.push(line);
            }
            collected
        });

        loop {
            tokio::select! {
                // Next stdout line
                line_result = lines_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            if let Ok(event) = serde_json::from_str::<Value>(&line) {
                                Self::handle_event(&event, &mut text_lines, &mut result_text);
                            }
                        }
                        Ok(None) => {
                            // EOF — done reading stdout
                            break;
                        }
                        Err(e) => {
                            return Err(AppError::Other(e.into()));
                        }
                    }
                }

                // Progress callback tick
                _ = progress_ticker.tick() => {
                    if let Some(ref cb) = on_progress {
                        cb(text_lines.clone()).await;
                    }
                }
            }
        }

        // Wait for process to exit and collect exit code.
        let status = child
            .wait()
            .await
            .map_err(|e| AppError::Other(e.into()))?;
        let exit_code = status.code().unwrap_or(-1);

        let stderr_lines = stderr_handle.await.unwrap_or_default();

        let final_text = if let Some(r) = result_text {
            r
        } else {
            text_lines.join("\n")
        };

        Ok((final_text, exit_code, stderr_lines.join("\n")))
    }

    fn handle_event(
        event: &Value,
        text_lines: &mut Vec<String>,
        result_text: &mut Option<String>,
    ) {
        let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");

        match event_type {
            "content_block_delta" => {
                // {type: 'content_block_delta', delta: {type: 'text_delta', text: string}}
                if let Some(text) = event
                    .get("delta")
                    .and_then(|d| d.get("text"))
                    .and_then(Value::as_str)
                {
                    text_lines.push(text.to_string());
                }
            }
            "assistant" => {
                // {type: 'assistant', message: {content: [{type: 'text', text: string}]}}
                if let Some(content) = event
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(Value::as_array)
                {
                    for block in content {
                        if block.get("type").and_then(Value::as_str) == Some("text") {
                            if let Some(t) = block.get("text").and_then(Value::as_str) {
                                text_lines.push(t.to_string());
                            }
                        }
                    }
                }
            }
            "result" => {
                // {type: 'result', is_error: bool, result: string}
                if let Some(r) = event.get("result").and_then(Value::as_str) {
                    *result_text = Some(r.to_string());
                }
            }
            _ => {}
        }
    }
}
