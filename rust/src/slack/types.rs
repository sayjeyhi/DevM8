#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// A single message in a Slack channel or DM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackMessage {
    #[serde(rename = "type", default)]
    pub message_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default)]
    pub text: String,
    pub ts: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_count: Option<u32>,
}

/// A Slack channel or DM conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackChannel {
    pub id: String,
    #[serde(default)]
    pub is_im: bool,
    #[serde(default)]
    pub is_mpim: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A Slack user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackUser {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<SlackUserProfile>,
}

/// Subset of a Slack user's profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackUserProfile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_name: Option<String>,
}

/// Result of `auth.test`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackAuthTestResult {
    pub ok: bool,
    pub user_id: String,
    pub user: String,
    pub team: String,
    pub team_id: String,
}

/// A new incoming message delivered to the poller's handler.
#[derive(Debug, Clone)]
pub struct SlackNewMessage {
    pub channel: SlackChannel,
    pub message: SlackMessage,
    pub sender_name: String,
}
