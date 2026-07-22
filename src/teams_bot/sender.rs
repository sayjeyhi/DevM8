use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::channel::{ChannelSender, Keyboard, SentMessageRef};

use super::types::{ActivityResponse, TeamsActivity, TokenResponse};

const MAX_MSG_LEN: usize = 25_000;

struct CachedToken {
    token: String,
    expires_at: Instant,
}

/// Sends messages to Microsoft Teams via the Bot Framework REST API.
#[derive(Clone)]
pub struct TeamsSender {
    client: reqwest::Client,
    app_id: Arc<String>,
    app_password: Arc<String>,
    /// OAuth token endpoint URL. Differs between single-tenant and multi-tenant bots.
    token_url: Arc<String>,
    /// Shared token cache across all clones of this sender.
    token_cache: Arc<Mutex<Option<CachedToken>>>,
    /// Stores the last message text per activity_id so edit_keyboard can re-use it.
    msg_text_cache: Arc<DashMap<String, String>>,
}

impl TeamsSender {
    /// `tenant_id` — required for Single Tenant bots (Azure Portal → Entra ID → Tenant ID).
    /// Pass `None` only for Multi Tenant bots.
    pub fn new(app_id: String, app_password: String, tenant_id: Option<&str>) -> Self {
        let token_url = match tenant_id {
            Some(tid) => format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
                tid
            ),
            None => {
                "https://login.microsoftonline.com/botframework.com/oauth2/v2.0/token".to_string()
            }
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client for TeamsSender");
        Self {
            client,
            app_id: Arc::new(app_id),
            app_password: Arc::new(app_password),
            token_url: Arc::new(token_url),
            token_cache: Arc::new(Mutex::new(None)),
            msg_text_cache: Arc::new(DashMap::new()),
        }
    }

    async fn get_token(&self) -> Result<String> {
        let mut guard = self.token_cache.lock().await;
        if let Some(ref t) = *guard {
            // Refresh 2 minutes before actual expiry.
            if t.expires_at > Instant::now() + Duration::from_secs(120) {
                return Ok(t.token.clone());
            }
        }
        let resp = self
            .client
            .post(self.token_url.as_str())
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.app_id.as_str()),
                ("client_secret", self.app_password.as_str()),
                ("scope", "https://api.botframework.com/.default"),
            ])
            .send()
            .await?
            .json::<TokenResponse>()
            .await?;
        let token = resp.access_token.clone();
        *guard = Some(CachedToken {
            token: resp.access_token,
            expires_at: Instant::now() + Duration::from_secs(resp.expires_in.saturating_sub(120)),
        });
        Ok(token)
    }

    async fn post_activity(&self, chat_id: &str, body: Value) -> Result<String> {
        let (service_url, conv_id) = TeamsActivity::decode_chat_id(chat_id);
        anyhow::ensure!(
            !service_url.is_empty() && !conv_id.is_empty(),
            "invalid Teams chat_id"
        );
        let url = format!(
            "{}/v3/conversations/{}/activities",
            service_url.trim_end_matches('/'),
            conv_id
        );
        let token = self.get_token().await?;
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Teams API {status}: {text}");
        }
        Ok(resp.json::<ActivityResponse>().await?.id)
    }

    async fn put_activity(&self, chat_id: &str, activity_id: &str, body: Value) -> Result<()> {
        let (service_url, conv_id) = TeamsActivity::decode_chat_id(chat_id);
        let url = format!(
            "{}/v3/conversations/{}/activities/{}",
            service_url.trim_end_matches('/'),
            conv_id,
            activity_id
        );
        let token = self.get_token().await?;
        let resp = self
            .client
            .put(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Teams update {status}: {text}");
        }
        Ok(())
    }

    async fn del_activity(&self, chat_id: &str, activity_id: &str) -> Result<()> {
        let (service_url, conv_id) = TeamsActivity::decode_chat_id(chat_id);
        let url = format!(
            "{}/v3/conversations/{}/activities/{}",
            service_url.trim_end_matches('/'),
            conv_id,
            activity_id
        );
        let token = self.get_token().await?;
        let _ = self.client.delete(&url).bearer_auth(&token).send().await;
        Ok(())
    }

    fn adaptive_card(text: &str, keyboard: &Keyboard) -> Value {
        let body: Vec<Value> = if text.is_empty() {
            vec![]
        } else {
            vec![json!({ "type": "TextBlock", "text": text, "wrap": true })]
        };
        let actions: Vec<Value> = keyboard
            .iter()
            .flat_map(|row| {
                row.iter().map(|btn| {
                    json!({
                        "type": "Action.Submit",
                        "title": btn.label,
                        "data": { "devm8_action": btn.data }
                    })
                })
            })
            .collect();
        json!({
            "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
            "type": "AdaptiveCard",
            "version": "1.4",
            "body": body,
            "actions": actions
        })
    }

    fn card_body(text: &str, keyboard: &Keyboard) -> Value {
        json!({
            "type": "message",
            "text": "",
            "attachments": [{
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": Self::adaptive_card(text, keyboard)
            }]
        })
    }

    fn truncate(text: &str) -> String {
        if text.chars().count() <= MAX_MSG_LEN {
            text.to_string()
        } else {
            format!(
                "{}…",
                text.chars().take(MAX_MSG_LEN - 1).collect::<String>()
            )
        }
    }
}

#[async_trait]
impl ChannelSender for TeamsSender {
    async fn send(&self, chat_id: &str, text: &str) -> Result<SentMessageRef> {
        let text = Self::truncate(text);
        let id = self
            .post_activity(chat_id, json!({ "type": "message", "text": text }))
            .await?;
        self.msg_text_cache.insert(id.clone(), text);
        Ok(SentMessageRef::new(chat_id, id))
    }

    async fn send_with_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<SentMessageRef> {
        let text = Self::truncate(text);
        let body = Self::card_body(&text, &keyboard);
        let id = self.post_activity(chat_id, body).await?;
        self.msg_text_cache.insert(id.clone(), text);
        Ok(SentMessageRef::new(chat_id, id))
    }

    async fn edit_text(&self, msg_ref: &SentMessageRef, text: &str) -> Result<()> {
        let text = Self::truncate(text);
        self.msg_text_cache
            .insert(msg_ref.message_id.clone(), text.clone());
        self.put_activity(
            &msg_ref.chat_id,
            &msg_ref.message_id,
            json!({ "type": "message", "text": text }),
        )
        .await
    }

    async fn edit_with_keyboard(
        &self,
        msg_ref: &SentMessageRef,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<()> {
        let text = Self::truncate(text);
        self.msg_text_cache
            .insert(msg_ref.message_id.clone(), text.clone());
        let body = Self::card_body(&text, &keyboard);
        self.put_activity(&msg_ref.chat_id, &msg_ref.message_id, body)
            .await
    }

    async fn edit_keyboard(&self, msg_ref: &SentMessageRef, keyboard: Keyboard) -> Result<()> {
        let text = self
            .msg_text_cache
            .get(&msg_ref.message_id)
            .map(|v| v.clone())
            .unwrap_or_default();
        let body = Self::card_body(&text, &keyboard);
        self.put_activity(&msg_ref.chat_id, &msg_ref.message_id, body)
            .await
    }

    async fn delete_message(&self, msg_ref: &SentMessageRef) {
        self.msg_text_cache.remove(&msg_ref.message_id);
        let _ = self
            .del_activity(&msg_ref.chat_id, &msg_ref.message_id)
            .await;
    }

    fn start_typing(&self, chat_id: &str) -> JoinHandle<()> {
        let sender = self.clone();
        let chat_id = chat_id.to_string();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            loop {
                interval.tick().await;
                let _ = sender
                    .post_activity(&chat_id, json!({ "type": "typing" }))
                    .await;
            }
        })
    }

    fn escape(&self, text: &str) -> String {
        text.replace('\\', "\\\\")
            .replace('*', "\\*")
            .replace('_', "\\_")
            .replace('[', "\\[")
            .replace(']', "\\]")
    }

    fn bold(&self, text: &str) -> String {
        format!("**{}**", text)
    }

    fn italic(&self, text: &str) -> String {
        format!("_{}_", text)
    }

    fn code(&self, text: &str) -> String {
        format!("`{}`", text)
    }

    fn code_block(&self, text: &str) -> String {
        format!("```\n{}\n```", text)
    }

    fn link(&self, url: &str, label: &str) -> String {
        format!("[{}]({})", label, url)
    }

    fn system_context_prefix(&self) -> &'static str {
        "\
[Context: You are responding inside a Microsoft Teams bot. Your text reply is the ONLY output \
the user sees — there is no terminal or separate display. Rules:\
\n- When you run a command or read a file, ALWAYS include the actual output verbatim in your \
reply. Never say it was \"shown\", \"displayed\", or \"listed above\".\
\n- Format code/output in markdown code blocks so it renders cleanly.\
\n- Keep replies concise but complete — do not truncate data the user asked for.]\
\n\n---\n\n"
    }

    fn channel_name(&self) -> &'static str {
        "teams"
    }

    async fn send_in_chunks(&self, chat_id: &str, text: &str) -> Result<()> {
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
