use teloxide::prelude::*;
use teloxide::types::ChatAction;
use tokio::task::JoinHandle;

/// Spawn a background task that sends "typing" chat action every 4 seconds.
///
/// Callers should abort the returned handle once the operation completes.
pub fn keep_typing(bot: Bot, chat_id: ChatId) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(4));
        loop {
            interval.tick().await;
            let _ = bot.send_chat_action(chat_id, ChatAction::Typing).await;
        }
    })
}
