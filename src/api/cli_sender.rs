use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use crate::channel::{ChannelSender, Keyboard, SentMessageRef};

use super::protocol::{AskEvent, ChoiceDto, KeyboardDto};

fn to_keyboard_dto(keyboard: &Keyboard) -> KeyboardDto {
    keyboard
        .iter()
        .map(|row| {
            row.iter()
                .map(|b| ChoiceDto {
                    label: b.label.clone(),
                    data: b.data.clone(),
                })
                .collect()
        })
        .collect()
}

/// `ChannelSender` implementation for the devm8-client API: emits a JSON event
/// per call instead of making a native platform request. `/v1/ask` and
/// `/v1/solve` stream these events back to the client over SSE.
pub struct CliSender {
    tx: UnboundedSender<AskEvent>,
    seq: AtomicU64,
}

impl CliSender {
    pub fn new(tx: UnboundedSender<AskEvent>) -> Self {
        Self {
            tx,
            seq: AtomicU64::new(0),
        }
    }

    fn next_id(&self) -> String {
        format!("m{}", self.seq.fetch_add(1, Ordering::Relaxed))
    }

    fn emit(&self, event: AskEvent) {
        let _ = self.tx.send(event);
    }
}

#[async_trait]
impl ChannelSender for CliSender {
    async fn send(&self, chat_id: &str, text: &str) -> Result<SentMessageRef> {
        let id = self.next_id();
        self.emit(AskEvent::Text {
            id: id.clone(),
            text: text.to_string(),
        });
        Ok(SentMessageRef::new(chat_id, id))
    }

    async fn send_with_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<SentMessageRef> {
        let id = self.next_id();
        self.emit(AskEvent::Keyboard {
            id: id.clone(),
            text: text.to_string(),
            choices: to_keyboard_dto(&keyboard),
        });
        Ok(SentMessageRef::new(chat_id, id))
    }

    async fn edit_text(&self, msg_ref: &SentMessageRef, text: &str) -> Result<()> {
        self.emit(AskEvent::EditText {
            id: msg_ref.message_id.clone(),
            text: text.to_string(),
        });
        Ok(())
    }

    async fn edit_with_keyboard(
        &self,
        msg_ref: &SentMessageRef,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<()> {
        self.emit(AskEvent::EditWithKeyboard {
            id: msg_ref.message_id.clone(),
            text: text.to_string(),
            choices: to_keyboard_dto(&keyboard),
        });
        Ok(())
    }

    async fn edit_keyboard(&self, msg_ref: &SentMessageRef, keyboard: Keyboard) -> Result<()> {
        self.emit(AskEvent::EditKeyboard {
            id: msg_ref.message_id.clone(),
            choices: to_keyboard_dto(&keyboard),
        });
        Ok(())
    }

    async fn delete_message(&self, msg_ref: &SentMessageRef) {
        self.emit(AskEvent::Delete {
            id: msg_ref.message_id.clone(),
        });
    }

    fn start_typing(&self, _chat_id: &str) -> JoinHandle<()> {
        // No native "typing" indicator over SSE — progress is already
        // communicated via the Keyboard/EditWithKeyboard "Thinking..." events.
        tokio::spawn(async {})
    }

    fn escape(&self, text: &str) -> String {
        text.to_string()
    }

    fn bold(&self, text: &str) -> String {
        format!("**{text}**")
    }

    fn italic(&self, text: &str) -> String {
        format!("_{text}_")
    }

    fn code(&self, text: &str) -> String {
        format!("`{text}`")
    }

    fn code_block(&self, text: &str) -> String {
        format!("```\n{text}\n```")
    }

    fn link(&self, url: &str, label: &str) -> String {
        format!("[{label}]({url})")
    }

    fn system_context_prefix(&self) -> &'static str {
        "\
[Context: You are responding inside the devm8-client terminal. Your text reply is the ONLY \
output the user sees. Rules:\
\n- When you run a command or read a file, ALWAYS include the actual output verbatim in your \
reply.\
\n- Format code/output in markdown code blocks.\
\n- There are no inline buttons — numbered choices are rendered as a plain list.\
\n- Keep replies concise but complete.]\
\n\n---\n\n"
    }

    fn channel_name(&self) -> &'static str {
        "cli"
    }

    async fn send_in_chunks(&self, chat_id: &str, text: &str) -> Result<()> {
        // No platform message-size limit over SSE/JSON — send as one event.
        self.send(chat_id, text).await.map(|_| ())
    }
}
