use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::Utc;

/// Tracks recent restart timestamps and enforces a sliding-window rate limit.
///
/// State is persisted in a JSON file as a list of Unix-millisecond timestamps.
pub struct RestartTracker {
    file_path: PathBuf,
    max_restarts: usize,
    window_ms: u64,
}

impl RestartTracker {
    /// Create a new tracker.
    ///
    /// - `file_path`    — where timestamps are persisted
    /// - `max_restarts` — maximum number of restarts allowed in `window_ms`
    /// - `window_ms`    — sliding window duration in milliseconds
    pub fn new(
        file_path: impl Into<PathBuf>,
        max_restarts: usize,
        window_ms: u64,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            max_restarts,
            window_ms,
        }
    }

    /// Record a restart.
    ///
    /// Adds the current timestamp, prunes entries older than the window, then
    /// writes back.  Returns `true` when the limit has been reached (i.e. the
    /// number of recent restarts is >= `max_restarts`).
    pub async fn record_restart(&self) -> anyhow::Result<bool> {
        let mut timestamps = self.read().await?;

        let now_ms = Utc::now().timestamp_millis() as u64;
        timestamps.push(now_ms);

        // Prune timestamps outside the window.
        timestamps.retain(|&ts| now_ms.saturating_sub(ts) <= self.window_ms);

        self.write(&timestamps).await?;

        Ok(timestamps.len() >= self.max_restarts)
    }

    /// Reset the tracker (write an empty list).
    #[allow(dead_code)]
    pub async fn reset(&self) -> anyhow::Result<()> {
        self.write(&[]).await
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    async fn read(&self) -> anyhow::Result<Vec<u64>> {
        match tokio::fs::read_to_string(&self.file_path).await {
            Ok(content) => {
                let v: Vec<u64> = serde_json::from_str(&content)
                    .unwrap_or_default();
                Ok(v)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).context(format!(
                "Failed to read restart tracker file: {}",
                self.file_path.display()
            )),
        }
    }

    async fn write(&self, timestamps: &[u64]) -> anyhow::Result<()> {
        if let Some(dir) = self.file_path.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .context("Failed to create restart tracker directory")?;
        }

        let json = serde_json::to_string(timestamps)
            .context("Failed to serialize restart timestamps")?;

        let tmp = self.file_path.with_extension("json.tmp");
        tokio::fs::write(&tmp, json.as_bytes())
            .await
            .context("Failed to write restart tracker tmp file")?;
        tokio::fs::rename(&tmp, &self.file_path)
            .await
            .context("Failed to rename restart tracker tmp file")?;

        Ok(())
    }
}

/// Convenience constructor that reads the path from `PATHS`.
impl Default for RestartTracker {
    fn default() -> Self {
        Self::new(
            crate::shared::paths::PATHS.restarts_file.clone(),
            10,
            60_000,
        )
    }
}

// ---------------------------------------------------------------------------
// Helper to construct from a plain &Path (used in commands)
// ---------------------------------------------------------------------------

impl RestartTracker {
    #[allow(dead_code)]
    pub fn from_path(path: impl AsRef<Path>) -> Self {
        Self::new(path.as_ref(), 10, 60_000)
    }
}
