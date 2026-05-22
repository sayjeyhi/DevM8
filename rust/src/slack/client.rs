#![allow(dead_code)]

use anyhow::Result;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Client,
};
use serde_json::{json, Value};

use super::types::{SlackAuthTestResult, SlackChannel, SlackMessage, SlackUser};

const BASE_URL: &str = "https://slack.com/api";

/// Error type for Slack rate-limiting (429 responses).
#[derive(Debug, thiserror::Error)]
#[error("Slack rate limit: retry after {retry_after_seconds}s")]
pub struct SlackRateLimitError {
    pub retry_after_seconds: u64,
}

pub struct SlackClient {
    token: String,
    http: Client,
}

impl SlackClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            http: Client::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn auth_header(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let value = format!("Bearer {}", self.token);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&value).expect("token is ASCII"),
        );
        headers
    }

    /// GET a Slack API method with query parameters.
    async fn get(&self, method: &str, params: &[(&str, &str)]) -> Result<Value> {
        let url = format!("{}/{}", BASE_URL, method);
        let resp = self
            .http
            .get(&url)
            .headers(self.auth_header())
            .query(params)
            .send()
            .await?;

        if resp.status().as_u16() == 429 {
            let retry = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            return Err(SlackRateLimitError { retry_after_seconds: retry }.into());
        }

        let val: Value = resp.json().await?;
        Self::check_ok(&val)?;
        Ok(val)
    }

    /// POST a Slack API method with `application/x-www-form-urlencoded` body.
    async fn post(&self, method: &str, params: &[(&str, &str)]) -> Result<Value> {
        let url = format!("{}/{}", BASE_URL, method);
        let mut headers = self.auth_header();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );

        // Encode the params as a form body.
        let form_body: String = params
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    urlencoding_simple(k),
                    urlencoding_simple(v)
                )
            })
            .collect::<Vec<_>>()
            .join("&");

        let resp = self
            .http
            .post(&url)
            .headers(headers)
            .body(form_body)
            .send()
            .await?;

        if resp.status().as_u16() == 429 {
            let retry = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            return Err(SlackRateLimitError { retry_after_seconds: retry }.into());
        }

        let val: Value = resp.json().await?;
        Self::check_ok(&val)?;
        Ok(val)
    }

    /// Also support JSON POST for methods like chat.postMessage.
    async fn post_json(&self, method: &str, body: &Value) -> Result<Value> {
        let url = format!("{}/{}", BASE_URL, method);
        let resp = self
            .http
            .post(&url)
            .headers(self.auth_header())
            .json(body)
            .send()
            .await?;

        if resp.status().as_u16() == 429 {
            let retry = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            return Err(SlackRateLimitError { retry_after_seconds: retry }.into());
        }

        let val: Value = resp.json().await?;
        Self::check_ok(&val)?;
        Ok(val)
    }

    fn check_ok(val: &Value) -> Result<()> {
        if val.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return Ok(());
        }
        let err = val
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown_error");
        anyhow::bail!("Slack API error: {}", err)
    }

    fn parse_messages(val: &Value) -> Vec<SlackMessage> {
        val.get("messages")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| serde_json::from_value::<SlackMessage>(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Verify the token and return basic auth info.
    pub async fn auth_test(&self) -> Result<SlackAuthTestResult> {
        let val = self.post("auth.test", &[]).await?;
        let result = serde_json::from_value::<SlackAuthTestResult>(val)?;
        Ok(result)
    }

    /// Return all IM (1:1 DM) and MPIM channels, paginated.
    pub async fn list_im_channels(&self) -> Result<Vec<SlackChannel>> {
        let mut channels: Vec<SlackChannel> = Vec::new();
        let mut cursor = String::new();

        loop {
            let mut params: Vec<(&str, &str)> = vec![
                ("types", "im,mpim"),
                ("limit", "200"),
            ];
            if !cursor.is_empty() {
                params.push(("cursor", &cursor));
            }
            // borrow issue: keep cursor as owned
            let cursor_owned = cursor.clone();
            let mut params2: Vec<(&str, &str)> = vec![
                ("types", "im,mpim"),
                ("limit", "200"),
            ];
            if !cursor_owned.is_empty() {
                params2.push(("cursor", &cursor_owned));
            }

            let val = self.get("conversations.list", &params2).await?;

            let batch: Vec<SlackChannel> = val
                .get("channels")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| serde_json::from_value::<SlackChannel>(v.clone()).ok())
                        .collect()
                })
                .unwrap_or_default();

            channels.extend(batch);

            let next = val
                .pointer("/response_metadata/next_cursor")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            if next.is_empty() {
                break;
            }
            cursor = next;
        }

        Ok(channels)
    }

    /// Return recent messages for a channel, optionally since `oldest`.
    /// Bot/app messages and messages with a subtype are filtered out.
    pub async fn get_history(
        &self,
        channel_id: &str,
        oldest: Option<&str>,
        limit: u32,
    ) -> Result<Vec<SlackMessage>> {
        let limit_str = limit.to_string();
        let mut params: Vec<(&str, &str)> =
            vec![("channel", channel_id), ("limit", &limit_str)];
        let oldest_owned = oldest.map(|s| s.to_string());
        if let Some(ref o) = oldest_owned {
            params.push(("oldest", o));
        }

        let val = self.get("conversations.history", &params).await?;
        let messages = Self::parse_messages(&val);

        let filtered = messages
            .into_iter()
            .filter(|m| m.subtype.is_none() && m.bot_id.is_none() && m.app_id.is_none())
            .collect();

        Ok(filtered)
    }

    /// Return a single message identified by its timestamp.
    pub async fn get_message_by_ts(
        &self,
        channel_id: &str,
        ts: &str,
    ) -> Result<Option<SlackMessage>> {
        let params = [
            ("channel", channel_id),
            ("latest", ts),
            ("limit", "1"),
            ("inclusive", "true"),
        ];
        let val = self.get("conversations.history", &params).await?;
        let messages = Self::parse_messages(&val);
        Ok(messages.into_iter().next())
    }

    /// Fetch user information.
    pub async fn get_user_info(&self, user_id: &str) -> Result<SlackUser> {
        let val = self.get("users.info", &[("user", user_id)]).await?;
        let user = serde_json::from_value::<SlackUser>(
            val.get("user").cloned().unwrap_or(Value::Null),
        )?;
        Ok(user)
    }

    /// Return replies in a thread, skipping the root message.
    pub async fn get_replies(
        &self,
        channel_id: &str,
        thread_ts: &str,
        oldest: Option<&str>,
    ) -> Result<Vec<SlackMessage>> {
        let oldest_owned = oldest.map(|s| s.to_string());
        let mut params: Vec<(&str, &str)> = vec![
            ("channel", channel_id),
            ("ts", thread_ts),
        ];
        if let Some(ref o) = oldest_owned {
            params.push(("oldest", o));
        }

        let val = self.get("conversations.replies", &params).await?;
        let mut messages = Self::parse_messages(&val);

        // Skip the first message (the thread root).
        if !messages.is_empty() {
            messages.remove(0);
        }

        Ok(messages)
    }

    /// Add a reaction emoji to a message (non-fatal: already_reacted is ignored).
    pub async fn add_reaction(
        &self,
        channel_id: &str,
        ts: &str,
        emoji: &str,
    ) -> Result<()> {
        let result = self
            .post("reactions.add", &[
                ("channel", channel_id),
                ("timestamp", ts),
                ("name", emoji),
            ])
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("already_reacted") {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Post a message to a channel, optionally in a thread.
    pub async fn post_message(
        &self,
        channel_id: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<()> {
        let mut body = json!({ "channel": channel_id, "text": text });
        if let Some(ts) = thread_ts {
            body["thread_ts"] = json!(ts);
        }
        self.post_json("chat.postMessage", &body).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Minimal URL-encoding (percent-encode non-alphanumeric bytes)
// ---------------------------------------------------------------------------

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
