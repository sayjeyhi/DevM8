use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tokio::task::JoinHandle;

use crate::channel::sender::ChannelSender;
use crate::channel::types::{Keyboard, SentMessageRef};

use super::types::{PostMessageResponse, UpdateMessageResponse};

const MAX_MSG_LEN: usize = 3000;

#[derive(Clone)]
pub struct SlackSender {
    client: reqwest::Client,
    bot_token: String,
}

impl SlackSender {
    pub fn new(bot_token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client for SlackSender");
        Self { client, bot_token }
    }

    async fn post_message_raw(
        &self,
        channel: &str,
        text: &str,
        blocks: Option<serde_json::Value>,
    ) -> anyhow::Result<String> {
        let mut body = json!({ "channel": channel, "text": text });
        if let Some(b) = blocks {
            body["blocks"] = b;
        }
        let resp = self
            .client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&self.bot_token)
            .json(&body)
            .send()
            .await?
            .json::<PostMessageResponse>()
            .await?;
        if resp.ok {
            Ok(resp.ts.unwrap_or_default())
        } else {
            Err(anyhow::anyhow!(
                "chat.postMessage failed: {}",
                resp.error.unwrap_or_else(|| "unknown error".into())
            ))
        }
    }

    async fn update_message_raw(
        &self,
        channel: &str,
        ts: &str,
        text: &str,
        blocks: Option<serde_json::Value>,
    ) -> anyhow::Result<()> {
        let mut body = json!({ "channel": channel, "ts": ts, "text": text });
        if let Some(b) = blocks {
            body["blocks"] = b;
        }
        let resp = self
            .client
            .post("https://slack.com/api/chat.update")
            .bearer_auth(&self.bot_token)
            .json(&body)
            .send()
            .await?
            .json::<UpdateMessageResponse>()
            .await?;
        if resp.ok {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "chat.update failed: {}",
                resp.error.unwrap_or_else(|| "unknown error".into())
            ))
        }
    }

    fn build_blocks(text: &str, keyboard: &Keyboard) -> serde_json::Value {
        let mut blocks = vec![json!({
            "type": "section",
            "text": { "type": "mrkdwn", "text": text }
        })];
        if !keyboard.is_empty() {
            let elements: Vec<serde_json::Value> = keyboard
                .iter()
                .flat_map(|row| {
                    row.iter().map(|btn| {
                        json!({
                            "type": "button",
                            "text": { "type": "plain_text", "text": btn.label },
                            "action_id": "devm8_action",
                            "value": btn.data,
                        })
                    })
                })
                .collect();
            blocks.push(json!({ "type": "actions", "elements": elements }));
        }
        json!(blocks)
    }

    fn truncate(text: &str) -> String {
        if text.chars().count() <= MAX_MSG_LEN {
            text.to_string()
        } else {
            let truncated: String = text.chars().take(MAX_MSG_LEN - 1).collect();
            format!("{}…", truncated)
        }
    }
}

#[async_trait]
impl ChannelSender for SlackSender {
    async fn send(&self, chat_id: &str, text: &str) -> anyhow::Result<SentMessageRef> {
        let text = Self::truncate(text);
        let blocks = json!([{
            "type": "section",
            "text": { "type": "mrkdwn", "text": text }
        }]);
        let ts = self.post_message_raw(chat_id, &text, Some(blocks)).await?;
        Ok(SentMessageRef::new(chat_id, ts))
    }

    async fn send_with_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        keyboard: Keyboard,
    ) -> anyhow::Result<SentMessageRef> {
        let text = Self::truncate(text);
        let blocks = Self::build_blocks(&text, &keyboard);
        let ts = self.post_message_raw(chat_id, &text, Some(blocks)).await?;
        Ok(SentMessageRef::new(chat_id, ts))
    }

    async fn edit_text(&self, msg_ref: &SentMessageRef, text: &str) -> anyhow::Result<()> {
        let text = Self::truncate(text);
        let blocks = json!([{
            "type": "section",
            "text": { "type": "mrkdwn", "text": text }
        }]);
        self.update_message_raw(&msg_ref.chat_id, &msg_ref.message_id, &text, Some(blocks))
            .await
    }

    async fn edit_with_keyboard(
        &self,
        msg_ref: &SentMessageRef,
        text: &str,
        keyboard: Keyboard,
    ) -> anyhow::Result<()> {
        let text = Self::truncate(text);
        let blocks = Self::build_blocks(&text, &keyboard);
        self.update_message_raw(&msg_ref.chat_id, &msg_ref.message_id, &text, Some(blocks))
            .await
    }

    async fn edit_keyboard(
        &self,
        msg_ref: &SentMessageRef,
        keyboard: Keyboard,
    ) -> anyhow::Result<()> {
        let elements: Vec<serde_json::Value> = keyboard
            .iter()
            .flat_map(|row| {
                row.iter().map(|btn| {
                    json!({
                        "type": "button",
                        "text": { "type": "plain_text", "text": btn.label },
                        "action_id": "devm8_action",
                        "value": btn.data,
                    })
                })
            })
            .collect();
        let blocks = json!([{ "type": "actions", "elements": elements }]);
        // Keep existing text (pass empty string — Slack preserves it when blocks present)
        self.update_message_raw(&msg_ref.chat_id, &msg_ref.message_id, "", Some(blocks))
            .await
    }

    async fn delete_message(&self, msg_ref: &SentMessageRef) {
        let body = json!({
            "channel": msg_ref.chat_id,
            "ts": msg_ref.message_id,
        });
        let _ = self
            .client
            .post("https://slack.com/api/chat.delete")
            .bearer_auth(&self.bot_token)
            .json(&body)
            .send()
            .await;
    }

    fn start_typing(&self, _chat_id: &str) -> JoinHandle<()> {
        tokio::spawn(async {})
    }

    fn escape(&self, text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    fn bold(&self, text: &str) -> String {
        format!("*{}*", text)
    }

    fn italic(&self, text: &str) -> String {
        format!("_{}_", text)
    }

    fn code(&self, text: &str) -> String {
        format!("`{}`", text)
    }

    fn code_block(&self, text: &str) -> String {
        format!("```{}```", text)
    }

    fn link(&self, url: &str, label: &str) -> String {
        format!("<{}|{}>", url, label)
    }

    fn system_context_prefix(&self) -> &'static str {
        "\
[Context: You are responding inside a Slack bot. Your text reply is the ONLY output the \
user sees. Rules:\
\n- When you run a command or read a file, ALWAYS include the actual output verbatim in your \
reply.\
\n- Format code/output in code blocks.\
\n- Keep replies concise but complete.]\
\n\n---\n\n"
    }

    async fn send_in_chunks(&self, chat_id: &str, text: &str) -> anyhow::Result<()> {
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;
        while start < chars.len() {
            let end = (start + MAX_MSG_LEN).min(chars.len());
            let chunk: String = chars[start..end].iter().collect();
            self.send(chat_id, &chunk).await?;
            start = end;
        }
        Ok(())
    }
}
