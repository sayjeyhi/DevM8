use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::claude::client::AiClient;
use crate::claude::types::{AskOptions, UsageInfo};
use crate::logger::Logger;
use crate::shared::errors::AppError;

use super::types::KiroClientConfig;

const DEFAULT_TIMEOUT_MS: u64 = 300_000;

pub struct KiroClient {
    config: KiroClientConfig,
    logger: Arc<dyn Logger>,
}

impl KiroClient {
    pub fn new(config: KiroClientConfig, logger: Arc<dyn Logger>) -> Self {
        Self { config, logger }
    }

    fn build_command(&self, opts: &AskOptions) -> Command {
        let mut cmd = Command::new(&self.config.binary_path);
        cmd.args(["--print", "--dangerously-skip-permissions"]);
        if let Some(ref cwd) = opts.cwd {
            cmd.current_dir(cwd);
        }
        cmd
    }
}

#[async_trait]
impl AiClient for KiroClient {
    async fn ask(&self, prompt: &str, opts: AskOptions) -> Result<(String, UsageInfo), AppError> {
        let timeout_ms = opts
            .timeout_ms
            .or(self.config.timeout_ms)
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        self.logger.info(
            "kiro: invoking",
            Some(&json!({
                "cwd": opts.cwd.as_deref().unwrap_or("(none)"),
                "timeout_ms": timeout_ms,
                "prompt_len": prompt.len(),
            })),
        );

        let mut cmd = self.build_command(&opts);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            let binary = &self.config.binary_path;
            let detail = if e.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "binary not found at '{binary}' — run `devm8 config` to set the correct path"
                )
            } else {
                e.to_string()
            };
            self.logger
                .error(&format!("kiro: failed to spawn: {detail}"), None);
            AppError::Other(anyhow::anyhow!("{}", detail))
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
            collect_output(child, opts.on_progress),
        )
        .await;

        let elapsed_ms = started.elapsed().as_millis() as u64;

        match result {
            Ok(Ok((text, exit_code, stderr_text))) => {
                if exit_code != 0 {
                    self.logger.error(
                        "kiro: process exited with error",
                        Some(&json!({
                            "exit_code": exit_code,
                            "elapsed_ms": elapsed_ms,
                            "stderr": &stderr_text[..stderr_text.len().min(500)],
                        })),
                    );
                    let detail = if !text.is_empty() { text } else { stderr_text };
                    Err(AppError::Other(anyhow::anyhow!(
                        "kiro exited with code {}: {}",
                        exit_code,
                        detail
                    )))
                } else {
                    self.logger.info(
                        "kiro: completed",
                        Some(&json!({
                            "elapsed_ms": elapsed_ms,
                            "response_len": text.len(),
                        })),
                    );
                    Ok((text, UsageInfo::default()))
                }
            }
            Ok(Err(e)) => {
                self.logger.error(&format!("kiro: stream error: {e}"), None);
                Err(e)
            }
            Err(_elapsed) => {
                self.logger.error(
                    "kiro: timed out",
                    Some(&json!({ "timeout_ms": timeout_ms })),
                );
                Err(AppError::Other(anyhow::anyhow!(
                    "kiro timed out after {}ms",
                    timeout_ms
                )))
            }
        }
    }
}

async fn collect_output(
    mut child: tokio::process::Child,
    on_progress: Option<crate::claude::types::ProgressCallback>,
) -> Result<(String, i32, String), AppError> {
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let mut lines_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let mut text_lines: Vec<String> = Vec::new();

    let mut progress_ticker = tokio::time::interval(Duration::from_millis(2_000));
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
                        text_lines.push(line);
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

    let status = child.wait().await.map_err(|e| AppError::Other(e.into()))?;
    let exit_code = status.code().unwrap_or(-1);
    let stderr_lines = stderr_handle.await.unwrap_or_default();

    Ok((text_lines.join("\n"), exit_code, stderr_lines.join("\n")))
}
