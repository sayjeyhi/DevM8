pub mod parse_args;
pub mod split_message;

pub use parse_args::{parse_first_and_rest, project_key_from_args};
pub use split_message::split_message;

/// Escape HTML special characters for Telegram HTML parse mode.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
