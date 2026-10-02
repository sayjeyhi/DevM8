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
/// Bright cyan — the accent color used for focus markers (`❯`) and the
/// question echo, kept separate from `cyan` so the two can diverge later.
pub fn accent(s: &str) -> String {
    styled(out_color(), "96", s)
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
// Layout
// ---------------------------------------------------------------------------

/// Current terminal width, clamped to a usable range so boxes and separators
/// never collapse or overflow on odd window sizes (and degenerate to 80
/// columns when the size is unavailable, e.g. piped output).
pub fn term_width() -> usize {
    let (w, _) = termimad::terminal_size();
    (w as usize).clamp(40, 100)
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

pub const CHECK: &str = "✔";
pub const CROSS: &str = "✖";

/// A short dim separator line between conversation turns.
pub fn separator() -> String {
    dim(&"─".repeat(term_width()))
}

/// The content of a header box, layout-computed once so both renderers —
/// ANSI text for the linear mode ([`header_box`]) and ratatui spans for the
/// TUI — draw the identical shape.
pub struct HeaderData {
    pub title: String,
    pub rows: Vec<(String, String)>,
    /// Visible columns of the widest row content (excluding the 4 columns of
    /// border + padding every line carries).
    pub content_width: usize,
}

/// Compute the shared header layout at `max_width` terminal columns.
pub fn header_data(title: &str, rows: &[(&str, &str)], max_width: usize) -> HeaderData {
    let inner_max = max_width.saturating_sub(5).max(8);
    let label_width = rows
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0);

    let title_plain = truncate(title, inner_max);
    let mut content_width = title_plain.chars().count() + 1;
    let mut out_rows: Vec<(String, String)> = Vec::new();
    for (label, value) in rows {
        let budget = inner_max.saturating_sub(label_width + 2);
        let value_plain = truncate(value, budget);
        content_width = content_width.max(label_width + 2 + value_plain.chars().count());
        out_rows.push(((*label).to_string(), value_plain));
    }

    HeaderData {
        title: title_plain,
        rows: out_rows,
        content_width,
    }
}

/// A rounded header box in the opencode style: dim border, bold title, and
/// dim-labelled rows (e.g. `project  EISA`), labels padded to a shared
/// column. Values are plain strings; the box truncates rows rather than
/// wrapping so the shape always holds.
pub fn header_box(title: &str, rows: &[(&str, &str)]) -> String {
    let data = header_data(title, rows, term_width());
    let label_width = data
        .rows
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0);

    // Every line is exactly `content_width + 4` columns wide:
    //   top:    "╭─ " + title + " " + D×"─" + "╮"
    //   row:    "│ " + label (padded) + "  " + value + P spaces + " │"
    //   bottom: "╰" + B×"─" + "╯"
    // Labels are padded before styling: ANSI-wrapped strings would break the
    // visible-width arithmetic.
    let dashes = data.content_width - data.title.chars().count() - 1;
    let mut lines = vec![format!(
        "{}{} {}",
        dim("╭─ "),
        bold(&data.title),
        dim(&format!("{}╮", "─".repeat(dashes)))
    )];
    for (label, value) in &data.rows {
        let pad = data.content_width - label_width - value.chars().count() - 2;
        lines.push(format!(
            "{} {}  {}{} {}",
            dim("│"),
            dim(&format!("{:<label_width$}", label)),
            value,
            " ".repeat(pad),
            dim("│")
        ));
    }
    lines.push(format!(
        "{}{}",
        dim("╰"),
        dim(&format!("{}╯", "─".repeat(data.content_width + 2)))
    ));
    lines.join("\n")
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

/// The markdown skin. Shared by the linear renderer and the TUI (which uses
/// it for wrapping/layout decisions only, styling spans itself).
pub fn markdown_skin() -> MadSkin {
    let mut skin = MadSkin::default();
    for h in skin.headers.iter_mut() {
        h.compound_style.add_attr(Attribute::Bold);
    }
    skin.inline_code = CompoundStyle::new(Some(Color::Cyan), None, Attribute::Reset.into());
    skin.code_block.compound_style.set_fg(Color::Grey);
    skin.bullet.set_fg(Color::Cyan);
    skin.quote_mark.set_fg(Color::DarkCyan);
    skin.horizontal_rule.set_fg(Color::DarkCyan);
    skin.table.compound_style.set_fg(Color::DarkCyan);
    skin
}

// ---------------------------------------------------------------------------
// Prompt theme
// ---------------------------------------------------------------------------

/// The shared `inquire` render config: a bright-cyan `❯` focus marker for the
/// active prompt and highlighted option, dim help messages and scroll hints,
/// no `[?]` brackets — the quiet, single-accent look of opencode's TUI.
pub fn prompt_theme() -> inquire::ui::RenderConfig<'static> {
    use inquire::ui::{Attributes, Color, IndexPrefix, RenderConfig, StyleSheet, Styled};

    let accent = StyleSheet::new().with_fg(Color::LightCyan);
    let muted = StyleSheet::new().with_fg(Color::Grey);
    let focused = StyleSheet::new()
        .with_fg(Color::LightCyan)
        .with_attr(Attributes::BOLD);

    let mut theme = RenderConfig::default_colored()
        .with_prompt_prefix(Styled::new("❯").with_style_sheet(accent))
        .with_answered_prompt_prefix(Styled::new("·").with_style_sheet(muted))
        .with_answer(StyleSheet::new().with_attr(Attributes::BOLD))
        .with_help_message(muted)
        .with_highlighted_option_prefix(Styled::new("❯").with_style_sheet(accent))
        .with_scroll_up_prefix(Styled::new("↑").with_style_sheet(muted))
        .with_scroll_down_prefix(Styled::new("↓").with_style_sheet(muted))
        .with_selected_option(Some(focused))
        .with_option_index_prefix(IndexPrefix::None)
        .with_canceled_prompt_indicator(Styled::new("(cancelled)").with_style_sheet(muted));
    // `prompt` has no builder method — set the field directly.
    theme.prompt = StyleSheet::new().with_attr(Attributes::BOLD);
    theme
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

    /// Strip all SGR/ANSI escape sequences so tests can assert on visible text.
    fn plain(s: &str) -> String {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap())
            .replace_all(s, "")
            .into_owned()
    }

    #[test]
    fn header_box_rows_are_equal_width_and_padded() {
        let out = plain(&header_box(
            "devm8 v1.2.3",
            &[("project", "EISA"), ("user", "jafar@company.com")],
        ));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 4, "title border + 2 rows + bottom border");
        let widths: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
        assert!(widths.iter().all(|&w| w == widths[0]), "unequal: {out}");
        assert!(lines[0].starts_with("╭─ devm8 v1.2.3"));
        assert!(lines[0].ends_with('╮'));
        assert!(lines[1].starts_with("│ project  EISA"));
        assert!(lines[1].ends_with('│'));
        assert!(lines[3].starts_with("╰"));
        assert!(lines[3].ends_with('╯'));
    }

    #[test]
    fn header_box_truncates_long_values() {
        let long = "x".repeat(200);
        let out = plain(&header_box("devm8", &[("server", &long)]));
        for line in out.lines() {
            assert!(
                line.chars().count() <= 105,
                "line overflows terminal cap: {line}"
            );
        }
    }

    #[test]
    fn separator_spans_the_terminal_width() {
        assert_eq!(plain(&separator()).chars().count(), term_width());
        assert_eq!(plain(&separator()), "─".repeat(term_width()));
    }

    #[test]
    #[ignore] // visual preview: cargo test --bin devm8-client -- --ignored --nocapture visual_preview
    fn visual_preview() {
        println!();
        println!(
            "{}",
            header_box(
                "devm8 v0.1.0",
                &[
                    ("project", "EISA"),
                    ("user", "jafar@company.com"),
                    ("server", "myserver.tailnet-name.ts.net:7887"),
                ]
            )
        );
        println!(
            "{}",
            dim("Interactive session — Ctrl-C or Ctrl-D exits · Esc opens the free-text prompt")
        );
        println!();
        println!("{}", accent("❯ what's failing in the login flow?"));
        println!();
        print_markdown("## Analysis\n\nThe `login_handler` drops the **session cookie** on redirect.\n\n```bash\n$ cargo test login\nok. 3 passed\n```\n\n- point 1\n- point 2\n\n---\nsub-line");
        println!();
        println!("{}", separator());
        println!();
        println!("{}", accent("❯ and the branch situation?"));
        println!();
        println!("{}", dim("✻ Analyzing worktree…"));
        println!();
        println!(
            "{}",
            header_box(
                "devm8",
                &[
                    ("project", "a-very-long-project-key-name-here"),
                    ("server", &"x".repeat(120))
                ]
            )
        );
        println!();
        println!(
            "{}",
            dim("(the prompt theme applies to live inquire menus, not printable here)")
        );
    }

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
