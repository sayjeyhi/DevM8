/// Platform-agnostic button for interactive messages.
#[derive(Debug, Clone)]
pub struct Button {
    pub label: String,
    /// Colon-separated action identifier, e.g. "jira:move:PROJ-123".
    pub action: String,
}

impl Button {
    pub fn new(label: impl Into<String>, action: impl Into<String>) -> Self {
        Self { label: label.into(), action: action.into() }
    }
}

/// A row of buttons (displayed side by side).
pub type ButtonRow = Vec<Button>;

/// A grid of buttons (keyboard / interactive controls).
pub type Keyboard = Vec<ButtonRow>;

/// Reference to a sent message — used to edit or remove it later.
#[derive(Debug, Clone)]
pub struct SentMessageRef {
    /// Chat / channel identifier (string for platform neutrality).
    pub chat_id: String,
    /// Platform-specific message identifier.
    /// Telegram: `message_id` serialised as a string.
    /// Slack:    `ts` (timestamp string, e.g. "1713000000.123456").
    pub message_id: String,
}

impl SentMessageRef {
    pub fn new(chat_id: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self { chat_id: chat_id.into(), message_id: message_id.into() }
    }
}
