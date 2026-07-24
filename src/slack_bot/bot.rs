use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::bot::AppState;
use crate::logger::Logger;

/// Start the Slack Socket Mode bot.
///
/// If `config.slack` is absent or `bot_enabled()` returns false, returns `Ok(())`
/// immediately — the Telegram bot continues unaffected.
///
/// On disconnect the socket loop reconnects with exponential backoff.
pub async fn start_slack_bot(
    ct: CancellationToken,
    state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
) -> anyhow::Result<()> {
    // Guard: only start if bot token + app token are configured.
    let bot_enabled = state
        .config
        .slack
        .as_ref()
        .map(|sc| sc.bot_enabled())
        .unwrap_or(false);

    if !bot_enabled {
        logger.info(
            "slack bot not configured (bot_token / app_token missing) — skipping",
            None,
        );
        return Ok(());
    }

    logger.info("slack bot starting", None);

    let bot_token = state
        .config
        .slack
        .as_ref()
        .and_then(|sc| sc.bot_token.clone())
        .expect("bot_enabled implies bot_token is set");

    let bot_user_id = crate::slack::SlackClient::new(bot_token)
        .get_own_user_id()
        .await
        .map_err(|e| {
            logger.warn(
                &format!("slack bot: failed to resolve own user id via auth.test: {e}"),
                None,
            );
        })
        .unwrap_or_default();

    crate::slack_bot::socket::run_socket_loop(ct, state, logger, bot_user_id).await
}
