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

    crate::slack_bot::socket::run_socket_loop(ct, state, logger).await
}
