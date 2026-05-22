#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Configuration for the Jira client (mirrors JiraConfig in the TypeScript source).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraClientConfig {
    pub host: String,
    pub email: String,
    pub api_token: String,
    pub project_keys: Vec<String>,
    /// Default issue type to use when creating issues (e.g. "Task").
    pub issue_type: Option<String>,
    /// HTTP request timeout in milliseconds (default: 10_000).
    pub request_timeout_ms: Option<u64>,
}

/// A minimal Jira issue representation returned by client methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub status: String,
    pub description: String,
    pub url: String,
}
