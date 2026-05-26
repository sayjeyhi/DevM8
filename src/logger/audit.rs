use std::fs;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde_json::json;

use super::rotate::rotate_if_needed;

// Audit log rotates at double the regular log's 10 MiB threshold.
const AUDIT_MAX_BYTES: u64 = 20 * 1024 * 1024;
const AUDIT_KEEP_COUNT: u32 = 5;

pub struct AuditLogger {
    path: PathBuf,
}

impl AuditLogger {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        Self { path }
    }

    /// Append one audit record. Never panics — failures are silently dropped.
    /// `extra` is merged into the top-level JSON object (keys from `extra` win on collision).
    pub fn log_action(
        &self,
        user_id: i64,
        username: &str,
        action: &str,
        detail: &str,
        extra: Option<serde_json::Value>,
    ) {
        let ts = Local::now().to_rfc3339();
        let mut record = json!({
            "ts": ts,
            "user_id": user_id,
            "username": username,
            "action": action,
            "detail": detail,
        });
        if let Some(serde_json::Value::Object(map)) = extra {
            if let serde_json::Value::Object(ref mut base) = record {
                base.extend(map);
            }
        }
        self.append(&record.to_string());
    }

    /// Rotate audit.log if it exceeds 20 MiB (double the regular log threshold).
    pub fn rotate_if_needed(&self) {
        let _ = rotate_if_needed(&self.path, Some(AUDIT_MAX_BYTES), Some(AUDIT_KEEP_COUNT));
    }

    fn append(&self, line: &str) {
        use std::fs::OpenOptions;
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = f.write_all(format!("{line}\n").as_bytes());
        }
    }
}
