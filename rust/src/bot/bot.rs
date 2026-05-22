use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::config::schema::AppConfig;
use crate::logger::Logger;

/// Build an `AppState` and start the Telegram polling loop.
#[allow(dead_code)]
pub async fn start_bot_from_config(
    config: &AppConfig,
    ct: CancellationToken,
    logger: Arc<dyn Logger>,
) -> anyhow::Result<()> {
    super::polling::start_polling(ct, &logger, config).await
}
