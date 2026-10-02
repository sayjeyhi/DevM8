//! Full-screen terminal UI (alternate screen, ratatui) for interactive
//! sessions — `ask` without a question and `jira`. The look follows
//! opencode's TUI: a rounded header box, a borderless scrolling transcript,
//! a live status line, the pending choices as a selectable list above a
//! bordered input box, and a dim footer of key hints.
//!
//! The session loop drives the same SSE protocol as the linear renderer: a
//! spawned task consumes the event stream through the shared
//! [`EventRenderer`](render::EventRenderer) and forwards high-level updates
//! (markdown blocks, status, choices) over a channel, so streaming and user
//! input interleave in one select loop.
//!
//! The TUI only engages on real interactive terminals — piped output,
//! `--plain`, or `NO_COLOR` still get the linear renderer unchanged.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    EventStream, KeyCode, KeyEvent, KeyModifiers, MouseEventKind,
};
use futures_util::StreamExt;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};
use ratatui::DefaultTerminal;
use termimad::{CompositeKind, FmtLine, FmtText, MadSkin};

use devm8::api::protocol::{ActionRequest, AskEvent, AskRequest, AskStartRequest, ChoiceDto};

use super::render::{EventRenderer, EventSink};
use super::{config, sse, ui};
use config::Credentials;

// ---------------------------------------------------------------------------
// Session kinds / requests
// ---------------------------------------------------------------------------

/// Which flow the TUI session drives: `ask` (project-scoped REPL) or `jira`
/// (the menu, with no project context).
#[derive(Clone)]
pub enum SessionKind {
    Ask { project: String },
    Jira,
}

/// The next server request the session needs to make, derived from user input.
#[derive(Clone, Debug)]
enum Request {
    Start,
    Ask { question: String },
    Action { action: String },
}

impl Request {
    fn url_and_body(&self, creds: &Credentials, kind: &SessionKind) -> (String, serde_json::Value) {
        match self {
            Request::Start => match kind {
                SessionKind::Ask { project } => (
                    format!("{}/v1/ask/start", creds.server),
                    serde_json::to_value(AskStartRequest {
                        project: project.clone(),
                    })
                    .unwrap_or(serde_json::Value::Null),
                ),
                SessionKind::Jira => (
                    format!("{}/v1/jira/start", creds.server),
                    serde_json::json!({}),
                ),
            },
            Request::Ask { question } => {
                let project = match kind {
                    SessionKind::Ask { project } => Some(project.clone()),
                    SessionKind::Jira => None,
                };
                (
                    format!("{}/v1/ask", creds.server),
                    serde_json::to_value(AskRequest {
                        question: question.clone(),
                        project,
                    })
                    .unwrap_or(serde_json::Value::Null),
                )
            }
            Request::Action { action } => (
                format!("{}/v1/action", creds.server),
                serde_json::to_value(ActionRequest {
                    action: action.clone(),
                })
                .unwrap_or(serde_json::Value::Null),
            ),
        }
    }
}

fn url_host(server: &str) -> String {
    url::Url::parse(server)
        .ok()
        .and_then(|u| match (u.host_str(), u.port()) {
            (Some(h), Some(p)) => Some(format!("{h}:{p}")),
            (Some(h), None) => Some(h.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| server.to_string())
}

/// The header-box content: `devm8 ask v… / project / user / server`.
fn header_source(kind: &SessionKind, creds: &Credentials) -> (String, Vec<(String, String)>) {
    let (title, project) = match kind {
        SessionKind::Ask { project } => ("ask", project.clone()),
        SessionKind::Jira => ("jira", "—".to_string()),
    };
    let rows = vec![
        ("project".to_string(), project),
        ("user".to_string(), creds.email.clone()),
        ("server".to_string(), url_host(&creds.server)),
    ];
    // DEVM8_VERSION already carries its leading `v`.
    (format!("devm8 {title} {}", env!("DEVM8_VERSION")), rows)
}

// ---------------------------------------------------------------------------
// Stream → TUI updates
// ---------------------------------------------------------------------------

/// Updates flowing from the stream task into the TUI loop.
enum StreamUpdate {
    Markdown(String),
    Status(String),
    StatusEnd,
    Error(String),
    Choices(Vec<ChoiceDto>),
    /// The stream ended (server Done, connection drop, or request error).
    Finished,
}

/// `EventSink` forwarding rendered output to the TUI loop. Uses an unbounded
/// channel because `EventSink` is sync and is called from inside the async
/// stream task.
struct TuiSink {
    tx: tokio::sync::mpsc::UnboundedSender<StreamUpdate>,
}

impl EventSink for TuiSink {
    fn markdown(&mut self, text: &str) {
        let _ = self.tx.send(StreamUpdate::Markdown(text.to_string()));
    }
    fn status(&mut self, text: &str) {
        let _ = self.tx.send(StreamUpdate::Status(text.to_string()));
    }
    fn status_end(&mut self) {
        let _ = self.tx.send(StreamUpdate::StatusEnd);
    }
    fn error(&mut self, msg: &str) {
        let _ = self.tx.send(StreamUpdate::Error(msg.to_string()));
    }
    fn choice_list(&mut self, items: &[ChoiceDto]) {
        let _ = self.tx.send(StreamUpdate::Choices(items.to_vec()));
    }
}

/// Spawn the SSE consumption for one request. Renders events through the
/// shared [`EventRenderer`] (so cancel-only keyboards stay a live status) and
/// pushes updates into `tx`.
fn spawn_stream(
    client: &reqwest::Client,
    creds: &Credentials,
    kind: &SessionKind,
    request: Request,
    tx: tokio::sync::mpsc::UnboundedSender<StreamUpdate>,
) {
    let client = client.clone();
    let creds = creds.clone();
    let kind = kind.clone();
    tokio::spawn(async move {
        let (url, body) = request.url_and_body(&creds, &kind);
        let resp = match client.post(&url).json(&body).send().await {
            Ok(resp) => resp,
            Err(e) => {
                let _ = tx.send(StreamUpdate::Error(format!(
                    "request failed: {e:#} — check the connection and try again"
                )));
                let _ = tx.send(StreamUpdate::Finished);
                return;
            }
        };
        let resp = match resp.error_for_status() {
            Ok(resp) => resp,
            Err(e) => {
                let _ = tx.send(StreamUpdate::Error(format!("server error: {e}")));
                let _ = tx.send(StreamUpdate::Finished);
                return;
            }
        };

        let mut renderer = EventRenderer::new();
        let mut sink = TuiSink { tx: tx.clone() };
        let stream = sse::for_each_event(resp, |event: &AskEvent| {
            renderer.render(&mut sink, event);
        })
        .await;
        renderer.finish(&mut sink);
        if let Err(e) = stream {
            let _ = tx.send(StreamUpdate::Error(format!("connection lost: {e}")));
        }
        let _ = tx.send(StreamUpdate::Finished);
    });
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Role {
    User,
    Assistant,
    Error,
}

struct Message {
    role: Role,
    text: String,
}

/// The TUI state machine. Deliberately terminal-free so the input/choice
/// logic stays unit-testable.
struct App {
    header_title: String,
    header_rows: Vec<(String, String)>,
    header: ui::HeaderData,
    messages: Vec<Message>,
    lines: Vec<Line<'static>>,
    lines_width: u16,
    lines_dirty: bool,
    status: Option<(String, Instant)>,
    choices: Vec<ChoiceDto>,
    choice_selected: usize,
    choices_visible: bool,
    input: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    history_idx: Option<usize>,
    scroll_up: usize,
    quit: bool,
}

impl App {
    fn new(kind: &SessionKind, creds: &Credentials) -> Self {
        let (header_title, header_rows) = header_source(kind, creds);
        let header = ui::header_data(&header_title, &rows_slice(&header_rows), 80);
        Self {
            header_title,
            header_rows,
            header,
            messages: Vec::new(),
            lines: Vec::new(),
            lines_width: 0,
            lines_dirty: true,
            status: None,
            choices: Vec::new(),
            choice_selected: 0,
            choices_visible: false,
            input: Vec::new(),
            cursor: 0,
            history: Vec::new(),
            history_idx: None,
            scroll_up: 0,
            quit: false,
        }
    }

    fn push_message(&mut self, role: Role, text: impl Into<String>) {
        self.messages.push(Message {
            role,
            text: text.into(),
        });
        self.lines_dirty = true;
    }

    fn apply_update(&mut self, update: StreamUpdate) -> bool {
        match update {
            StreamUpdate::Markdown(text) => self.push_message(Role::Assistant, text),
            StreamUpdate::Status(text) => {
                let text = ui::one_line(&text);
                match &mut self.status {
                    Some((current, _)) => *current = text,
                    None => self.status = Some((text, Instant::now())),
                }
            }
            StreamUpdate::StatusEnd => self.status = None,
            StreamUpdate::Error(msg) => self.push_message(Role::Error, msg),
            StreamUpdate::Choices(items) => {
                self.choices = items;
                self.choice_selected = 0;
                self.choices_visible = !self.choices.is_empty();
            }
            StreamUpdate::Finished => return true,
        }
        false
    }

    // -- input editing ------------------------------------------------------

    fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }

    fn input_text(&self) -> String {
        self.input.iter().collect()
    }

    fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.history_idx = None;
    }

    // -- history ------------------------------------------------------------

    fn history_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_idx {
            None => self.history.len() - 1,
            Some(0) => return,
            Some(i) => i - 1,
        };
        self.set_from_history(idx);
    }

    fn history_next(&mut self) {
        let Some(idx) = self.history_idx else {
            return;
        };
        if idx + 1 >= self.history.len() {
            self.clear_input();
        } else {
            self.set_from_history(idx + 1);
        }
    }

    fn set_from_history(&mut self, idx: usize) {
        self.history_idx = Some(idx);
        self.input = self.history[idx].chars().collect();
        self.cursor = self.input.len();
    }

    // -- events -------------------------------------------------------------

    /// Handle a key press; `Enter` may produce the next server request.
    fn on_key(&mut self, key: KeyEvent) -> Option<Request> {
        if key.kind == crossterm::event::KeyEventKind::Release {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match (key.code, ctrl) {
            (KeyCode::Char('c'), true) | (KeyCode::Char('d'), true) => self.quit = true,
            (KeyCode::Char('u'), true) => self.clear_input(),
            (KeyCode::Char('a'), true) => self.cursor = 0,
            (KeyCode::Char('e'), true) => self.cursor = self.input.len(),
            (KeyCode::Char(c), false) => self.insert_char(c),
            (KeyCode::Backspace, _) => self.backspace(),
            (KeyCode::Delete, _) => {
                if self.cursor < self.input.len() {
                    self.input.remove(self.cursor);
                }
            }
            (KeyCode::Left, _) => self.cursor = self.cursor.saturating_sub(1),
            (KeyCode::Right, _) => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                }
            }
            (KeyCode::Home, _) => self.cursor = 0,
            (KeyCode::End, _) => self.cursor = self.input.len(),
            (KeyCode::Up, _) => {
                if self.choices_visible && !self.choices.is_empty() {
                    self.choice_selected = self.choice_selected.saturating_sub(1);
                } else {
                    self.history_previous();
                }
            }
            (KeyCode::Down, _) => {
                if self.choices_visible && !self.choices.is_empty() {
                    let max = self.choices.len().saturating_sub(1);
                    self.choice_selected = (self.choice_selected + 1).min(max);
                } else {
                    self.history_next();
                }
            }
            (KeyCode::PageUp, _) => self.scroll_up += 10,
            (KeyCode::PageDown, _) => self.scroll_up = self.scroll_up.saturating_sub(10),
            (KeyCode::Enter, _) => return self.submit(),
            (KeyCode::Esc, _) => {
                if !self.choices.is_empty() {
                    self.choices_visible = !self.choices_visible;
                } else {
                    self.clear_input();
                }
            }
            _ => {}
        }
        None
    }

    fn on_mouse_scroll(&mut self, down: bool) {
        if down {
            self.scroll_up = self.scroll_up.saturating_sub(3);
        } else {
            self.scroll_up += 3;
        }
    }

    /// Enter: non-empty input sends a question; otherwise a visible choice
    /// list sends the highlighted action. Whatever was sent is echoed into
    /// the transcript and recorded in the input history.
    fn submit(&mut self) -> Option<Request> {
        let text = self.input_text();
        if !text.trim().is_empty() {
            self.clear_input();
            self.history.push(text.clone());
            self.scroll_up = 0; // sending means we want to follow the reply
            self.push_message(Role::User, text.clone());
            return Some(Request::Ask { question: text });
        }
        if self.choices_visible && !self.choices.is_empty() {
            let choice = self.choices[self.choice_selected.min(self.choices.len() - 1)].clone();
            self.scroll_up = 0;
            self.push_message(Role::User, choice.label.clone());
            self.choices.clear();
            self.choices_visible = false;
            return Some(Request::Action {
                action: choice.data,
            });
        }
        None
    }

    // -- rendering ----------------------------------------------------------

    /// Ensure the line cache matches the current width and content. The
    /// header box is recomputed here too, so it tracks resizes.
    fn ensure_lines(&mut self, width: u16) {
        if self.lines_width != width {
            self.header = ui::header_data(
                &self.header_title,
                &rows_slice(&self.header_rows),
                width as usize,
            );
            self.lines_width = width;
            self.lines_dirty = true;
        }
        if self.lines_dirty {
            let skin = ui::markdown_skin();
            let mut lines = Vec::new();
            for msg in &self.messages {
                match msg.role {
                    Role::Assistant => lines.extend(markdown_lines(&skin, &msg.text, width)),
                    Role::User => lines.extend(user_lines(&skin, &msg.text, width)),
                    Role::Error => lines.extend(note_lines(
                        &skin,
                        &msg.text,
                        width,
                        Style::new().fg(Color::Red),
                    )),
                }
                lines.push(Line::default());
            }
            self.lines = lines;
            self.lines_dirty = false;
        }
    }

    fn total_lines(&self) -> usize {
        self.lines.len()
    }
}

fn rows_slice(rows: &[(String, String)]) -> Vec<(&str, &str)> {
    rows.iter().map(|(l, v)| (l.as_str(), v.as_str())).collect()
}

// ---------------------------------------------------------------------------
// Markdown → ratatui lines (termimad parses/wraps, we style)
// ---------------------------------------------------------------------------

const ACCENT: Color = Color::LightCyan;
const MUTED: Color = Color::DarkGray;

/// Render one markdown block into styled lines wrapped at `width` cells.
/// termimad does the parsing and hard-wrapping (same engine as the linear
/// renderer); we map its line kinds and compound flags to ratatui styles.
fn markdown_lines(skin: &MadSkin, md: &str, width: u16) -> Vec<Line<'static>> {
    let width = width.max(10) as usize;
    let text = FmtText::from(skin, md, Some(width));
    let mut out = Vec::new();
    for line in text.lines {
        match line {
            FmtLine::Normal(composite) => {
                let (indent, prefix, prefix_style, base) = kind_style(composite.kind);
                let mut spans: Vec<Span> = Vec::new();
                if !indent.is_empty() {
                    spans.push(Span::raw(indent));
                }
                if !prefix.is_empty() {
                    spans.push(Span::styled(prefix, prefix_style));
                }
                for c in &composite.compounds {
                    let mut style = base;
                    if c.bold {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if c.italic {
                        style = style.add_modifier(Modifier::ITALIC);
                    }
                    if c.code {
                        style = style.fg(ACCENT);
                    }
                    if c.strikeout {
                        style = style.add_modifier(Modifier::CROSSED_OUT);
                    }
                    spans.push(Span::styled(c.src.to_string(), style));
                }
                out.push(Line::from(spans));
            }
            FmtLine::TableRow(row) => {
                let mut spans = Vec::new();
                for (i, cell) in row.cells.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::styled("  ", Style::new().fg(MUTED)));
                    }
                    for c in &cell.compounds {
                        let mut style = Style::new();
                        if c.bold {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        if c.code {
                            style = style.fg(ACCENT);
                        }
                        spans.push(Span::styled(c.src.to_string(), style));
                    }
                }
                out.push(Line::from(spans));
            }
            FmtLine::TableRule(rule) => {
                // rule widths are known after termimad's table fix-up
                let total: usize = rule.widths.iter().sum();
                let width = total + rule.widths.len().saturating_sub(1) * 2;
                out.push(Line::from(Span::styled(
                    "─".repeat(width.max(1)),
                    Style::new().fg(MUTED),
                )));
            }
            FmtLine::HorizontalRule => {
                out.push(Line::from(Span::styled(
                    "─".repeat(width),
                    Style::new().fg(MUTED),
                )));
            }
        }
    }
    out
}

/// Indent, prefix and base style for a composite kind.
fn kind_style(kind: CompositeKind) -> (String, String, Style, Style) {
    match kind {
        CompositeKind::Code => (
            String::new(),
            String::new(),
            Style::new(),
            Style::new().fg(MUTED),
        ),
        CompositeKind::Header(_) => (
            String::new(),
            String::new(),
            Style::new(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        CompositeKind::Quote => (
            String::new(),
            "▌ ".to_string(),
            Style::new().fg(MUTED),
            Style::new().fg(MUTED),
        ),
        CompositeKind::ListItem(level) => (
            String::new(),
            format!("{}• ", "  ".repeat(level as usize)),
            Style::new().fg(ACCENT),
            Style::new(),
        ),
        CompositeKind::ListItemFollowUp(level) => (
            "  ".repeat(level as usize + 1),
            String::new(),
            Style::new(),
            Style::new(),
        ),
        CompositeKind::OrderedListItem { level, index } => (
            String::new(),
            format!("{}{}. ", "  ".repeat(level as usize), index),
            Style::new().fg(ACCENT),
            Style::new(),
        ),
        CompositeKind::OrderedListItemFollowUp { level, .. } => (
            "  ".repeat(level as usize + 1),
            String::new(),
            Style::new(),
            Style::new(),
        ),
        CompositeKind::Paragraph => (String::new(), String::new(), Style::new(), Style::new()),
    }
}

/// A sent message: `❯ text` in the accent color, wrapped, continuation lines
/// indented under the marker.
fn user_lines(skin: &MadSkin, text: &str, width: u16) -> Vec<Line<'static>> {
    let width = (width.max(10) as usize).saturating_sub(2);
    let wrapped = FmtText::raw_str(skin, text, Some(width));
    let mut out = Vec::new();
    for (i, line) in wrapped.lines.iter().enumerate() {
        let FmtLine::Normal(composite) = line else {
            continue;
        };
        let body: String = composite.compounds.iter().map(|c| c.src).collect();
        let content = if i == 0 {
            format!("❯ {body}")
        } else {
            format!("  {body}")
        };
        out.push(Line::from(Span::styled(content, Style::new().fg(ACCENT))));
    }
    if out.is_empty() {
        out.push(Line::from(Span::styled(
            format!("❯ {text}"),
            Style::new().fg(ACCENT),
        )));
    }
    out
}

/// A plain-text note (dim) or error (red), wrapped at `width`.
fn note_lines(skin: &MadSkin, text: &str, width: u16, style: Style) -> Vec<Line<'static>> {
    let width = width.max(10) as usize;
    let wrapped = FmtText::raw_str(skin, text, Some(width));
    wrapped
        .lines
        .iter()
        .map(|line| match line {
            FmtLine::Normal(composite) => {
                let body: String = composite.compounds.iter().map(|c| c.src).collect();
                Line::from(Span::styled(body, style))
            }
            _ => Line::default(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Layout / draw
// ---------------------------------------------------------------------------

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn header_lines(data: &ui::HeaderData) -> Vec<Line<'static>> {
    let label_width = data
        .rows
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0);
    let dashes = data
        .content_width
        .saturating_sub(data.title.chars().count())
        .saturating_sub(1);
    let mut lines = vec![Line::from(vec![
        Span::styled("╭─ ", Style::new().fg(MUTED)),
        Span::styled(
            data.title.clone(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{}╮", "─".repeat(dashes)), Style::new().fg(MUTED)),
    ])];
    for (label, value) in &data.rows {
        let pad = data
            .content_width
            .saturating_sub(label_width)
            .saturating_sub(value.chars().count())
            .saturating_sub(2);
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::new().fg(MUTED)),
            Span::styled(format!("{:<label_width$}", label), Style::new().fg(MUTED)),
            Span::raw("  "),
            Span::raw(value.clone()),
            Span::raw(" ".repeat(pad)),
            Span::styled(" │", Style::new().fg(MUTED)),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("╰", Style::new().fg(MUTED)),
        Span::styled("─".repeat(data.content_width + 2), Style::new().fg(MUTED)),
        Span::styled("╯", Style::new().fg(MUTED)),
    ]));
    lines
}

fn draw(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    terminal
        .draw(|frame| {
            let area = frame.area();

            // Fixed rows: header box, optional status, optional choices,
            // input box, footer. Everything in between is the transcript.
            let choice_rows = if app.choices_visible && !app.choices.is_empty() {
                app.choices.len() as u16 + 2 // items + top/bottom border
            } else {
                0
            };
            let mut constraints = vec![
                Constraint::Length(4), // header box + trailing blank line
                Constraint::Min(3),    // transcript
            ];
            if app.status.is_some() {
                constraints.push(Constraint::Length(1));
            }
            if choice_rows > 0 {
                constraints.push(Constraint::Length(choice_rows));
            }
            constraints.push(Constraint::Length(3)); // input box
            constraints.push(Constraint::Length(1)); // footer hints
            let chunks = Layout::vertical(constraints).split(area);

            let transcript_area = chunks[1];
            let mut next = 2;
            let status_area = if app.status.is_some() {
                let a = chunks[next];
                next += 1;
                Some(a)
            } else {
                None
            };
            let choices_area = if choice_rows > 0 {
                let a = chunks[next];
                next += 1;
                Some(a)
            } else {
                None
            };
            let input_area = chunks[next];
            let footer_area = chunks[next + 1];

            // Header
            let mut header = header_lines(&app.header);
            header.push(Line::default());
            frame.render_widget(Paragraph::new(header), chunks[0]);

            // Transcript — borderless, scrolls, sticks to the bottom.
            let width = transcript_area.width;
            app.ensure_lines(width);
            let height = transcript_area.height as usize;
            let max_offset = app.total_lines().saturating_sub(height);
            let offset = max_offset.saturating_sub(app.scroll_up);
            frame.render_widget(
                Paragraph::new(app.lines.clone()).scroll((offset as u16, 0)),
                transcript_area,
            );

            // Status (spinner) line
            if let (Some(sarea), Some((text, started))) = (status_area, &app.status) {
                let frame_idx =
                    (started.elapsed().as_millis() / 90) as usize % SPINNER_FRAMES.len();
                let secs = started.elapsed().as_secs();
                let elapsed = if secs >= 3 {
                    format!(" ({secs}s)")
                } else {
                    String::new()
                };
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(SPINNER_FRAMES[frame_idx], Style::new().fg(ACCENT)),
                        Span::styled(format!(" {text}{elapsed}"), Style::new().dim()),
                    ])),
                    sarea,
                );
            }

            // Choices list
            if let (Some(carea), false) = (choices_area, app.choices.is_empty()) {
                let items: Vec<ListItem> = app
                    .choices
                    .iter()
                    .map(|c| ListItem::new(Line::from(Span::raw(c.label.clone()))))
                    .collect();
                let list = List::new(items)
                    .block(
                        Block::bordered()
                            .border_set(border::ROUNDED)
                            .border_style(Style::new().fg(MUTED))
                            .title(Span::styled(" actions ", Style::new().fg(MUTED))),
                    )
                    .highlight_symbol("❯ ")
                    .highlight_style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD));
                let mut state = ListState::default()
                    .with_selected(Some(app.choice_selected.min(app.choices.len() - 1)));
                frame.render_stateful_widget(list, carea, &mut state);
            }

            // Input box with a reverse-video cursor
            let cursor_char = app.input.get(app.cursor).copied().unwrap_or(' ');
            let mut spans = Vec::new();
            let before: String = app.input[..app.cursor].iter().collect();
            if !before.is_empty() {
                spans.push(Span::raw(before));
            }
            spans.push(Span::styled(
                cursor_char.to_string(),
                Style::new().add_modifier(Modifier::REVERSED),
            ));
            if app.cursor < app.input.len() {
                let after: String = app.input[app.cursor + 1..].iter().collect();
                if !after.is_empty() {
                    spans.push(Span::raw(after));
                }
            }
            let input_block = Block::bordered()
                .border_set(border::ROUNDED)
                .border_style(Style::new().fg(MUTED))
                .title(Span::styled(" message ", Style::new().fg(MUTED)));
            frame.render_widget(
                Paragraph::new(Line::from(spans)).block(input_block),
                input_area,
            );

            // Footer hints
            let hints = if !app.choices.is_empty() && app.choices_visible {
                "↑/↓ choose · ↵ send · esc free text · pgup/pgdn scroll · ctrl-c exit"
            } else {
                "↵ send · ↑ history · pgup/pgdn scroll · ctrl-c exit"
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(hints, Style::new().fg(MUTED)))),
                footer_area,
            );
        })
        .map_err(|e| anyhow::anyhow!("terminal draw failed: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Session loop
// ---------------------------------------------------------------------------

/// Terminal restore on every exit path — including unwinds from panics
/// (ratatui::init already installed a panic hook; this covers the rest).
struct TuiCleanup;

impl Drop for TuiCleanup {
    fn drop(&mut self) {
        let _ = crossterm::execute!(
            std::io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture
        );
        ratatui::restore();
    }
}

/// Run a full-screen session until the user quits (Ctrl-C / Ctrl-D).
pub async fn run_session(
    client: reqwest::Client,
    creds: Credentials,
    kind: SessionKind,
) -> Result<()> {
    let mut terminal = ratatui::init();
    let _cleanup = TuiCleanup;
    crossterm::execute!(std::io::stdout(), EnableBracketedPaste, EnableMouseCapture)
        .context("failed to enable terminal features")?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    spawn_stream(&client, &creds, &kind, Request::Start, tx.clone());

    let mut app = App::new(&kind, &creds);
    let mut streaming = true;
    let mut queued: Option<Request> = None;
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(120));

    let result: Result<()> = loop {
        let mut key_request: Option<Request> = None;
        let mut finished = false;

        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => key_request = app.on_key(key),
                    Some(Ok(Event::Mouse(mouse))) => {
                        app.on_mouse_scroll(mouse.kind == MouseEventKind::ScrollDown);
                    }
                    Some(Ok(Event::Paste(text))) => {
                        for c in text.chars() {
                            app.insert_char(c);
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break Ok(()),
                }
            }
            update = rx.recv(), if streaming => {
                match update {
                    Some(u) => {
                        finished = app.apply_update(u);
                        if finished {
                            streaming = false;
                        }
                    }
                    None => {
                        finished = true;
                        streaming = false;
                    }
                }
            }
            _ = tick.tick() => {}
        }

        if app.quit {
            break Ok(());
        }

        // A submit while streaming queues one follow-up; otherwise the next
        // stream starts immediately. A finished stream flushes the queue.
        if let Some(request) = key_request {
            if streaming {
                queued = Some(request);
            } else {
                streaming = true;
                spawn_stream(&client, &creds, &kind, request, tx.clone());
            }
        } else if finished {
            if let Some(request) = queued.take() {
                streaming = true;
                spawn_stream(&client, &creds, &kind, request, tx.clone());
            }
        }

        draw(&mut terminal, &mut app)?;
    };

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;

    fn app() -> App {
        App::new(
            &SessionKind::Ask {
                project: "EISA".to_string(),
            },
            &Credentials {
                server: "https://myserver.tailnet-name.ts.net:7887".to_string(),
                email: "jafar@company.com".to_string(),
                token: "tok".to_string(),
            },
        )
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: crossterm::event::KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.insert_char(c);
        }
    }

    #[test]
    fn header_rows_align_and_carry_context() {
        let a = app();
        assert!(a.header_title.starts_with("devm8 ask v"));
        let labels: Vec<&str> = a.header.rows.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["project", "user", "server"]);
        assert_eq!(a.header.rows[0].1, "EISA");
        assert!(a.header.rows[2].1.contains(":7887"));
    }

    #[test]
    fn typing_and_enter_sends_a_question() {
        let mut a = app();
        type_text(&mut a, "why is the build red?");
        assert_eq!(a.input_text(), "why is the build red?");
        let request = a.on_key(key(KeyCode::Enter));
        assert!(matches!(request, Some(Request::Ask { .. })));
        assert!(a.input.is_empty());
        // the sent question is echoed into the transcript and kept as history
        assert_eq!(a.messages.len(), 1);
        assert_eq!(a.messages[0].role, Role::User);
        assert_eq!(
            a.history.last().map(String::as_str),
            Some("why is the build red?")
        );
    }

    #[test]
    fn cursor_editing_works() {
        let mut a = app();
        type_text(&mut a, "helo");
        a.on_key(key(KeyCode::Left));
        a.on_key(key(KeyCode::Left)); // cursor between 'e' and 'l'
        a.insert_char('l');
        assert_eq!(a.input_text(), "hello");
        a.on_key(key(KeyCode::Backspace)); // removes the char before the cursor
        assert_eq!(a.input_text(), "helo");
        a.on_key(key(KeyCode::End));
        a.insert_char('o');
        assert_eq!(a.input_text(), "heloo");
        a.on_key(key(KeyCode::Home));
        a.insert_char('x'); // "xheloo", cursor after the x
        a.on_key(key(KeyCode::Left)); // back onto the x
        a.on_key(key(KeyCode::Delete)); // removes the char at the cursor
        assert_eq!(a.input_text(), "heloo");
    }

    #[test]
    fn empty_enter_with_choices_picks_the_highlighted_action() {
        let mut a = app();
        a.apply_update(StreamUpdate::Choices(vec![
            ChoiceDto {
                label: "Pull latest (0 behind)".into(),
                data: "ask:pull_latest".into(),
            },
            ChoiceDto {
                label: "New branch".into(),
                data: "ask:branch".into(),
            },
        ]));
        assert!(a.choices_visible);

        a.on_key(key(KeyCode::Down)); // select "New branch"
        let request = a.on_key(key(KeyCode::Enter));
        match request {
            Some(Request::Action { action }) => assert_eq!(action, "ask:branch"),
            other => panic!("expected action, got {other:?}"),
        }
        assert!(a.choices.is_empty());
        assert_eq!(a.messages[0].text, "New branch");
    }

    #[test]
    fn esc_toggles_choice_list_for_free_text() {
        let mut a = app();
        a.apply_update(StreamUpdate::Choices(vec![ChoiceDto {
            label: "OpenCode".into(),
            data: "ask:opencode".into(),
        }]));
        a.on_key(key(KeyCode::Esc));
        assert!(!a.choices_visible);
        a.on_key(key(KeyCode::Esc));
        assert!(a.choices_visible);

        // free text wins over the visible list
        type_text(&mut a, "hi again");
        let request = a.on_key(key(KeyCode::Enter));
        assert!(matches!(request, Some(Request::Ask { .. })));
    }

    #[test]
    fn empty_enter_without_choices_is_a_noop() {
        let mut a = app();
        assert!(a.on_key(key(KeyCode::Enter)).is_none());
        assert!(a.messages.is_empty());
    }

    #[test]
    fn ctrl_c_quits() {
        let mut a = app();
        a.on_key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            kind: crossterm::event::KeyEventKind::Press,
            state: KeyEventState::empty(),
        });
        assert!(a.quit);
    }

    #[test]
    fn history_navigates_sent_questions() {
        let mut a = app();
        type_text(&mut a, "first");
        a.on_key(key(KeyCode::Enter));
        type_text(&mut a, "second");
        a.on_key(key(KeyCode::Enter));
        assert!(a.input.is_empty());

        a.on_key(key(KeyCode::Up));
        assert_eq!(a.input_text(), "second");
        a.on_key(key(KeyCode::Up));
        assert_eq!(a.input_text(), "first");
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.input_text(), "second");
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.input_text(), "");
    }

    #[test]
    fn status_updates_replace_in_place_and_clear() {
        let mut a = app();
        assert!(!a.apply_update(StreamUpdate::Status("Thinking...".into())));
        assert_eq!(a.status.as_ref().unwrap().0, "Thinking...");
        a.apply_update(StreamUpdate::Status("Analyzing <b>PROJ-1</b>".into()));
        assert_eq!(a.status.as_ref().unwrap().0, "Analyzing PROJ-1");
        assert!(!a.apply_update(StreamUpdate::StatusEnd));
        assert!(a.status.is_none());
        // Finished marks the stream as done for the session loop
        assert!(a.apply_update(StreamUpdate::Finished));
    }

    #[test]
    fn markdown_lines_style_headers_code_and_lists() {
        let skin = ui::markdown_skin();
        let lines = markdown_lines(
            &skin,
            "# Title\n\ntext with `code`\n\n- bullet\n\n```bash\n$ echo\n```",
            80,
        );
        let plain: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        let title = lines
            .iter()
            .zip(plain.iter())
            .find(|(_, p)| p.as_str() == "Title")
            .unwrap()
            .0;
        assert!(
            title
                .spans
                .iter()
                .all(|s| s.style.add_modifier.contains(Modifier::BOLD)),
            "header must be bold"
        );

        let code_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.as_ref() == "code")
            .expect("inline code span exists");
        assert_eq!(code_span.style.fg, Some(ACCENT), "inline code is accented");

        let bullet = plain.iter().find(|p| p.contains("• bullet")).unwrap();
        assert!(bullet.contains("• "), "bullets get a marker: {bullet}");

        let block = lines
            .iter()
            .zip(plain.iter())
            .find(|(_, p)| p.contains("$ echo"))
            .unwrap()
            .0;
        assert_eq!(
            block.spans[0].style.fg,
            Some(MUTED),
            "code blocks are muted"
        );
    }

    #[test]
    fn markdown_wraps_at_width() {
        let skin = ui::markdown_skin();
        let lines = markdown_lines(&skin, &"word ".repeat(60), 40);
        assert!(lines.len() > 1, "long paragraph wraps");
        for line in &lines {
            let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(width <= 40, "line overflows wrap width: {width}");
        }
    }

    #[test]
    fn user_lines_wrap_with_marker_and_indent() {
        let skin = ui::markdown_skin();
        let lines = user_lines(&skin, &"lorem ipsum ".repeat(30), 40);
        assert!(lines.len() > 1);
        let first = lines[0]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        let cont = lines[1]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        assert!(first.starts_with("❯ "), "first line carries the marker");
        assert!(cont.starts_with("  "), "continuation lines are indented");
        assert!(lines
            .iter()
            .all(|l| l.spans.iter().all(|s| s.style.fg == Some(ACCENT))));
    }
}
