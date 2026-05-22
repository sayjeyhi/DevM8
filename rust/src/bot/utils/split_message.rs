/// Reserve space for chunk prefix like `[1/9] `.
const PREFIX_RESERVE: usize = 8;

/// Split a potentially long text into Telegram-safe chunks (≤ `limit` chars each).
///
/// - Tries to split on paragraph boundaries (`\n\n`) first.
/// - Falls back to splitting on spaces, then hard-splits if necessary.
/// - When more than one chunk is produced, every chunk is prefixed `[N/Total] `.
pub fn split_message(text: &str, limit: usize) -> Vec<String> {
    if text.len() <= limit {
        return vec![text.to_string()];
    }

    let effective = limit.saturating_sub(PREFIX_RESERVE);

    let mut chunks: Vec<String> = Vec::new();

    /// Push a string that may be longer than `effective`, splitting on spaces
    /// or hard-splitting as a last resort.  Returns whatever didn't fit.
    fn push_long_text(s: &str, effective: usize, chunks: &mut Vec<String>) -> String {
        let mut remaining = s.to_string();
        while remaining.len() > effective {
            let slice = &remaining[..effective];
            let split_at = slice.rfind(' ').filter(|&p| p > 0).unwrap_or(effective);
            chunks.push(remaining[..split_at].to_string());
            remaining = remaining[split_at..].trim_start().to_string();
        }
        remaining
    }

    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut current = String::new();

    for para in &paragraphs {
        if para.len() > effective {
            // Flush current buffer first
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            current = push_long_text(para, effective, &mut chunks);
        } else {
            let candidate = if current.is_empty() {
                para.to_string()
            } else {
                format!("{}\n\n{}", current, para)
            };

            if candidate.len() > effective {
                if !current.is_empty() {
                    chunks.push(std::mem::take(&mut current));
                }
                current = para.to_string();
            } else {
                current = candidate;
            }
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.len() <= 1 {
        return chunks;
    }

    let total = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| format!("[{}/{}] {}", i + 1, total, chunk))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_message_is_unchanged() {
        let msg = "Hello world";
        assert_eq!(split_message(msg, 4096), vec![msg.to_string()]);
    }

    #[test]
    fn long_message_gets_numbered() {
        let para = "word ".repeat(200); // ~1000 chars
        let text = format!("{}\n\n{}", para.trim(), para.trim());
        let parts = split_message(&text, 300);
        assert!(parts.len() > 1);
        assert!(parts[0].starts_with("[1/"));
    }
}
