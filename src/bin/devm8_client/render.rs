//! Event-stream renderer: turns the SSE events from `/v1/ask`, `/v1/solve`,
//! `/v1/pr-review` and `/v1/jira/start` into terminal output.
//!
//! The server drives every flow the way it drives Telegram: it first sends a
//! status message ("Thinking…", "Analyzing PROJ-1…") with a Cancel button,
//! then repeatedly *edits* that message with progress previews, and finally
//! *edits* it into the actual answer (or "Cancelled." / an error). Printing
//! each of those edits verbatim — as the client used to — floods the terminal
//! with repeated progress snapshots and "[1] Cancel" lists.
//!
//! Instead, a message that currently offers nothing but a Cancel button is
//! treated as a *live status*: it renders as a spinner line that gets updated
//! in place, and only the final edit (the one that stops being cancel-only)
//! is printed as a real message.

use devm8::api::protocol::{AskEvent, ChoiceDto, KeyboardDto};

use super::ui;

/// Flattened choices from the last event that carried a keyboard, kept so a
/// follow-up selection can be posted to `/v1/action`.
pub struct PendingChoices {
    pub items: Vec<ChoiceDto>,
}

impl PendingChoices {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Where rendered output goes. Implemented by [`TerminalSink`]; a recording
/// implementation drives the unit tests below.
pub trait EventSink {
    /// A full message body (markdown).
    fn markdown(&mut self, text: &str);
    /// A live status update ("Thinking…", progress preview) — spinner text.
    fn status(&mut self, text: &str);
    /// The live status is over — clear it.
    fn status_end(&mut self);
    fn error(&mut self, msg: &str);
    /// The numbered choice list. The interactive REPL passes `list_choices:
    /// false` because the arrow-key select replaces it.
    fn choice_list(&mut self, items: &[ChoiceDto]);
}

/// [`EventSink`] writing to the real terminal.
pub struct TerminalSink {
    spinner: Option<ui::Spinner>,
    /// Whether choice keyboards are printed as a numbered list. True for
    /// one-shot/piped runs; the interactive REPL shows an arrow-key select
    /// instead, so the list would be redundant.
    list_choices: bool,
}

impl TerminalSink {
    pub fn new(list_choices: bool) -> Self {
        Self {
            spinner: None,
            list_choices,
        }
    }

    /// Whether the next stream's choice keyboards are printed as a numbered
    /// list — one-shot runs want the list; the interactive REPL shows an
    /// arrow-key select instead.
    pub fn set_list_choices(&mut self, list_choices: bool) {
        self.list_choices = list_choices;
    }
}

impl EventSink for TerminalSink {
    fn markdown(&mut self, text: &str) {
        ui::print_markdown(text);
    }

    fn status(&mut self, text: &str) {
        let line = ui::one_line(text);
        match &self.spinner {
            Some(spinner) => spinner.update(&line),
            None => self.spinner = Some(ui::Spinner::start(&line)),
        }
    }

    fn status_end(&mut self) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish();
        }
    }

    fn error(&mut self, msg: &str) {
        eprintln!("{} {}", ui::err_red(ui::CROSS), msg);
    }

    fn choice_list(&mut self, items: &[ChoiceDto]) {
        if !self.list_choices {
            return;
        }
        for (i, c) in items.iter().enumerate() {
            println!("{} {}", ui::dim(&format!(" {:2}.", i + 1)), c.label);
        }
    }
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

fn flattened(k: &KeyboardDto) -> Vec<&ChoiceDto> {
    k.iter().flatten().collect()
}

/// A keyboard that offers nothing but Cancel buttons — i.e. a "work in
/// progress" message rather than a real choice for the user (ask:cancel,
/// solve:cancel, prreview:cancel, …).
fn is_cancel_only(k: &KeyboardDto) -> bool {
    let items = flattened(k);
    !items.is_empty() && items.iter().all(|c| c.data.ends_with(":cancel"))
}

pub struct EventRenderer {
    /// Id of the message currently acting as the live status line, if any.
    status_id: Option<String>,
}

impl EventRenderer {
    pub fn new() -> Self {
        Self { status_id: None }
    }

    /// Render one event. Returns the flattened choices when the event ends
    /// with a real (non-cancel-only) keyboard.
    pub fn render(&mut self, sink: &mut dyn EventSink, event: &AskEvent) -> Option<PendingChoices> {
        match event {
            AskEvent::Text { text, .. } => {
                self.end_status(sink);
                sink.markdown(&ui::strip_html(text));
                None
            }
            AskEvent::Keyboard { id, text, choices } => {
                if is_cancel_only(choices) {
                    // "Thinking…" / "Analyzing …" — a live status, not a message.
                    self.status_id = Some(id.clone());
                    sink.status(text);
                    return None;
                }
                self.end_status(sink);
                sink.markdown(&ui::strip_html(text));
                self.emit_choices(sink, choices)
            }
            AskEvent::EditText { text, .. } => {
                // An edit of the status message without any keyboard (e.g.
                // "Cancelled.") ends it; an edit of any other message is just
                // its new content. Either way: stop the spinner, print.
                self.end_status(sink);
                sink.markdown(&ui::strip_html(text));
                None
            }
            AskEvent::EditWithKeyboard { id, text, choices } => {
                if self.is_status(id) && is_cancel_only(choices) {
                    // Progress preview while still cancellable — update in place.
                    sink.status(text);
                    return None;
                }
                self.end_status(sink);
                sink.markdown(&ui::strip_html(text));
                self.emit_choices(sink, choices)
            }
            AskEvent::EditKeyboard { id, choices } => {
                if self.is_status(id) && is_cancel_only(choices) {
                    return None; // buttons-only refresh of the status message
                }
                self.end_status(sink);
                self.emit_choices(sink, choices)
            }
            AskEvent::Delete { id } => {
                if self.is_status(id) {
                    self.status_id = None;
                    sink.status_end();
                }
                None
            }
            AskEvent::Done => {
                self.end_status(sink);
                None
            }
            AskEvent::Error { message } => {
                self.end_status(sink);
                sink.error(message);
                None
            }
        }
    }

    /// The stream ended without a terminal event (e.g. the connection
    /// dropped) — make sure no spinner line is left behind.
    pub fn finish(&mut self, sink: &mut dyn EventSink) {
        self.end_status(sink);
    }

    fn is_status(&self, id: &str) -> bool {
        self.status_id.as_deref() == Some(id)
    }

    fn end_status(&mut self, sink: &mut dyn EventSink) {
        if self.status_id.take().is_some() {
            sink.status_end();
        }
    }

    fn emit_choices(
        &mut self,
        sink: &mut dyn EventSink,
        keyboard: &KeyboardDto,
    ) -> Option<PendingChoices> {
        let items: Vec<ChoiceDto> = flattened(keyboard).into_iter().cloned().collect();
        if items.is_empty() {
            return None;
        }
        sink.choice_list(&items);
        Some(PendingChoices { items })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum Entry {
        Markdown(String),
        Status(String),
        StatusEnd,
        Error(String),
        Choices(Vec<String>),
    }

    #[derive(Default)]
    struct Recorder {
        entries: Vec<Entry>,
    }

    impl EventSink for Recorder {
        fn markdown(&mut self, text: &str) {
            self.entries.push(Entry::Markdown(text.to_string()));
        }
        fn status(&mut self, text: &str) {
            self.entries.push(Entry::Status(text.to_string()));
        }
        fn status_end(&mut self) {
            self.entries.push(Entry::StatusEnd);
        }
        fn error(&mut self, msg: &str) {
            self.entries.push(Entry::Error(msg.to_string()));
        }
        fn choice_list(&mut self, items: &[ChoiceDto]) {
            self.entries.push(Entry::Choices(
                items.iter().map(|c| c.label.clone()).collect(),
            ));
        }
    }

    fn choice(label: &str, data: &str) -> ChoiceDto {
        ChoiceDto {
            label: label.to_string(),
            data: data.to_string(),
        }
    }

    fn cancel_kb() -> KeyboardDto {
        vec![vec![choice("Cancel", "ask:cancel")]]
    }

    /// Drives the renderer over a sequence of events, returning the recorder
    /// and the last choices seen.
    fn run(events: &[AskEvent]) -> (Recorder, Option<PendingChoices>) {
        let mut sink = Recorder::default();
        let mut renderer = EventRenderer::new();
        let mut last = None;
        for event in events {
            if let Some(pc) = renderer.render(&mut sink, event) {
                last = Some(pc);
            }
        }
        renderer.finish(&mut sink);
        (sink, last)
    }

    #[test]
    fn ask_flow_renders_status_as_spinner_not_messages() {
        let session_kb = vec![vec![
            choice("Ask a follow-up", "ask:followup"),
            choice("Open PR", "ask:openpr"),
        ]];
        let events = vec![
            AskEvent::Keyboard {
                id: "m0".into(),
                text: "Thinking...".into(),
                choices: cancel_kb(),
            },
            AskEvent::EditWithKeyboard {
                id: "m0".into(),
                text: "<pre>$ cargo build\n   Compiling devm8…</pre>".into(),
                choices: cancel_kb(),
            },
            AskEvent::EditWithKeyboard {
                id: "m0".into(),
                text: "Here is the **answer**".into(),
                choices: session_kb,
            },
            AskEvent::Done,
        ];
        let (sink, choices) = run(&events);

        assert_eq!(
            sink.entries,
            vec![
                Entry::Status("Thinking...".into()),
                Entry::Status("<pre>$ cargo build\n   Compiling devm8…</pre>".into()),
                Entry::StatusEnd,
                Entry::Markdown("Here is the **answer**".into()),
                Entry::Choices(vec!["Ask a follow-up".into(), "Open PR".into()]),
            ]
        );
        let pc = choices.expect("final keyboard returned as choices");
        assert_eq!(pc.items.len(), 2);
        assert_eq!(pc.items[0].data, "ask:followup");
    }

    #[test]
    fn cancel_flow_ends_status_and_prints_short_line() {
        let events = vec![
            AskEvent::Keyboard {
                id: "m0".into(),
                text: "Thinking...".into(),
                choices: cancel_kb(),
            },
            AskEvent::EditWithKeyboard {
                id: "m0".into(),
                text: "Cancelled.".into(),
                choices: vec![],
            },
        ];
        let (sink, choices) = run(&events);
        assert_eq!(
            sink.entries,
            vec![
                Entry::Status("Thinking...".into()),
                Entry::StatusEnd,
                Entry::Markdown("Cancelled.".into()),
            ]
        );
        assert!(choices.is_none());
    }

    #[test]
    fn solve_flow_analysis_complete_then_text() {
        let events = vec![
            AskEvent::Keyboard {
                id: "m0".into(),
                text: "Analyzing <b>PROJ-1</b> with Claude...".into(),
                choices: vec![vec![choice("Cancel", "solve:cancel")]],
            },
            AskEvent::EditWithKeyboard {
                id: "m0".into(),
                text: "Analysis complete for <b>PROJ-1</b>".into(),
                choices: vec![],
            },
            AskEvent::Text {
                id: "m1".into(),
                text: "## Analysis\n- root cause: X".into(),
            },
            AskEvent::Keyboard {
                id: "m2".into(),
                text: "Ready to implement?".into(),
                choices: vec![vec![choice("Yes", "solve:implement")]],
            },
        ];
        let (sink, choices) = run(&events);
        // HTML tags must be stripped from everything that gets printed.
        assert_eq!(
            sink.entries,
            vec![
                Entry::Status("Analyzing <b>PROJ-1</b> with Claude...".into()),
                Entry::StatusEnd,
                Entry::Markdown("Analysis complete for PROJ-1".into()),
                Entry::Markdown("## Analysis\n- root cause: X".into()),
                Entry::Markdown("Ready to implement?".into()),
                Entry::Choices(vec!["Yes".into()]),
            ]
        );
        assert_eq!(choices.unwrap().items[0].data, "solve:implement");
    }

    #[test]
    fn error_event_clears_status() {
        let events = vec![
            AskEvent::Keyboard {
                id: "m0".into(),
                text: "Thinking...".into(),
                choices: cancel_kb(),
            },
            AskEvent::Error {
                message: "boom".into(),
            },
        ];
        let (sink, _) = run(&events);
        assert_eq!(
            sink.entries,
            vec![
                Entry::Status("Thinking...".into()),
                Entry::StatusEnd,
                Entry::Error("boom".into()),
            ]
        );
    }

    #[test]
    fn delete_of_status_clears_it_silently() {
        let events = vec![
            AskEvent::Keyboard {
                id: "m0".into(),
                text: "Thinking...".into(),
                choices: cancel_kb(),
            },
            AskEvent::Delete { id: "m0".into() },
        ];
        let (sink, _) = run(&events);
        assert_eq!(
            sink.entries,
            vec![Entry::Status("Thinking...".into()), Entry::StatusEnd,]
        );
    }

    #[test]
    fn plain_menu_events_print_without_status() {
        let events = vec![
            AskEvent::Text {
                id: "m0".into(),
                text: "**Jira menu** — pick one:".into(),
            },
            AskEvent::EditKeyboard {
                id: "m0".into(),
                choices: vec![vec![
                    choice("My Tickets", "jira:mine"),
                    choice("Create", "jira:create"),
                ]],
            },
        ];
        let (sink, choices) = run(&events);
        assert_eq!(
            sink.entries,
            vec![
                Entry::Markdown("**Jira menu** — pick one:".into()),
                Entry::Choices(vec!["My Tickets".into(), "Create".into()]),
            ]
        );
        assert_eq!(choices.unwrap().items.len(), 2);
    }

    #[test]
    fn trailing_done_does_not_clear_choices() {
        let events = vec![
            AskEvent::Keyboard {
                id: "m0".into(),
                text: "pick".into(),
                choices: vec![vec![choice("Go", "go")]],
            },
            AskEvent::Done,
        ];
        let (_, choices) = run(&events);
        assert!(choices.is_some());
    }

    #[test]
    fn unterminated_status_is_cleared_at_finish() {
        let mut sink = Recorder::default();
        let mut renderer = EventRenderer::new();
        renderer.render(
            &mut sink,
            &AskEvent::Keyboard {
                id: "m0".into(),
                text: "Thinking...".into(),
                choices: cancel_kb(),
            },
        );
        renderer.finish(&mut sink);
        assert_eq!(
            sink.entries,
            vec![Entry::Status("Thinking...".into()), Entry::StatusEnd]
        );
    }
}
