#![allow(dead_code)]

use std::pin::Pin;

/// Token usage and cost from a Claude response.
#[derive(Debug, Clone, Default)]
pub struct UsageInfo {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

impl UsageInfo {
    pub fn is_empty(&self) -> bool {
        self.input_tokens.is_none() && self.output_tokens.is_none() && self.cost_usd.is_none()
    }

    pub fn format_footer(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut parts: Vec<String> = Vec::new();
        match (self.input_tokens, self.output_tokens) {
            (Some(i), Some(o)) => parts.push(format!("{i} in / {o} out tokens")),
            (Some(i), None) => parts.push(format!("{i} in tokens")),
            (None, Some(o)) => parts.push(format!("{o} out tokens")),
            (None, None) => {}
        }
        if let Some(c) = self.cost_usd {
            parts.push(format!("${c:.4}"));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" · "))
        }
    }
}

/// Configuration for the Claude CLI client.
#[derive(Debug, Clone)]
pub struct ClaudeClientConfig {
    /// Path to the `claude` CLI binary.
    pub binary_path: String,
    /// Default timeout for requests, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Default model to pass via `--model`.
    pub model: Option<String>,
    /// Anthropic API key — passed into the sandbox environment when sandbox is active.
    pub api_key: Option<String>,
    /// Isolate Claude subprocess with bubblewrap (effective on Linux only).
    pub sandbox_enabled: bool,
}

/// Per-request options that override the client defaults.
pub type ProgressCallback =
    Box<dyn Fn(Vec<String>) -> Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>;

#[derive(Default)]
pub struct AskOptions {
    /// Override the client-level timeout for this call.
    pub timeout_ms: Option<u64>,
    /// Override the client-level model for this call.
    pub model: Option<String>,
    /// Invoked roughly every 2 seconds with the accumulated text lines so far.
    pub on_progress: Option<ProgressCallback>,
    /// Working directory for the subprocess.
    pub cwd: Option<String>,
}

impl std::fmt::Debug for AskOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AskOptions")
            .field("timeout_ms", &self.timeout_ms)
            .field("model", &self.model)
            .field(
                "on_progress",
                &self.on_progress.as_ref().map(|_| "<callback>"),
            )
            .field("cwd", &self.cwd)
            .finish()
    }
}
