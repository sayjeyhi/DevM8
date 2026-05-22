use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::config::schema::AppConfig;
use crate::logger::Logger;

use super::AppState;

/// Build an `AppState` and start the Telegram polling loop.
///
/// This is the main entry point called from `daemon_command`.
pub async fn start_bot_from_config(
    config: &AppConfig,
    ct: CancellationToken,
    logger: Arc<dyn Logger>,
) -> anyhow::Result<()> {
    super::polling::start_polling(ct, &logger, config).await
}

pub use start_bot_from_config as run;
