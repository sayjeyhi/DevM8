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

/// How many conversations to check per poll cycle. Slack's rate limit for
/// conversations.history/replies/info is per-minute across the whole app, so
/// this is a batch rather than "all of them" to avoid blowing through it once
/// there's more than a handful of conversations — but checking more than one
/// per tick (the old behavior) meaningfully cuts delivery latency.
const CHANNELS_PER_TICK: usize = 5;

/// Async handler function type for new messages.
pub type MessageHandler = Box<
    dyn Fn(SlackNewMessage) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
        + Send
        + Sync,
>;

/// An error callback for non-fatal poller errors.
pub type ErrorHandler = Box<dyn Fn(anyhow::Error) + Send + Sync>;

/// Polls Slack for new messages at a configured interval. DMs and group DMs
/// are always watched; public/private channels are only watched if listed in
/// `allowed_channel_ids` (same semantics as the Socket Mode bot's channel gate).
pub struct SlackPoller {
    client: Arc<SlackClient>,
    interval_ms: u64,
    allowed_channel_ids: Vec<String>,
    on_message: Arc<MessageHandler>,
    on_error: Option<Arc<ErrorHandler>>,
    logger: Arc<dyn Logger>,
}

impl SlackPoller {
    pub fn new(
        client: Arc<SlackClient>,
        interval_ms: u64,
        allowed_channel_ids: Vec<String>,
        on_message: MessageHandler,
        on_error: Option<ErrorHandler>,
        logger: Arc<dyn Logger>,
    ) -> Self {
        Self {
            client,
            interval_ms,
            allowed_channel_ids,
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
                let fetched = self.client.list_conversations().await.inspect_err(|e| {
                    self.logger.error(
                        "slack: failed to list conversations",
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

        // DMs/MPIMs are always watched; channels only if explicitly allowed.
        let channels: Vec<SlackChannel> = channels
            .into_iter()
            .filter(|c| {
                c.is_im || c.is_mpim || self.allowed_channel_ids.iter().any(|id| id == &c.id)
            })
            .collect();

        if channels.is_empty() {
            self.logger.debug("slack: no conversations to poll", None);
            return Ok(());
        }

        let now_ts = unix_now_ts();

        // Own user ID, used to exclude self-sent messages from forwarding.
        // Cached on the client after the first successful lookup.
        let own_user_id = self.client.get_own_user_id().await.ok();

        // ----------------------------------------------------------------
        // Round-robin: check a batch of conversations per poll cycle. Slack's
        // rate limit for conversations.history/replies/info is per-minute
        // across the whole app, not per-conversation, so looping over every
        // one of them on every tick can blow through it once there's more
        // than a handful. Batching (rather than exactly one, as before)
        // trades a bit of that headroom for materially lower delivery
        // latency; spreading the remainder across cycles keeps growth in
        // conversation count from re-introducing the same problem.
        // ----------------------------------------------------------------
        let batch_size = CHANNELS_PER_TICK.min(channels.len());

        for _ in 0..batch_size {
            let idx = *cursor % channels.len();
            *cursor = (*cursor + 1) % channels.len();
            let channel = channels[idx].clone();

            if channel.is_archived.unwrap_or(false) {
                continue;
            }

            match self
                .process_channel(&channel, &mut state, &now_ts, own_user_id.as_deref())
                .await
            {
                Ok(changed) => state_changed = state_changed || changed,
                Err(e) => {
                    if e.downcast_ref::<SlackRateLimitError>().is_some() {
                        if state_changed {
                            save_slack_state(&state).await?;
                        }
                        return Err(e);
                    }
                    self.logger.error(
                        "slack: failed processing conversation",
                        Some(&serde_json::json!({ "channel_id": channel.id, "error": e.to_string() })),
                    );
                }
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

    /// Fetch and forward new messages (and thread replies) for a single
    /// conversation. Returns whether any state was updated.
    async fn process_channel(
        &self,
        channel: &SlackChannel,
        state: &mut SlackState,
        now_ts: &str,
        own_user_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let last_ts = state.last_ts.get(&channel.id).cloned();

        if last_ts.is_none() {
            // First time seeing this channel — bookmark now, skip history.
            state.last_ts.insert(channel.id.clone(), now_ts.to_string());
            return Ok(true);
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

        // Best-effort: Slack's own read cursor for this conversation, so
        // messages already read directly in Slack aren't re-forwarded. If
        // this fails (e.g. missing scope), fall back to forwarding everything.
        let last_read = self.client.get_last_read(&channel.id).await.unwrap_or(None);

        let mut new_last_ts: Option<String> = None;
        let mut active_thread: Option<String> = None; // thread_ts, if any
        let mut changed = false;

        for msg in &reversed {
            new_last_ts = Some(msg.ts.clone());

            // Track the most recent thread with replies (checked below).
            if msg.reply_count.unwrap_or(0) > 0 {
                active_thread = Some(msg.ts.clone());
            }

            if is_own_message(own_user_id, &msg.user) || is_already_read(&last_read, &msg.ts) {
                continue;
            }

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
        }

        if let Some(ts) = new_last_ts {
            state.last_ts.insert(channel.id.clone(), ts);
            changed = true;
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
                new_last_reply_ts = Some(reply.ts.clone());

                if is_own_message(own_user_id, &reply.user) || is_already_read(&last_read, &reply.ts)
                {
                    continue;
                }

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
            }

            if let Some(ts) = new_last_reply_ts {
                state
                    .thread_ts
                    .entry(channel.id.clone())
                    .or_default()
                    .insert(thread_ts, ts);
                changed = true;
            }
        }

        Ok(changed)
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

/// Whether `sender` is the authenticated user's own ID (i.e. a message the
/// user sent themselves, which should not be forwarded back to them).
fn is_own_message(own_user_id: Option<&str>, sender: &Option<String>) -> bool {
    match (own_user_id, sender) {
        (Some(me), Some(sender)) => me == sender,
        _ => false,
    }
}

/// Whether `ts` is at or before Slack's own read cursor for the conversation
/// (i.e. already marked read directly in Slack). Malformed timestamps are
/// treated as "not read" so they still get forwarded.
fn is_already_read(last_read: &Option<String>, ts: &str) -> bool {
    let Some(last_read) = last_read else {
        return false;
    };
    match (ts.parse::<f64>(), last_read.parse::<f64>()) {
        (Ok(ts), Ok(last_read)) => ts <= last_read,
        _ => false,
    }
}
