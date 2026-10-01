//! Presentation layer for the devm8-client terminal UI: color/TTY detection,
//! symbols, a live status spinner, markdown rendering, and the small text
//! cleanups the streamed events need (they embed Telegram-HTML tags).
//!
//! Everything here degrades gracefully: when stdout/stderr is not a terminal
//! (piped, redirected) or `NO_COLOR` is set, colors disappear, the spinner
//! becomes a no-op, and markdown prints verbatim — so scripting the client
//! keeps working unchanged.

use std::io::{IsTerminal, Write};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use regex::Regex;
use termimad::crossterm::style::{Attribute, Color};
use termimad::{CompoundStyle, MadSkin};

// ---------------------------------------------------------------------------
// TTY / color detection
// ---------------------------------------------------------------------------

fn term_is_dumb() -> bool {
    std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false)
}

fn decide(is_tty: bool) -> bool {
    // https://no-color.org — set to any non-empty value to disable.
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0") {
        return true;
    }
    is_tty && !term_is_dumb()
}

static OUT_COLOR: OnceLock<bool> = OnceLock::new();
static ERR_COLOR: OnceLock<bool> = OnceLock::new();

pub fn out_color() -> bool {
    *OUT_COLOR.get_or_init(|| decide(std::io::stdout().is_terminal()))
}

pub fn err_color() -> bool {
    *ERR_COLOR.get_or_init(|| decide(std::io::stderr().is_terminal()))
}

/// Whether prompts can be interactive (arrow-key selects): both stdin and
/// stdout must be a real terminal, otherwise fall back to the plain
/// numbered-choice `> ` loop.
pub fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal() && !term_is_dumb()
}

// ---------------------------------------------------------------------------
// ANSI styling
// ---------------------------------------------------------------------------

fn styled(enabled: bool, code: &str, s: &str) -> String {
    if enabled {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    styled(out_color(), "1", s)
}
pub fn dim(s: &str) -> String {
    styled(out_color(), "2", s)
}
pub fn cyan(s: &str) -> String {
    styled(out_color(), "36", s)
}
pub fn green(s: &str) -> String {
    styled(out_color(), "32", s)
}
pub fn yellow(s: &str) -> String {
    styled(out_color(), "33", s)
}
pub fn magenta(s: &str) -> String {
    styled(out_color(), "35", s)
}
pub fn gray(s: &str) -> String {
    styled(out_color(), "90", s)
}

/// Red for stderr (error messages) — keyed off stderr's own TTY state.
pub fn err_red(s: &str) -> String {
    styled(err_color(), "31", s)
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

pub const CHECK: &str = "✔";
pub const CROSS: &str = "✖";

/// A short dim separator line between conversation turns.
pub fn separator() -> String {
    dim(&"─".repeat(16))
}

// ---------------------------------------------------------------------------
// Live status spinner
// ---------------------------------------------------------------------------

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TICK: Duration = Duration::from_millis(90);

enum SpinnerMsg {
    Set(String),
    Stop,
}

/// A single-line spinner on stderr, used to render the server's "Thinking…"
/// / progress-preview status messages instead of printing each edit as a new
/// paragraph. Renders nothing at all when stderr is not a TTY, so piped runs
/// stay clean.
pub struct Spinner {
    tx: Option<Sender<SpinnerMsg>>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    pub fn start(msg: &str) -> Self {
        if !std::io::stderr().is_terminal() || term_is_dumb() {
            return Self {
                tx: None,
                handle: None,
            };
        }

        let (tx, rx) = mpsc::channel::<SpinnerMsg>();
        let initial = msg.to_string();
        let handle = std::thread::spawn(move || {
            let mut msg = initial;
            let mut frame = 0usize;
            let start = Instant::now();
            let mut err = std::io::stderr();
            let _ = write!(err, "\x1b[?25l"); // hide cursor
            let _ = err.flush();

            loop {
                match rx.recv_timeout(TICK) {
                    Ok(SpinnerMsg::Set(m)) => msg = m,
                    Ok(SpinnerMsg::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }
                let secs = start.elapsed().as_secs();
                let elapsed = if secs >= 3 {
                    format!(" ({secs}s)")
                } else {
                    String::new()
                };
                let line = if err_color() {
                    format!(
                        "\x1b[36m{}\x1b[0m \x1b[2m{}{}\x1b[0m",
                        FRAMES[frame], msg, elapsed
                    )
                } else {
                    format!("{} {}{}", FRAMES[frame], msg, elapsed)
                };
                let _ = write!(err, "\r\x1b[2K{line}");
                let _ = err.flush();
                frame = (frame + 1) % FRAMES.len();
            }

            let _ = write!(err, "\r\x1b[2K\x1b[?25h"); // clear line, restore cursor
            let _ = err.flush();
        });

        Self {
            tx: Some(tx),
            handle: Some(handle),
        }
    }

    pub fn update(&self, msg: &str) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(SpinnerMsg::Set(msg.to_string()));
        }
    }

    /// Clear the spinner line. Joins the worker thread so output printed
    /// afterwards can never race the clear.
    pub fn finish(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(SpinnerMsg::Stop);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

/// Render a markdown message (Claude's answers, menu texts) with the terminal
/// skin, falling back to verbatim text when colors are off/piped.
pub fn print_markdown(text: &str) {
    if out_color() {
        markdown_skin().print_text(text);
    } else {
        println!("{text}");
    }
}

fn markdown_skin() -> MadSkin {
    let mut skin = MadSkin::default();
    for h in skin.headers.iter_mut() {
        h.set_fg(Color::Cyan);
        h.compound_style.add_attr(Attribute::Bold);
    }
    skin.inline_code = CompoundStyle::new(Some(Color::Yellow), None, Attribute::Bold.into());
    skin.code_block.compound_style.set_fg(Color::Grey);
    skin.bullet.set_fg(Color::Cyan);
    skin
}

// ---------------------------------------------------------------------------
// Text cleanup
// ---------------------------------------------------------------------------

/// Remove the Telegram-HTML tags the chat flows embed for the Telegram
/// channel (`<b>`, `<pre>`, `<a href=…>`, …). The CLI channel renders
/// markdown, so these tags would otherwise show up literally.
pub fn strip_html(text: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)</?(?:b|strong|i|em|u|s|strike|code|pre)>|<a\s[^>]*>|</a>").unwrap()
    });
    re.replace_all(text, "").into_owned()
}

/// Collapse a status/progress text into a single spinner line: strip HTML and
/// markdown noise, join lines with " · ", and cap the length.
pub fn one_line(text: &str) -> String {
    let text = strip_html(text);
    let text = text.replace("```", " ");
    let text = text.replace(['`', '*'], "");
    static LINK: OnceLock<Regex> = OnceLock::new();
    let link = LINK.get_or_init(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
    let text = link.replace_all(&text, "$1");
    let mut parts = Vec::new();
    for line in text.lines() {
        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            parts.push(collapsed);
        }
    }
    let mut joined = parts.join(" · ");
    if joined.chars().count() > 72 {
        joined = format!("{}…", joined.chars().take(71).collect::<String>());
    }
    joined
}

/// Character-count truncation for previews.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars().take(max.saturating_sub(1)).collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_removes_telegram_tags() {
        assert_eq!(
            strip_html("Analyzing <b>PROJ-1</b>..."),
            "Analyzing PROJ-1..."
        );
        assert_eq!(
            strip_html("<pre>$ cargo build\nok</pre>"),
            "$ cargo build\nok"
        );
        assert_eq!(strip_html(r#"<a href="https://x.com">link</a>"#), "link");
        // markdown syntax is not HTML and must survive
        assert_eq!(strip_html("**bold** `code`"), "**bold** `code`");
    }

    #[test]
    fn one_line_joins_lines_not_words() {
        assert_eq!(one_line("Thinking..."), "Thinking...");
        assert_eq!(
            one_line("<pre>$ cargo build\n   Compiling devm8 v0.1.0</pre>"),
            "$ cargo build · Compiling devm8 v0.1.0"
        );
        assert_eq!(one_line("see [docs](https://x.io) now"), "see docs now");
        assert_eq!(one_line("**Done**: `all good`"), "Done: all good");
    }

    #[test]
    fn one_line_truncates_long_texts() {
        let long = "x".repeat(100);
        let out = one_line(&long);
        assert!(out.chars().count() <= 72);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_counts_chars() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 3), "he…");
    }
}
