pub mod parse_args;
pub mod split_message;
pub mod typing;

pub use parse_args::parse_first_and_rest;
pub use split_message::split_message;
pub use typing::keep_typing;

/// Escape HTML special characters for Telegram HTML parse mode.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
