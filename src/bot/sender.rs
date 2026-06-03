use anyhow::Result;
use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::types::{
    ChatAction, InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode,
};
use tokio::task::JoinHandle;

use crate::bot::utils::split_message;
use crate::channel::types::{Button, Keyboard, SentMessageRef};
use crate::channel::ChannelSender;

/// Telegram implementation of `ChannelSender`.
pub struct TelegramSender {
    bot: Bot,
}

impl TelegramSender {
    pub fn new(bot: Bot) -> Self {
        Self { bot }
    }
}

fn build_telegram_keyboard(keyboard: Keyboard) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(
        keyboard
            .into_iter()
            .map(|row: Vec<Button>| {
                row.into_iter()
                    .map(|btn| InlineKeyboardButton::callback(btn.label, btn.data))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
    )
}

fn parse_chat_id(chat_id: &str) -> ChatId {
    ChatId(chat_id.parse::<i64>().unwrap_or(0))
}

fn parse_message_id(msg_id: &str) -> MessageId {
    MessageId(msg_id.parse::<i32>().unwrap_or(0))
}

#[async_trait]
impl ChannelSender for TelegramSender {
    async fn send(&self, chat_id: &str, text: &str) -> Result<SentMessageRef> {
        let sent = self
            .bot
            .send_message(parse_chat_id(chat_id), text)
            .parse_mode(ParseMode::Html)
            .await?;
        Ok(SentMessageRef::new(chat_id, sent.id.0.to_string()))
    }

    async fn send_with_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<SentMessageRef> {
        let kb = build_telegram_keyboard(keyboard);
        let sent = self
            .bot
            .send_message(parse_chat_id(chat_id), text)
            .parse_mode(ParseMode::Html)
            .reply_markup(kb)
            .await?;
        Ok(SentMessageRef::new(chat_id, sent.id.0.to_string()))
    }

    async fn edit_text(&self, msg_ref: &SentMessageRef, text: &str) -> Result<()> {
        let _ = self
            .bot
            .edit_message_text(
                parse_chat_id(&msg_ref.chat_id),
                parse_message_id(&msg_ref.message_id),
                text,
            )
            .parse_mode(ParseMode::Html)
            .await;
        Ok(())
    }

    async fn edit_with_keyboard(
        &self,
        msg_ref: &SentMessageRef,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<()> {
        let kb = build_telegram_keyboard(keyboard);
        let _ = self
            .bot
            .edit_message_text(
                parse_chat_id(&msg_ref.chat_id),
                parse_message_id(&msg_ref.message_id),
                text,
            )
            .parse_mode(ParseMode::Html)
            .reply_markup(kb)
            .await;
        Ok(())
    }

    async fn edit_keyboard(&self, msg_ref: &SentMessageRef, keyboard: Keyboard) -> Result<()> {
        let kb = build_telegram_keyboard(keyboard);
        let _ = self
            .bot
            .edit_message_reply_markup(
                parse_chat_id(&msg_ref.chat_id),
                parse_message_id(&msg_ref.message_id),
            )
            .reply_markup(kb)
            .await;
        Ok(())
    }

    async fn delete_message(&self, msg_ref: &SentMessageRef) {
        let _ = self
            .bot
            .delete_message(
                parse_chat_id(&msg_ref.chat_id),
                parse_message_id(&msg_ref.message_id),
            )
            .await;
    }

    fn start_typing(&self, chat_id: &str) -> JoinHandle<()> {
        let bot = self.bot.clone();
        let cid = parse_chat_id(chat_id);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(4));
            loop {
                interval.tick().await;
                let _ = bot.send_chat_action(cid, ChatAction::Typing).await;
            }
        })
    }

    fn escape(&self, text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    fn bold(&self, text: &str) -> String {
        format!("<b>{}</b>", text)
    }

    fn italic(&self, text: &str) -> String {
        format!("<i>{}</i>", text)
    }

    fn code(&self, text: &str) -> String {
        format!("<code>{}</code>", text)
    }

    fn code_block(&self, text: &str) -> String {
        format!("<pre>{}</pre>", text)
    }

    fn link(&self, url: &str, label: &str) -> String {
        format!("<a href=\"{}\">{}</a>", url, label)
    }

    fn system_context_prefix(&self) -> &'static str {
        "\
[Context: You are responding inside a Telegram bot. Your text reply is the ONLY output the \
user sees — there is no terminal or separate display. Rules:\
\n- When you run a command or read a file, ALWAYS include the actual output verbatim in your \
reply. Never say it was \"shown\", \"displayed\", or \"listed above\".\
\n- Format code/output in markdown code blocks so it renders cleanly.\
\n- Keep replies concise but complete — do not truncate data the user asked for.]\
\n\n---\n\n"
    }

    async fn send_in_chunks(&self, chat_id: &str, text: &str) -> Result<()> {
        let chunks = split_message(text, 4096);
        for chunk in &chunks {
            self.send(chat_id, chunk).await?;
        }
        Ok(())
    }
}
