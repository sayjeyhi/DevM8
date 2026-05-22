#![allow(dead_code)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::shared::paths::PATHS;

/// Persisted poller state: last-seen timestamps per channel and per thread.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlackState {
    /// Maps channel_id → last seen message `ts`.
    #[serde(default)]
    pub last_ts: HashMap<String, String>,

    /// Maps channel_id → (thread_ts → last seen reply `ts`).
    #[serde(default)]
    pub thread_ts: HashMap<String, HashMap<String, String>>,
}

/// Load the Slack poller state from disk.
/// Returns a default (empty) state if the file does not exist or is corrupt.
pub async fn load_slack_state() -> anyhow::Result<SlackState> {
    let path = &PATHS.slack_state_file;
    match fs::read_to_string(path).await {
        Ok(contents) => {
            let state: SlackState = serde_json::from_str(&contents).unwrap_or_default();
            Ok(state)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SlackState::default()),
        Err(e) => Err(e.into()),
    }
}

/// Persist the Slack poller state to disk.
pub async fn save_slack_state(state: &SlackState) -> anyhow::Result<()> {
    let path = &PATHS.slack_state_file;
    // Ensure the parent directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(state)?;
    fs::write(path, json).await?;
    Ok(())
}
