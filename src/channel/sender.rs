use async_trait::async_trait;
use anyhow::Result;
use tokio::task::JoinHandle;

use super::types::{Button, Keyboard, SentMessageRef};

/// Platform-agnostic interface for sending messages and interacting with users.
#[async_trait]
pub trait ChannelSender: Send + Sync {
    /// Send a plain text message (may include platform markup).
    async fn send(&self, chat_id: &str, text: &str) -> Result<SentMessageRef>;

    /// Send a message with an inline keyboard.
    async fn send_with_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<SentMessageRef>;

    /// Edit the text of a previously sent message.
    async fn edit_text(&self, msg_ref: &SentMessageRef, text: &str) -> Result<()>;

    /// Edit the text and keyboard of a previously sent message.
    async fn edit_with_keyboard(
        &self,
        msg_ref: &SentMessageRef,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<()>;

    /// Edit only the reply markup (keyboard) of a previously sent message.
    async fn edit_keyboard(&self, msg_ref: &SentMessageRef, keyboard: Keyboard) -> Result<()>;

    /// Delete a previously sent message. Errors are silently ignored.
    async fn delete_message(&self, msg_ref: &SentMessageRef);

    /// Start a background typing indicator.
    /// The returned handle should be aborted when the operation completes.
    fn start_typing(&self, chat_id: &str) -> JoinHandle<()>;

    /// Escape text for the platform's markup format.
    fn escape(&self, text: &str) -> String;

    /// Wrap text in bold markup.
    fn bold(&self, text: &str) -> String;

    /// Wrap text in italic markup.
    fn italic(&self, text: &str) -> String;

    /// Wrap text in inline code markup.
    fn code(&self, text: &str) -> String;

    /// Wrap text in a code block.
    fn code_block(&self, text: &str) -> String;

    /// Create a hyperlink.
    fn link(&self, url: &str, label: &str) -> String;

    /// A system context prefix injected at the start of Claude prompts.
    fn system_context_prefix(&self) -> &'static str;

    /// Send a long message split into chunks if needed.
    async fn send_in_chunks(&self, chat_id: &str, text: &str) -> Result<()>;
}

#[allow(dead_code)]
pub fn single_button(label: impl Into<String>, data: impl Into<String>) -> Keyboard {
    vec![vec![Button::new(label, data)]]
}
