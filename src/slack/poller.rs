#![allow(dead_code)]

use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::time::{sleep, Duration};

use crate::logger::Logger;

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
    logger: Arc<dyn Logger>,
}

impl SlackPoller {
    pub fn new(
        client: Arc<SlackClient>,
        interval_ms: u64,
        on_message: MessageHandler,
        on_error: Option<ErrorHandler>,
        logger: Arc<dyn Logger>,
    ) -> Self {
        Self {
            client,
            interval_ms,
            on_message: Arc::new(on_message),
            on_error: on_error.map(Arc::new),
            logger,
        }
    }

    /// Run the poll loop until the `cancelled` flag is set to `true`.
    pub async fn start(&self, cancelled: Arc<std::sync::atomic::AtomicBool>) {
        let mut channel_cache: Option<(Vec<SlackChannel>, std::time::Instant)> = None;
        let mut cursor: usize = 0;

        self.logger.info(
            "slack poller starting",
            Some(&serde_json::json!({ "interval_ms": self.interval_ms })),
        );

        loop {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            match self.poll(&mut channel_cache, &mut cursor).await {
                Ok(()) => {}
                Err(e) => {
                    // Check if it's a rate-limit error.
                    if let Some(rle) = e.downcast_ref::<SlackRateLimitError>() {
                        let wait_ms = rle.retry_after_seconds * 1_000;
                        self.logger.warn(
                            "slack poller rate limited",
                            Some(&serde_json::json!({ "wait_ms": wait_ms })),
                        );
                        sleep(Duration::from_millis(wait_ms)).await;
                        continue;
                    }

                    self.logger.error(
                        "slack poller error",
                        Some(&serde_json::json!({ "error": e.to_string() })),
                    );

                    if let Some(ref handler) = self.on_error {
                        handler(e);
                    }
                }
            }

            sleep(Duration::from_millis(self.interval_ms)).await;
        }

        self.logger.info("slack poller stopped", None);
    }

    // -----------------------------------------------------------------------
    // Internal poll cycle
    // -----------------------------------------------------------------------

    async fn poll(
        &self,
        channel_cache: &mut Option<(Vec<SlackChannel>, std::time::Instant)>,
        cursor: &mut usize,
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
                let fetched = self.client.list_im_channels().await.inspect_err(|e| {
                    self.logger.error(
                        "slack: failed to list im channels",
                        Some(&serde_json::json!({ "error": e.to_string() })),
                    );
                })?;
                self.logger.debug(
                    "slack: refreshed channel list",
                    Some(&serde_json::json!({ "channel_count": fetched.len() })),
                );
                *channel_cache = Some((fetched, now));
            }

            channel_cache.as_ref().unwrap().0.clone()
        };

        if channels.is_empty() {
            self.logger
                .debug("slack: no im/mpim channels to poll", None);
            return Ok(());
        }

        let now_ts = unix_now_ts();

        // ----------------------------------------------------------------
        // Round-robin: check exactly one channel per poll cycle. Slack's
        // rate limit for conversations.history/replies is per-minute across
        // the whole app, not per-channel, so looping over every DM on every
        // tick blows through it as soon as there's more than a handful of
        // channels. Spreading the work across cycles keeps us to at most a
        // couple of calls per `interval_ms`, at the cost of checking any
        // single channel less often.
        // ----------------------------------------------------------------
        let idx = *cursor % channels.len();
        *cursor = (*cursor + 1) % channels.len();
        let channel = &channels[idx];

        let mut active_thread: Option<String> = None; // thread_ts, if any

        if channel.is_archived.unwrap_or(false) {
            return Ok(());
        }

        let last_ts = state.last_ts.get(&channel.id).cloned();

        if last_ts.is_none() {
            // First time seeing this channel — bookmark now, skip history.
            state.last_ts.insert(channel.id.clone(), now_ts.clone());
            save_slack_state(&state).await?;
            return Ok(());
        }

        let oldest = last_ts.as_deref();
        let messages = self
            .client
            .get_history(&channel.id, oldest, 50)
            .await
            .inspect_err(|e| {
                self.logger.error(
                    "slack: failed to fetch channel history",
                    Some(&serde_json::json!({ "channel_id": channel.id, "error": e.to_string() })),
                );
            })?;

        // Iterate in chronological order (oldest first).
        let mut reversed: Vec<_> = messages;
        reversed.reverse();

        if !reversed.is_empty() {
            self.logger.debug(
                "slack: new messages found",
                Some(&serde_json::json!({ "channel_id": channel.id, "count": reversed.len() })),
            );
        }

        let mut new_last_ts: Option<String> = None;

        for msg in &reversed {
            let sender_name = self.resolve_username(&msg.user).await;

            let new_msg = SlackNewMessage {
                channel: channel.clone(),
                message: msg.clone(),
                sender_name,
            };

            if let Err(e) = (self.on_message)(new_msg).await {
                self.logger.error(
                    "slack: message handler error",
                    Some(&serde_json::json!({ "channel_id": channel.id, "error": e.to_string() })),
                );
            }

            new_last_ts = Some(msg.ts.clone());

            // Track the most recent thread with replies (checked below).
            if msg.reply_count.unwrap_or(0) > 0 {
                active_thread = Some(msg.ts.clone());
            }
        }

        if let Some(ts) = new_last_ts {
            state.last_ts.insert(channel.id.clone(), ts);
            state_changed = true;
        }

        // ----------------------------------------------------------------
        // Process replies for at most one thread in this channel.
        // ----------------------------------------------------------------
        if let Some(thread_ts) = active_thread {
            let last_reply_ts = state
                .thread_ts
                .get(&channel.id)
                .and_then(|m| m.get(&thread_ts))
                .cloned();

            let replies = self
                .client
                .get_replies(&channel.id, &thread_ts, last_reply_ts.as_deref())
                .await
                .inspect_err(|e| {
                    self.logger.error(
                        "slack: failed to fetch thread replies",
                        Some(&serde_json::json!({ "channel_id": channel.id, "thread_ts": thread_ts, "error": e.to_string() })),
                    );
                })?;

            let mut new_last_reply_ts: Option<String> = None;

            for reply in &replies {
                let sender_name = self.resolve_username(&reply.user).await;

                let new_msg = SlackNewMessage {
                    channel: channel.clone(),
                    message: reply.clone(),
                    sender_name,
                };

                if let Err(e) = (self.on_message)(new_msg).await {
                    self.logger.error(
                        "slack: reply handler error",
                        Some(&serde_json::json!({ "channel_id": channel.id, "error": e.to_string() })),
                    );
                }

                new_last_reply_ts = Some(reply.ts.clone());
            }

            if let Some(ts) = new_last_reply_ts {
                state
                    .thread_ts
                    .entry(channel.id.clone())
                    .or_insert_with(HashMap::new)
                    .insert(thread_ts, ts);
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
