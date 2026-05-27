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

use super::types::{AskOptions, ClaudeClientConfig, UsageInfo};

const DEFAULT_TIMEOUT_MS: u64 = 300_000;
const PROGRESS_INTERVAL_MS: u64 = 2_000;
const SIGTERM_GRACE_MS: u64 = 2_000;

/// Bind-mount a path (and its canonical symlink target) that lives under /home back into a
/// bwrap sandbox. Necessary because build_bwrap_base overlays /home with an empty tmpfs.
///
/// Strategy: for each path under /home, bind the first dotdir under ~/  (e.g. ~/.local,
/// ~/.nvm) so that the binary AND its runtime deps (node, npm packages) remain accessible.
#[cfg(target_os = "linux")]
fn bind_home_subtree(cmd: &mut Command, path_str: &str) {
    use std::collections::HashSet;
    use std::path::Path;

    let path = Path::new(path_str);
    let real_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    let mut bound: HashSet<String> = HashSet::new();

    for p in [path.to_path_buf(), real_path] {
        if !p.starts_with("/home") {
            continue;
        }
        let mut comps = p.components();
        let _ = comps.next(); // RootDir  "/"
        let _ = comps.next(); // "home"
        let Some(user) = comps.next() else { continue };
        let Some(dotdir) = comps.next() else { continue };

        let user_dir = Path::new("/home").join(user.as_os_str());
        let mount_root = user_dir.join(dotdir.as_os_str());

        if !mount_root.exists() {
            continue;
        }
        let mount_str = mount_root.to_string_lossy().into_owned();
        if !bound.insert(mount_str.clone()) {
            continue;
        }

        // Create parent dir inside sandbox tmpfs, then bind the subtree read-only.
        cmd.args(["--dir", &*user_dir.to_string_lossy()]);
        cmd.args(["--ro-bind", &mount_str, &mount_str]);
    }
}

pub struct ClaudeClient {
    config: ClaudeClientConfig,
    logger: Arc<dyn Logger>,
}

impl ClaudeClient {
    pub fn new(config: ClaudeClientConfig, logger: Arc<dyn Logger>) -> Self {
        Self { config, logger }
    }

    fn build_direct_command(&self, opts: &AskOptions) -> Command {
        let mut cmd = Command::new(&self.config.binary_path);
        cmd.args([
            "--print",
            "--verbose",
            "--dangerously-skip-permissions",
            "--output-format",
            "stream-json",
        ]);
        cmd.env_remove("CLAUDECODE");
        if let Some(ref cwd) = opts.cwd {
            cmd.current_dir(cwd);
        }
        cmd
    }

    /// Shared bwrap namespace setup used by both Claude and shell commands.
    /// Returns a Command ready for appending the binary + its args.
    #[cfg(target_os = "linux")]
    fn build_bwrap_base(&self, cwd: Option<&str>) -> Command {
        let mut cmd = Command::new("bwrap");

        cmd.args(["--ro-bind", "/", "/"]);
        cmd.args(["--proc", "/proc"]);
        cmd.args(["--dev", "/dev"]);
        cmd.args(["--tmpfs", "/tmp"]);
        cmd.args(["--tmpfs", "/home"]);
        cmd.args(["--tmpfs", "/root"]);

        cmd.args(["--dir", "/home/sandbox"]);
        if let Ok(host_home) = std::env::var("HOME") {
            let claude_dir = format!("{host_home}/.claude");
            if std::path::Path::new(&claude_dir).exists() {
                cmd.args(["--ro-bind", &claude_dir, "/home/sandbox/.claude"]);
            }
            let claude_json = format!("{host_home}/.claude.json");
            if std::path::Path::new(&claude_json).exists() {
                cmd.args(["--ro-bind", &claude_json, "/home/sandbox/.claude.json"]);
            }
        }

        let inner_cwd = if let Some(cwd) = cwd {
            cmd.args(["--dir", "/tmp/workspace"]);
            cmd.args(["--bind", cwd, "/tmp/workspace"]);
            "/tmp/workspace"
        } else {
            "/tmp"
        };
        cmd.args(["--chdir", inner_cwd]);

        cmd.arg("--clearenv");
        cmd.args(["--setenv", "HOME", "/home/sandbox"]);
        cmd.args(["--setenv", "TMPDIR", "/tmp"]);
        cmd.args([
            "--setenv",
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        ]);

        let api_key = self
            .config
            .api_key
            .clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());
        if let Some(ref key) = api_key {
            cmd.args(["--setenv", "ANTHROPIC_API_KEY", key]);
        }

        cmd.args([
            "--unshare-pid",
            "--unshare-uts",
            "--unshare-ipc",
            "--die-with-parent",
        ]);

        cmd
    }

    #[cfg(target_os = "linux")]
    fn build_bwrap_command(&self, opts: &AskOptions) -> Command {
        let mut cmd = self.build_bwrap_base(opts.cwd.as_deref());
        // Re-bind the claude binary (and its symlink target) into the sandbox.
        // build_bwrap_base overlays /home with tmpfs, hiding any binary installed there.
        bind_home_subtree(&mut cmd, &self.config.binary_path);
        cmd.arg(&self.config.binary_path);
        cmd.args([
            "--print",
            "--verbose",
            "--dangerously-skip-permissions",
            "--output-format",
            "stream-json",
        ]);

        cmd
    }

    fn build_command(&self, opts: &AskOptions) -> Command {
        #[cfg(target_os = "linux")]
        if self.config.sandbox_enabled {
            return self.build_bwrap_command(opts);
        }
        self.build_direct_command(opts)
    }

    /// Build a sandboxed `sh -c <shell_cmd>` command using the same bwrap namespace as Claude.
    /// On macOS or when sandbox is disabled, falls back to a plain `sh -c`.
    pub fn sandboxed_sh_command(&self, cwd: Option<&str>, shell_cmd: &str) -> Command {
        #[cfg(target_os = "linux")]
        if self.config.sandbox_enabled {
            let mut cmd = self.build_bwrap_base(cwd);
            cmd.args(["sh", "-c", shell_cmd]);
            return cmd;
        }
        let mut cmd = Command::new("sh");
        cmd.args(["-c", shell_cmd]);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        cmd
    }

    pub async fn ask(&self, prompt: &str, opts: AskOptions) -> Result<(String, UsageInfo), AppError> {
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
                "sandbox": self.config.sandbox_enabled,
                "prompt_len": prompt.len(),
            })),
        );

        let mut cmd = self.build_command(&opts);

        if let Some(m) = model {
            cmd.args(["--model", m]);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        #[cfg(unix)]
        {
            extern "C" {
                fn geteuid() -> u32;
            }
            if unsafe { geteuid() } == 0 {
                let msg = "claude refuses --dangerously-skip-permissions as root/sudo; run devm8 as a non-root user";
                self.logger.error(msg, None);
                return Err(AppError::Other(anyhow::anyhow!("{}", msg)));
            }
        }

        let mut child = cmd.spawn().map_err(|e| {
            let binary = &self.config.binary_path;
            let detail = if e.kind() == std::io::ErrorKind::PermissionDenied {
                diagnose_binary(binary)
            } else if e.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "binary not found at '{binary}' — run `devm8 config` to set the correct path"
                )
            } else {
                e.to_string()
            };
            self.logger
                .error(&format!("claude: failed to spawn: {detail}"), None);
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
            Self::stream_output(child, opts.on_progress),
        )
        .await;

        let elapsed_ms = started.elapsed().as_millis() as u64;

        match result {
            Ok(Ok((text, exit_code, stderr_text, usage))) => {
                if exit_code != 0 {
                    let stdout_snippet = &text[..text.len().min(500)];
                    self.logger.error(
                        "claude: process exited with error",
                        Some(&json!({
                            "exit_code": exit_code,
                            "elapsed_ms": elapsed_ms,
                            "stderr": &stderr_text[..stderr_text.len().min(500)],
                            "stdout": stdout_snippet,
                        })),
                    );
                    let detail = if !text.is_empty() { text } else { stderr_text };
                    Err(AppError::Claude(ClaudeError::Exit {
                        exit_code,
                        stderr: detail,
                    }))
                } else {
                    self.logger.info(
                        "claude: completed",
                        Some(&json!({
                            "elapsed_ms": elapsed_ms,
                            "response_len": text.len(),
                        })),
                    );
                    Ok((text, usage))
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
    ) -> Result<(String, i32, String, UsageInfo), AppError> {
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let mut lines_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut text_lines: Vec<String> = Vec::new();
        let mut result_text: Option<String> = None;
        let mut usage = UsageInfo::default();

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
                                Self::handle_event(&event, &mut text_lines, &mut result_text, &mut usage);
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

        let status = child.wait().await.map_err(|e| AppError::Other(e.into()))?;
        let exit_code = status.code().unwrap_or(-1);

        let stderr_lines = stderr_handle.await.unwrap_or_default();

        let final_text = if let Some(r) = result_text {
            r
        } else {
            text_lines.join("\n")
        };

        Ok((final_text, exit_code, stderr_lines.join("\n"), usage))
    }

    fn handle_event(
        event: &Value,
        text_lines: &mut Vec<String>,
        result_text: &mut Option<String>,
        usage: &mut UsageInfo,
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
                usage.cost_usd = event.get("total_cost_usd").and_then(Value::as_f64);
                if let Some(u) = event.get("usage") {
                    usage.input_tokens = u.get("input_tokens").and_then(Value::as_u64);
                    usage.output_tokens = u.get("output_tokens").and_then(Value::as_u64);
                }
            }
            _ => {}
        }
    }
}

/// Build an actionable error message when spawning the claude binary returns EACCES.
fn diagnose_binary(binary: &str) -> String {
    let path = std::path::Path::new(binary);

    // Resolve symlinks so we inspect the real target, not the link itself.
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    if !real.exists() {
        return format!(
            "binary not found at '{binary}' (resolved: '{real}') — run `devm8 config` to set the correct path",
            real = real.display()
        );
    }

    // A symlink pointing to a directory looks executable but execve returns EISDIR → EACCES.
    if real.is_dir() {
        return format!(
            "'{binary}' resolves to a directory ('{real}'), not an executable — \
update claude.binary_path in `devm8 config` to the actual claude binary inside that directory \
(e.g. '{real}/claude')",
            real = real.display()
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&real) {
            if meta.permissions().mode() & 0o111 == 0 {
                return format!(
                    "binary at '{binary}' is not executable — run: chmod +x {real}",
                    real = real.display()
                );
            }
        }

        extern "C" {
            fn geteuid() -> u32;
        }
        let uid = unsafe { geteuid() };
        format!(
            "permission denied executing '{binary}' (uid {uid}, real path '{real}') — \
check: ls -la {real}",
            real = real.display()
        )
    }

    #[cfg(not(unix))]
    format!("permission denied executing '{binary}'")
}
