use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::bot::AppState;
use crate::logger::Logger;

use super::sender::TeamsSender;

/// Start the Teams webhook server.
///
/// If `config.teams` is absent, returns `Ok(())` immediately — the other
/// channels continue unaffected.
pub async fn start_teams_bot(
    ct: CancellationToken,
    state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
) -> anyhow::Result<()> {
    let teams_cfg = match state.config.teams.as_ref() {
        Some(cfg) => cfg,
        None => {
            logger.info("teams bot not configured — skipping", None);
            return Ok(());
        }
    };

    logger.info("teams bot starting", None);

    let sender = Arc::new(TeamsSender::new(
        teams_cfg.app_id.clone(),
        teams_cfg.app_password.clone(),
        teams_cfg.tenant_id.as_deref(),
    ));
    let port = teams_cfg.port;

    crate::teams_bot::webhook::run_webhook(ct, state, logger, sender, port).await
}
