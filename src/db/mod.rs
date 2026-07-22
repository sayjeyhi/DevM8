// Several methods here (session listing, token/pairing-code issuance) are wired
// up by the API server and Telegram history command in later milestones.
#![allow(dead_code)]

pub mod migrate;
pub mod schema;

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

/// Wraps a single SQLite connection behind a mutex. All access happens via
/// `tokio::task::spawn_blocking` — a single mutexed connection in WAL mode is
/// simple, correct, and fast enough at this scale (no connection pool needed).
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct ChatHistoryRow {
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub project_key: Option<String>,
    pub channel: String,
    pub first_message: String,
    pub started_at: String,
}

impl Db {
    /// Open (creating if needed) the SQLite database at `path` and apply migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite db at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        schema::apply_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    async fn blocking<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let guard = conn.lock().unwrap_or_else(|e| e.into_inner());
            f(&guard)
        })
        .await
        .context("db task panicked")?
    }

    /// Resolve a channel-native user ID (Telegram numeric ID, Slack `U...`, etc.)
    /// to the email of the devm8 user it's mapped to, if any.
    pub async fn resolve_email(&self, channel: &str, external_id: &str) -> Result<Option<String>> {
        let channel = channel.to_string();
        let external_id = external_id.to_string();
        self.blocking(move |conn| {
            conn.query_row(
                "SELECT u.email FROM user_channel_identities uci \
                 JOIN users u ON u.id = uci.user_id \
                 WHERE uci.channel = ?1 AND uci.external_id = ?2",
                params![channel, external_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(anyhow::Error::from)
        })
        .await
    }

    /// Look up the email mapped to `channel`/`external_id`, auto-provisioning a
    /// placeholder user+identity on first sight so chat history is never dropped
    /// before an admin runs `devm8 migrate-users`.
    pub async fn get_or_create_user_for_channel(
        &self,
        channel: &str,
        external_id: &str,
        display_name: Option<&str>,
    ) -> Result<String> {
        let channel = channel.to_string();
        let external_id = external_id.to_string();
        let display_name = display_name.map(|s| s.to_string());
        self.blocking(move |conn| {
            if let Some(email) = conn
                .query_row(
                    "SELECT u.email FROM user_channel_identities uci \
                     JOIN users u ON u.id = uci.user_id \
                     WHERE uci.channel = ?1 AND uci.external_id = ?2",
                    params![channel, external_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            {
                return Ok(email);
            }

            let placeholder_email = format!("{channel}-{external_id}@unmapped.devm8.local");
            conn.execute(
                "INSERT INTO users (email, display_name) VALUES (?1, ?2) \
                 ON CONFLICT(email) DO NOTHING",
                params![placeholder_email, display_name],
            )?;
            let user_id: i64 = conn.query_row(
                "SELECT id FROM users WHERE email = ?1",
                params![placeholder_email],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO user_channel_identities \
                 (user_id, channel, external_id, display_name) VALUES (?1, ?2, ?3, ?4)",
                params![user_id, channel, external_id, display_name],
            )?;
            Ok(placeholder_email)
        })
        .await
    }

    /// Mark the user identified by `email` as admin (or not).
    pub async fn set_admin_by_email(&self, email: &str, is_admin: bool) -> Result<()> {
        let email = email.to_string();
        self.blocking(move |conn| {
            conn.execute(
                "UPDATE users SET is_admin = ?1 WHERE email = ?2",
                params![is_admin as i64, email],
            )?;
            Ok(())
        })
        .await
    }

    /// All channel identities (channel, external_id) mapped to `email`.
    pub async fn list_channel_identities_for_email(
        &self,
        email: &str,
    ) -> Result<Vec<(String, String)>> {
        let email = email.to_string();
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT uci.channel, uci.external_id FROM user_channel_identities uci \
                 JOIN users u ON u.id = uci.user_id WHERE u.email = ?1",
            )?;
            let rows = stmt
                .query_map(params![email], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Append one chat turn (user question or assistant answer) to history.
    pub async fn record_chat_turn(
        &self,
        project_key: Option<String>,
        email: String,
        channel: String,
        session_id: String,
        role: &'static str,
        content: String,
    ) -> Result<()> {
        self.blocking(move |conn| {
            conn.execute(
                "INSERT INTO chat_history \
                 (project_key, email, channel, session_id, role, content) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![project_key, email, channel, session_id, role, content],
            )?;
            Ok(())
        })
        .await
    }

    /// Distinct project keys that appear in `email`'s chat history (most recent first).
    pub async fn list_project_keys_for_email(&self, email: &str) -> Result<Vec<String>> {
        let email = email.to_string();
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT project_key FROM chat_history WHERE email = ?1 AND project_key IS NOT NULL \
                 GROUP BY project_key ORDER BY MAX(created_at) DESC",
            )?;
            let rows = stmt
                .query_map(params![email], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Paginated session summaries for `email`, optionally filtered to one project.
    pub async fn list_sessions(
        &self,
        email: &str,
        project_key: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SessionSummary>> {
        let email = email.to_string();
        let project_key = project_key.map(|s| s.to_string());
        self.blocking(move |conn| {
            let sql = "SELECT session_id, project_key, channel, \
                       (SELECT content FROM chat_history c2 WHERE c2.session_id = c1.session_id \
                        AND c2.role = 'user' ORDER BY c2.created_at ASC LIMIT 1) AS first_message, \
                       MIN(created_at) AS started_at \
                       FROM chat_history c1 \
                       WHERE email = ?1 AND (?2 IS NULL OR project_key = ?2) \
                       GROUP BY session_id ORDER BY started_at DESC LIMIT ?3 OFFSET ?4";
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map(params![email, project_key, limit, offset], |row| {
                    Ok(SessionSummary {
                        session_id: row.get(0)?,
                        project_key: row.get(1)?,
                        channel: row.get(2)?,
                        first_message: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        started_at: row.get(4)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Full transcript for one session, in chronological order.
    pub async fn get_session_history(&self, session_id: &str) -> Result<Vec<ChatHistoryRow>> {
        let session_id = session_id.to_string();
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT role, content, created_at FROM chat_history \
                 WHERE session_id = ?1 ORDER BY created_at ASC, id ASC",
            )?;
            let rows = stmt
                .query_map(params![session_id], |row| {
                    Ok(ChatHistoryRow {
                        role: row.get(0)?,
                        content: row.get(1)?,
                        created_at: row.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Create a short-lived pairing code for `email` (creating the user if needed).
    /// Returns the code. `ttl_minutes` controls expiry.
    pub async fn create_pairing_code(&self, email: &str, ttl_minutes: i64) -> Result<String> {
        let email = email.to_string();
        self.blocking(move |conn| {
            conn.execute(
                "INSERT INTO users (email) VALUES (?1) ON CONFLICT(email) DO NOTHING",
                params![email],
            )?;
            let user_id: i64 = conn.query_row(
                "SELECT id FROM users WHERE email = ?1",
                params![email],
                |row| row.get(0),
            )?;
            let code = generate_pairing_code();
            conn.execute(
                "INSERT INTO pairing_codes (code, user_id, expires_at) \
                 VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?3))",
                params![code, user_id, format!("+{ttl_minutes} minutes")],
            )?;
            Ok(code)
        })
        .await
    }

    /// Redeem a pairing code: mints a bearer token, marks the code used, returns
    /// `(token, email)`. Fails if the code is unknown, already used, or expired.
    pub async fn redeem_pairing_code(
        &self,
        code: &str,
        label: Option<&str>,
    ) -> Result<(String, String)> {
        let code = code.to_string();
        let label = label.map(|s| s.to_string());
        self.blocking(move |conn| {
            let (user_id, email, expires_at, used_at): (i64, String, String, Option<String>) = conn
                .query_row(
                    "SELECT p.user_id, u.email, p.expires_at, p.used_at \
                     FROM pairing_codes p JOIN users u ON u.id = p.user_id WHERE p.code = ?1",
                    params![code],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?
                .context("unknown pairing code")?;

            if used_at.is_some() {
                anyhow::bail!("pairing code already used");
            }
            let now = conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |row| {
                row.get::<_, String>(0)
            })?;
            if now > expires_at {
                anyhow::bail!("pairing code expired");
            }

            let token = generate_token();
            let token_hash = hash_token(&token);
            conn.execute(
                "INSERT INTO auth_tokens (user_id, token_hash, label) VALUES (?1, ?2, ?3)",
                params![user_id, token_hash, label],
            )?;
            conn.execute(
                "UPDATE pairing_codes SET used_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE code = ?1",
                params![code],
            )?;
            Ok((token, email))
        })
        .await
    }

    /// Verify a bearer token, returning `(email, is_admin)` and touching `last_used_at`.
    pub async fn verify_token(&self, token: &str) -> Result<Option<(String, bool)>> {
        let token_hash = hash_token(token);
        self.blocking(move |conn| {
            let found: Option<(i64, String, bool)> = conn
                .query_row(
                    "SELECT u.id, u.email, u.is_admin FROM auth_tokens t \
                     JOIN users u ON u.id = t.user_id \
                     WHERE t.token_hash = ?1 AND t.revoked_at IS NULL",
                    params![token_hash],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? != 0)),
                )
                .optional()?;
            if let Some((_, ref email, _)) = found {
                conn.execute(
                    "UPDATE auth_tokens SET last_used_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
                     WHERE token_hash = ?1",
                    params![token_hash],
                )?;
                let _ = email;
            }
            Ok(found.map(|(_, email, is_admin)| (email, is_admin)))
        })
        .await
    }

    /// List every channel identity still attached to an auto-provisioned
    /// placeholder user (`<channel>-<id>@unmapped.devm8.local`), for `devm8
    /// migrate-users` to walk through.
    pub async fn list_placeholder_users(&self) -> Result<Vec<(String, String, String)>> {
        self.blocking(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT uci.channel, uci.external_id, u.email FROM user_channel_identities uci \
                 JOIN users u ON u.id = uci.user_id \
                 WHERE u.email LIKE '%@unmapped.devm8.local' ORDER BY u.email",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Attach a real email to a placeholder user, or merge it into an existing
    /// user if `new_email` already belongs to someone else (e.g. the admin is
    /// attaching a second channel identity to a person already mapped via
    /// another channel). Chat history always survives, re-pointed to the
    /// surviving email.
    pub async fn rename_or_merge_user_email(&self, old_email: &str, new_email: &str) -> Result<()> {
        let old_email = old_email.to_string();
        let new_email = new_email.to_string();
        self.blocking(move |conn| {
            let old_id: i64 = conn.query_row(
                "SELECT id FROM users WHERE email = ?1",
                params![old_email],
                |row| row.get(0),
            )?;

            let existing: Option<(i64, bool)> = conn
                .query_row(
                    "SELECT id, is_admin FROM users WHERE email = ?1",
                    params![new_email],
                    |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
                )
                .optional()?;

            match existing {
                None => {
                    conn.execute(
                        "UPDATE users SET email = ?1 WHERE id = ?2",
                        params![new_email, old_id],
                    )?;
                }
                Some((target_id, _)) if target_id == old_id => {
                    // Already this email — nothing to do.
                }
                Some((target_id, target_was_admin)) => {
                    let old_was_admin: bool = conn.query_row(
                        "SELECT is_admin FROM users WHERE id = ?1",
                        params![old_id],
                        |row| Ok(row.get::<_, i64>(0)? != 0),
                    )?;
                    conn.execute(
                        "UPDATE user_channel_identities SET user_id = ?1 WHERE user_id = ?2",
                        params![target_id, old_id],
                    )?;
                    conn.execute(
                        "UPDATE chat_history SET email = ?1 WHERE email = ?2",
                        params![new_email, old_email],
                    )?;
                    if old_was_admin && !target_was_admin {
                        conn.execute(
                            "UPDATE users SET is_admin = 1 WHERE id = ?1",
                            params![target_id],
                        )?;
                    }
                    conn.execute("DELETE FROM users WHERE id = ?1", params![old_id])?;
                }
            }
            Ok(())
        })
        .await
    }

    /// Revoke every active token for `email` (used by `devm8-client logout`'s server-side call).
    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        let token_hash = hash_token(token);
        self.blocking(move |conn| {
            conn.execute(
                "UPDATE auth_tokens SET revoked_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') \
                 WHERE token_hash = ?1",
                params![token_hash],
            )?;
            Ok(())
        })
        .await
    }
}

fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// A short, human-typeable pairing code (Crockford-base32-ish alphabet, no ambiguous chars).
fn generate_pairing_code() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
    let mut rng = rand::rng();
    let chars: String = (0..10)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect();
    format!("{}-{}", &chars[..5], &chars[5..])
}
