#![allow(dead_code)]

use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::time::{sleep, Duration};
use tracing::{error, info};

use super::{
    client::{SlackClient, SlackRateLimitError},
    state::{load_slack_state, save_slack_state, SlackState},
    types::{SlackChannel, SlackNewMessage},
};

/// How long (ms) to cache the channel list before re-fetching.
const CHANNEL_CACHE_TTL_MS: u128 = 5 * 60 * 1_000;

/// Async handler function type for new messages.
pub type MessageHandler = Box<
    dyn Fn(SlackNewMessage) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
        + Send
        + Sync,
>;

/// An error callback for non-fatal poller errors.
pub type ErrorHandler = Box<dyn Fn(anyhow::Error) + Send + Sync>;

/// Polls Slack for new DM/MPIM messages at a configured interval.
pub struct SlackPoller {
    client: Arc<SlackClient>,
    interval_ms: u64,
    on_message: Arc<MessageHandler>,
    on_error: Option<Arc<ErrorHandler>>,
}

impl SlackPoller {
    pub fn new(
        client: Arc<SlackClient>,
        interval_ms: u64,
        on_message: MessageHandler,
        on_error: Option<ErrorHandler>,
    ) -> Self {
        Self {
            client,
            interval_ms,
            on_message: Arc::new(on_message),
            on_error: on_error.map(Arc::new),
        }
    }

    /// Run the poll loop until the `cancelled` flag is set to `true`.
    pub async fn start(&self, cancelled: Arc<std::sync::atomic::AtomicBool>) {
        let mut channel_cache: Option<(Vec<SlackChannel>, std::time::Instant)> = None;

        loop {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            match self.poll(&mut channel_cache).await {
                Ok(()) => {}
                Err(e) => {
                    // Check if it's a rate-limit error.
                    if let Some(rle) = e.downcast_ref::<SlackRateLimitError>() {
                        let wait_ms = rle.retry_after_seconds * 1_000;
                        info!("Slack rate limited; waiting {}ms", wait_ms);
                        sleep(Duration::from_millis(wait_ms)).await;
                        continue;
                    }

                    if let Some(ref handler) = self.on_error {
                        handler(e);
                    } else {
                        error!("SlackPoller error: {}", e);
                    }
                }
            }

            sleep(Duration::from_millis(self.interval_ms)).await;
        }
    }

    // -----------------------------------------------------------------------
    // Internal poll cycle
    // -----------------------------------------------------------------------

    async fn poll(
        &self,
        channel_cache: &mut Option<(Vec<SlackChannel>, std::time::Instant)>,
    ) -> anyhow::Result<()> {
        let mut state = load_slack_state().await?;
        let mut state_changed = false;

        // Refresh channel list if cache is stale.
        let channels = {
            let now = std::time::Instant::now();
            let needs_refresh = channel_cache
                .as_ref()
                .map(|(_, ts)| now.duration_since(*ts).as_millis() > CHANNEL_CACHE_TTL_MS)
                .unwrap_or(true);

            if needs_refresh {
                let fetched = self.client.list_im_channels().await?;
                *channel_cache = Some((fetched, now));
            }

            channel_cache.as_ref().unwrap().0.clone()
        };

        let now_ts = unix_now_ts();

        // ----------------------------------------------------------------
        // Process each channel
        // ----------------------------------------------------------------
        let mut active_threads: Vec<(String, String)> = Vec::new(); // (channel_id, thread_ts)

        for channel in &channels {
            // Skip archived.
            if channel.is_archived.unwrap_or(false) {
                continue;
            }

            let last_ts = state.last_ts.get(&channel.id).cloned();

            if last_ts.is_none() {
                // First time seeing this channel — bookmark now, skip history.
                state.last_ts.insert(channel.id.clone(), now_ts.clone());
                state_changed = true;
                continue;
            }

            let oldest = last_ts.as_deref();
            let messages = self.client.get_history(&channel.id, oldest, 50).await?;

            // Iterate in chronological order (oldest first).
            let mut reversed: Vec<_> = messages;
            reversed.reverse();

            let mut new_last_ts: Option<String> = None;

            for msg in &reversed {
                let sender_name = self.resolve_username(&msg.user).await;

                let new_msg = SlackNewMessage {
                    channel: channel.clone(),
                    message: msg.clone(),
                    sender_name,
                };

                if let Err(e) = (self.on_message)(new_msg).await {
                    error!("Message handler error: {}", e);
                }

                new_last_ts = Some(msg.ts.clone());

                // Track threads (messages with replies).
                if msg.reply_count.unwrap_or(0) > 0 {
                    active_threads.push((channel.id.clone(), msg.ts.clone()));
                }
            }

            if let Some(ts) = new_last_ts {
                state.last_ts.insert(channel.id.clone(), ts);
                state_changed = true;
            }
        }

        // ----------------------------------------------------------------
        // Process thread replies
        // ----------------------------------------------------------------
        for (channel_id, thread_ts) in &active_threads {
            let last_reply_ts = state
                .thread_ts
                .get(channel_id)
                .and_then(|m| m.get(thread_ts))
                .cloned();

            let replies = self
                .client
                .get_replies(channel_id, thread_ts, last_reply_ts.as_deref())
                .await?;

            // Find the channel object.
            let channel = match channels.iter().find(|c| &c.id == channel_id) {
                Some(c) => c.clone(),
                None => continue,
            };

            let mut new_last_reply_ts: Option<String> = None;

            for reply in &replies {
                let sender_name = self.resolve_username(&reply.user).await;

                let new_msg = SlackNewMessage {
                    channel: channel.clone(),
                    message: reply.clone(),
                    sender_name,
                };

                if let Err(e) = (self.on_message)(new_msg).await {
                    error!("Reply handler error: {}", e);
                }

                new_last_reply_ts = Some(reply.ts.clone());
            }

            if let Some(ts) = new_last_reply_ts {
                state
                    .thread_ts
                    .entry(channel_id.clone())
                    .or_insert_with(HashMap::new)
                    .insert(thread_ts.clone(), ts);
                state_changed = true;
            }
        }

        // ----------------------------------------------------------------
        // Persist state if anything changed
        // ----------------------------------------------------------------
        if state_changed {
            save_slack_state(&state).await?;
        }

        Ok(())
    }

    /// Try to resolve a Slack user ID to a display name, falling back to the ID.
    async fn resolve_username(&self, user_id: &Option<String>) -> String {
        let id = match user_id {
            Some(id) => id,
            None => return "unknown".to_string(),
        };

        match self.client.get_user_info(id).await {
            Ok(user) => {
                // Prefer display_name → real_name → name.
                user.profile
                    .as_ref()
                    .and_then(|p| p.display_name.as_deref().filter(|s| !s.is_empty()))
                    .or_else(|| {
                        user.profile
                            .as_ref()
                            .and_then(|p| p.real_name.as_deref().filter(|s| !s.is_empty()))
                    })
                    .or(user.real_name.as_deref())
                    .unwrap_or(&user.name)
                    .to_string()
            }
            Err(_) => id.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unix_now_ts() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}.000000", secs)
}
