use serde::{Deserialize, Serialize};
use serde_json::Value;

#[allow(dead_code)]
/// Incoming Bot Framework Activity from Teams.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TeamsActivity {
    #[serde(rename = "type", default)]
    pub activity_type: String,
    pub id: Option<String>,
    pub text: Option<String>,
    /// Present on Adaptive Card Action.Submit clicks.
    pub value: Option<Value>,
    pub from: Option<TeamsAccount>,
    pub conversation: Option<TeamsConversation>,
    #[serde(rename = "serviceUrl", default)]
    pub service_url: Option<String>,
    /// Present on invoke activities (e.g. Universal Actions).
    pub name: Option<String>,
}

impl TeamsActivity {
    /// Returns the button callback string if this is an Adaptive Card submit.
    pub fn button_data(&self) -> Option<&str> {
        self.value.as_ref()?.get("devm8_action")?.as_str()
    }

    pub fn user_id(&self) -> &str {
        self.from.as_ref().map(|f| f.id.as_str()).unwrap_or("")
    }

    pub fn user_name(&self) -> &str {
        self.from
            .as_ref()
            .and_then(|f| f.name.as_deref())
            .unwrap_or("")
    }

    pub fn conversation_id(&self) -> &str {
        self.conversation
            .as_ref()
            .map(|c| c.id.as_str())
            .unwrap_or("")
    }

    /// Encodes `service_url + '\n' + conversation_id` as a single chat_id string.
    pub fn chat_id(&self) -> String {
        format!(
            "{}\n{}",
            self.service_url.as_deref().unwrap_or(""),
            self.conversation_id()
        )
    }

    /// Splits a chat_id back into `(service_url, conversation_id)`.
    pub fn decode_chat_id(chat_id: &str) -> (&str, &str) {
        chat_id.split_once('\n').unwrap_or(("", chat_id))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeamsAccount {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeamsConversation {
    pub id: String,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct ActivityResponse {
    pub id: String,
}
