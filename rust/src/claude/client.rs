#![allow(dead_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::process::Command;
use tokio::time::{interval, timeout};

use crate::logger::Logger;
use crate::shared::errors::{AppError, ClaudeError};

use super::types::{AskOptions, ClaudeClientConfig};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const PROGRESS_INTERVAL_MS: u64 = 2_000;
const SIGTERM_GRACE_MS: u64 = 2_000;

pub struct ClaudeClient {
    config: ClaudeClientConfig,
    logger: Arc<dyn Logger>,
}

impl ClaudeClient {
    pub fn new(config: ClaudeClientConfig, logger: Arc<dyn Logger>) -> Self {
        Self { config, logger }
    }

    pub async fn ask(&self, prompt: &str, opts: AskOptions) -> Result<String, AppError> {
        let timeout_ms = opts
            .timeout_ms
            .or(self.config.timeout_ms)
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        let model = opts.model.as_deref().or(self.config.model.as_deref());

        self.logger.info(
            "claude: invoking",
            Some(&json!({
                "model": model.unwrap_or("default"),
                "cwd": opts.cwd.as_deref().unwrap_or("(none)"),
                "timeout_ms": timeout_ms,
                "prompt_len": prompt.len(),
            })),
        );

        let mut cmd = Command::new(&self.config.binary_path);
        cmd.args([
            "--print",
            "--verbose",
            "--dangerously-skip-permissions",
            "--output-format",
            "stream-json",
        ]);

        if let Some(m) = model {
            cmd.args(["--model", m]);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        cmd.env_remove("CLAUDECODE");

        if let Some(ref cwd) = opts.cwd {
            cmd.current_dir(cwd);
        }

        let mut child = cmd.spawn().map_err(|e| {
            self.logger
                .error(&format!("claude: failed to spawn process: {e}"), None);
            AppError::Other(anyhow::anyhow!("failed to spawn claude: {}", e))
        })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .map_err(|e| AppError::Other(e.into()))?;
        }

        let started = Instant::now();

        let result = timeout(
            Duration::from_millis(timeout_ms),
            Self::stream_output(child, opts.on_progress),
        )
        .await;

        let elapsed_ms = started.elapsed().as_millis() as u64;

        match result {
            Ok(Ok((text, exit_code, stderr_text))) => {
                if exit_code != 0 {
                    self.logger.error(
                        "claude: process exited with error",
                        Some(&json!({
                            "exit_code": exit_code,
                            "elapsed_ms": elapsed_ms,
                            "stderr": &stderr_text[..stderr_text.len().min(500)],
                        })),
                    );
                    Err(AppError::Claude(ClaudeError::Exit {
                        exit_code,
                        stderr: stderr_text,
                    }))
                } else {
                    self.logger.info(
                        "claude: completed",
                        Some(&json!({
                            "elapsed_ms": elapsed_ms,
                            "response_len": text.len(),
                        })),
                    );
                    Ok(text)
                }
            }
            Ok(Err(e)) => {
                self.logger.error(
                    &format!("claude: stream error: {e}"),
                    Some(&json!({ "elapsed_ms": elapsed_ms })),
                );
                Err(e)
            }
            Err(_elapsed) => {
                self.logger.error(
                    "claude: timed out",
                    Some(&json!({ "timeout_ms": timeout_ms })),
                );
                Err(AppError::Claude(ClaudeError::Timeout { timeout_ms }))
            }
        }
    }

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

        let mut progress_ticker = interval(Duration::from_millis(PROGRESS_INTERVAL_MS));
        progress_ticker.tick().await;

        let stderr_handle = tokio::spawn(async move {
            let mut collected = Vec::<String>::new();
            while let Ok(Some(line)) = stderr_reader.next_line().await {
                collected.push(line);
            }
            collected
        });

        loop {
            tokio::select! {
                line_result = lines_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            if let Ok(event) = serde_json::from_str::<Value>(&line) {
                                Self::handle_event(&event, &mut text_lines, &mut result_text);
                            }
                        }
                        Ok(None) => break,
                        Err(e) => return Err(AppError::Other(e.into())),
                    }
                }
                _ = progress_ticker.tick() => {
                    if let Some(ref cb) = on_progress {
                        cb(text_lines.clone()).await;
                    }
                }
            }
        }

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
                if let Some(text) = event
                    .get("delta")
                    .and_then(|d| d.get("text"))
                    .and_then(Value::as_str)
                {
                    text_lines.push(text.to_string());
                }
            }
            "assistant" => {
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
                if let Some(r) = event.get("result").and_then(Value::as_str) {
                    *result_text = Some(r.to_string());
                }
            }
            _ => {}
        }
    }
}
