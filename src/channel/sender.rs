use async_trait::async_trait;
use tokio::task::JoinHandle;

use super::types::{Keyboard, SentMessageRef};

/// Platform-agnostic message sender.
///
/// Implementations wrap a specific chat platform (Telegram, Slack, …).
/// Commands call only these methods — they never import platform-specific types.
#[async_trait]
pub trait ChannelSender: Send + Sync {
    // ---- Sending ----

    /// Send a formatted message. Returns a ref suitable for future editing.
    async fn send(&self, chat_id: &str, text: &str) -> anyhow::Result<SentMessageRef>;

    /// Send a message with an interactive button keyboard.
    async fn send_with_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        keyboard: Keyboard,
    ) -> anyhow::Result<SentMessageRef>;

    // ---- Editing ----

    /// Replace the text of a previously sent message.
    async fn edit_text(&self, msg_ref: &SentMessageRef, text: &str) -> anyhow::Result<()>;

    /// Replace both the text and keyboard of a previously sent message.
    async fn edit_with_keyboard(
        &self,
        msg_ref: &SentMessageRef,
        text: &str,
        keyboard: Keyboard,
    ) -> anyhow::Result<()>;

    /// Remove the keyboard from a previously sent message, keeping the text.
    async fn remove_keyboard(&self, msg_ref: &SentMessageRef) -> anyhow::Result<()>;

    // ---- Typing indicator ----

    /// Spawn a background task that keeps emitting a "typing" signal.
    /// Callers abort the returned handle once the long operation finishes.
    fn start_typing(&self, chat_id: &str) -> JoinHandle<()>;

    // ---- Platform limits ----

    /// Maximum number of characters per outgoing message.
    fn max_message_len(&self) -> usize;

    // ---- Text formatting ----

    /// Wrap text in bold markup for this platform.
    fn bold(&self, text: &str) -> String;

    /// Wrap text in italic markup.
    fn italic(&self, text: &str) -> String;

    /// Wrap text as inline code.
    fn code(&self, text: &str) -> String;

    /// Wrap text in a multi-line code/pre block.
    fn code_block(&self, text: &str) -> String;

    /// Render a hyperlink.
    fn link(&self, url: &str, label: &str) -> String;

    /// Escape characters that would be interpreted as markup on this platform.
    fn escape(&self, text: &str) -> String;

    /// A one-line context prefix injected into every Claude prompt so the AI
    /// knows which platform it is talking through.
    fn system_context_prefix(&self) -> &'static str;
}
