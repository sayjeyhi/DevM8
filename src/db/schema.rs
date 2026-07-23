use rusqlite::Connection;

/// Current schema version. Bump this and add a branch in `apply_migrations`
/// whenever the schema changes.
const SCHEMA_VERSION: i64 = 2;

/// Apply all pending schema migrations. Idempotent — safe to call on every startup.
pub fn apply_migrations(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_meta (version INTEGER NOT NULL);")?;

    let current: i64 = conn
        .query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    if current < 1 {
        conn.execute_batch(V1_MIGRATION)?;
    }

    if current < 2 {
        conn.execute_batch(V2_MIGRATION)?;
    }

    if current == 0 {
        conn.execute(
            "INSERT INTO schema_meta (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )?;
    } else if current < SCHEMA_VERSION {
        conn.execute("UPDATE schema_meta SET version = ?1", [SCHEMA_VERSION])?;
    }

    Ok(())
}

const V1_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS users (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  email         TEXT NOT NULL UNIQUE COLLATE NOCASE,
  display_name  TEXT,
  is_admin      INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS user_channel_identities (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  channel       TEXT NOT NULL CHECK (channel IN ('telegram','slack','teams','cli')),
  external_id   TEXT NOT NULL,
  display_name  TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  UNIQUE(channel, external_id)
);
CREATE INDEX IF NOT EXISTS idx_uci_user ON user_channel_identities(user_id);

CREATE TABLE IF NOT EXISTS auth_tokens (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash    TEXT NOT NULL UNIQUE,
  label         TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  last_used_at  TEXT,
  revoked_at    TEXT
);
CREATE INDEX IF NOT EXISTS idx_auth_tokens_user ON auth_tokens(user_id);

CREATE TABLE IF NOT EXISTS pairing_codes (
  code          TEXT PRIMARY KEY,
  user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at    TEXT NOT NULL,
  used_at       TEXT
);

CREATE TABLE IF NOT EXISTS chat_history (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  project_key   TEXT,
  email         TEXT NOT NULL REFERENCES users(email) ON UPDATE CASCADE ON DELETE CASCADE,
  channel       TEXT NOT NULL,
  session_id    TEXT NOT NULL,
  role          TEXT NOT NULL CHECK (role IN ('user','assistant','system')),
  content       TEXT NOT NULL,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX IF NOT EXISTS idx_chat_history_email_project ON chat_history(email, project_key, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_history_session ON chat_history(session_id, created_at);
"#;

const V2_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS user_active_project (
  email         TEXT PRIMARY KEY REFERENCES users(email) ON UPDATE CASCADE ON DELETE CASCADE,
  project_key   TEXT NOT NULL,
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
"#;
