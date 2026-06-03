/// A reference to a previously sent message, used for editing.
#[derive(Debug, Clone)]
pub struct SentMessageRef {
    pub chat_id: String,
    pub message_id: String,
}

impl SentMessageRef {
    pub fn new(chat_id: &str, message_id: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.to_string(),
            message_id: message_id.into(),
        }
    }
}

/// A single inline keyboard button.
#[derive(Debug, Clone)]
pub struct Button {
    pub label: String,
    pub data: String,
}

impl Button {
    pub fn new(label: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            data: data.into(),
        }
    }
}

/// A keyboard is a grid of buttons (rows of columns).
pub type Keyboard = Vec<Vec<Button>>;
