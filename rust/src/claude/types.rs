#![allow(dead_code)]

use std::pin::Pin;

/// Configuration for the Claude CLI client.
#[derive(Debug, Clone)]
pub struct ClaudeClientConfig {
    /// Path to the `claude` CLI binary.
    pub binary_path: String,
    /// Default timeout for requests, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Default model to pass via `--model`.
    pub model: Option<String>,
}

/// Per-request options that override the client defaults.
pub type ProgressCallback = Box<
    dyn Fn(Vec<String>) -> Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

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
            .field("on_progress", &self.on_progress.as_ref().map(|_| "<callback>"))
            .field("cwd", &self.cwd)
            .finish()
    }
}

impl Default for AskOptions {
    fn default() -> Self {
        Self {
            timeout_ms: None,
            model: None,
            on_progress: None,
            cwd: None,
        }
    }
}
