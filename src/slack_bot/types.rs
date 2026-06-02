use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Socket Mode envelope
// ---------------------------------------------------------------------------

/// Wraps every incoming Socket Mode WebSocket frame.
#[derive(Debug, Clone, Deserialize)]
pub struct SocketEnvelope {
    pub envelope_id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Slack API responses
// ---------------------------------------------------------------------------

/// Response from `chat.postMessage`.
#[derive(Debug, Deserialize)]
pub struct PostMessageResponse {
    pub ok: bool,
    pub ts: Option<String>,
    pub error: Option<String>,
}

/// Response from `apps.connections.open`.
#[derive(Debug, Deserialize)]
pub struct ConnectionsOpenResponse {
    pub ok: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

/// Response from `chat.update`.
#[derive(Debug, Deserialize)]
pub struct UpdateMessageResponse {
    pub ok: bool,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// ACK frame sent back to Slack for each envelope
// ---------------------------------------------------------------------------

/// The JSON frame we send back to Slack to acknowledge receipt of an envelope.
#[derive(Debug, Serialize)]
pub struct AckFrame {
    pub envelope_id: String,
    pub payload: String,
}
