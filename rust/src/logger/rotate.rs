use std::fs;
use std::io;
use std::path::Path;

const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB
const DEFAULT_KEEP_COUNT: u32 = 5;

/// Rotate `log_file` if it exceeds `max_bytes`.
///
/// Rotation scheme (same as TypeScript):
///   app.log   → app.log.1
///   app.log.1 → app.log.2
///   …
///   app.log.N is deleted
///   A new empty app.log is created.
///
/// Errors are propagated to the caller.
pub fn rotate_if_needed(
    log_file: impl AsRef<Path>,
    max_bytes: Option<u64>,
    keep_count: Option<u32>,
) -> io::Result<()> {
    let log_file = log_file.as_ref();
    let max_bytes = max_bytes.unwrap_or(DEFAULT_MAX_BYTES);
    let keep_count = keep_count.unwrap_or(DEFAULT_KEEP_COUNT);

    // If the file doesn't exist, nothing to do.
    let meta = match fs::metadata(log_file) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    if meta.len() < max_bytes {
        return Ok(());
    }

    // Shift existing numbered backups: N-1 → N, …, 1 → 2
    for i in (1..keep_count).rev() {
        let src = numbered(log_file, i);
        let dst = numbered(log_file, i + 1);
        if src.exists() {
            fs::rename(&src, &dst)?;
        }
    }

    // Delete the oldest backup that now falls beyond keep_count
    let oldest = numbered(log_file, keep_count + 1);
    if oldest.exists() {
        fs::remove_file(&oldest)?;
    }

    // Rotate current log → .1
    fs::rename(log_file, numbered(log_file, 1))?;

    // Create a fresh empty log file
    fs::File::create(log_file)?;

    Ok(())
}

fn numbered(base: &Path, n: u32) -> std::path::PathBuf {
    let mut s = base.as_os_str().to_os_string();
    s.push(format!(".{n}"));
    std::path::PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn no_rotation_when_file_is_small() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        fs::write(&log, b"small").unwrap();
        rotate_if_needed(&log, Some(1024), Some(3)).unwrap();
        assert!(log.exists());
        assert!(!numbered(&log, 1).exists());
    }

    #[test]
    fn rotates_when_file_exceeds_limit() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        let mut f = fs::File::create(&log).unwrap();
        f.write_all(&vec![b'x'; 2048]).unwrap();
        drop(f);

        rotate_if_needed(&log, Some(1024), Some(3)).unwrap();

        assert!(log.exists(), "new empty log should exist");
        assert!(numbered(&log, 1).exists(), "rotated backup .1 should exist");
        assert_eq!(fs::metadata(&log).unwrap().len(), 0, "new log should be empty");
    }
}
