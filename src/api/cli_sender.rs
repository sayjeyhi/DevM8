use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use crate::channel::{ChannelSender, Keyboard, SentMessageRef};

use super::protocol::{AskEvent, ChoiceDto, KeyboardDto};

fn to_keyboard_dto(keyboard: &Keyboard) -> KeyboardDto {
    keyboard
        .iter()
        .map(|row| {
            row.iter()
                .map(|b| ChoiceDto {
                    label: plainify_label(&b.label),
                    data: b.data.clone(),
                })
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Plain-text adaptation
// ---------------------------------------------------------------------------
//
// The chat flows format every message for Telegram: color emoji as icons and
// Telegram-HTML for markup. The client already strips the HTML tags; what it
// cannot fix is the emoji, which render as noisy full-color glyphs (or bare
// replacement boxes) in a terminal. So the CLI channel demotes them here,
// once, at the single choke point through which every flow's output flows:
//
//   - a small map replaces emoji with their monochrome text glyphs (✅→✓,
//     ❌→✗, ⭐→★) or an ASCII equivalent where no glyph exists (⬜→[ ]),
//     which also preserves the on/off state of checkbox-style pickers;
//   - everything pictographic is stripped, along with the emoji modifiers
//     (VS16, ZWJ, keycap, skin tones) that force color rendering;
//   - semantic arrows in message bodies (→, "Jira → Settings → Tokens")
//     survive, but button labels lose their arrow/shape icons (⬇️ Pull →
//     Pull) — labels are flow-authored chrome, never user content.

/// Emoji whose meaning survives as a text glyph — replaced, not stripped.
const DEMOTIONS: &[(&str, &str)] = &[
    ("\u{2705}", "✓"),   // ✅ white heavy check mark
    ("\u{274c}", "✗"),   // ❌ cross mark
    ("\u{2b50}", "★"),   // ⭐ star
    ("\u{2b1c}", "[ ]"), // ⬜ white large square (checkbox)
];

/// Pictographic blocks, emoji modifiers and presentation selectors. The
/// dingbats range keeps ✓ (U+2713) and ✗ (U+2717); 2600–26FF keeps ★/☆
/// (U+2605/U+2606) since demotions produce them.
const STRIP_CLASS: &str = concat!(
    "\u{1F000}-\u{1FAFF}",
    "\u{2600}-\u{2604}\u{2607}-\u{26FF}",
    "\u{2700}-\u{2712}\u{2714}-\u{2716}\u{2718}-\u{27BF}",
    "\u{23E9}-\u{23FA}",
    "\u{2B00}-\u{2BFF}",
    "\u{FE0E}\u{FE0F}\u{200D}\u{20E3}",
);

/// Extra ranges stripped from button labels only: arrows, misc technical and
/// geometric shapes used as keyboard icons (◀️ Prev, ▶️ npm run build).
const LABEL_ONLY_CLASS: &str = concat!(
    "\u{2190}-\u{21FF}",
    "\u{2300}-\u{23FF}",
    "\u{25A0}-\u{25FF}",
);

fn demote_and_strip(text: &str, pattern: &Regex) -> String {
    let mut out = text.to_string();
    for (emoji, glyph) in DEMOTIONS {
        out = out.replace(emoji, glyph);
    }
    let out = pattern.replace_all(&out, "");
    // An emoji at end-of-line leaves a dangling space behind.
    out.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"[{STRIP_CLASS}]\x20?")).unwrap())
}

fn label_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"[{STRIP_CLASS}{LABEL_ONLY_CLASS}]\x20?")).unwrap())
}

/// Clean up a message body for the terminal.
fn plainify_text(text: &str) -> String {
    demote_and_strip(text, text_regex())
}

/// Clean up a keyboard button label — flow-authored chrome, so its arrow and
/// shape icons go the way of the emoji.
fn plainify_label(label: &str) -> String {
    demote_and_strip(label, label_regex())
}

/// `ChannelSender` implementation for the devm8-client API: emits a JSON event
/// per call instead of making a native platform request. `/v1/ask` and
/// `/v1/solve` stream these events back to the client over SSE.
pub struct CliSender {
    tx: UnboundedSender<AskEvent>,
    seq: AtomicU64,
}

impl CliSender {
    pub fn new(tx: UnboundedSender<AskEvent>) -> Self {
        Self {
            tx,
            seq: AtomicU64::new(0),
        }
    }

    fn next_id(&self) -> String {
        format!("m{}", self.seq.fetch_add(1, Ordering::Relaxed))
    }

    fn emit(&self, event: AskEvent) {
        let _ = self.tx.send(event);
    }
}

#[async_trait]
impl ChannelSender for CliSender {
    async fn send(&self, chat_id: &str, text: &str) -> Result<SentMessageRef> {
        let id = self.next_id();
        self.emit(AskEvent::Text {
            id: id.clone(),
            text: plainify_text(text),
        });
        Ok(SentMessageRef::new(chat_id, id))
    }

    async fn send_with_keyboard(
        &self,
        chat_id: &str,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<SentMessageRef> {
        let id = self.next_id();
        self.emit(AskEvent::Keyboard {
            id: id.clone(),
            text: plainify_text(text),
            choices: to_keyboard_dto(&keyboard),
        });
        Ok(SentMessageRef::new(chat_id, id))
    }

    async fn edit_text(&self, msg_ref: &SentMessageRef, text: &str) -> Result<()> {
        self.emit(AskEvent::EditText {
            id: msg_ref.message_id.clone(),
            text: plainify_text(text),
        });
        Ok(())
    }

    async fn edit_with_keyboard(
        &self,
        msg_ref: &SentMessageRef,
        text: &str,
        keyboard: Keyboard,
    ) -> Result<()> {
        self.emit(AskEvent::EditWithKeyboard {
            id: msg_ref.message_id.clone(),
            text: plainify_text(text),
            choices: to_keyboard_dto(&keyboard),
        });
        Ok(())
    }

    async fn edit_keyboard(&self, msg_ref: &SentMessageRef, keyboard: Keyboard) -> Result<()> {
        self.emit(AskEvent::EditKeyboard {
            id: msg_ref.message_id.clone(),
            choices: to_keyboard_dto(&keyboard),
        });
        Ok(())
    }

    async fn delete_message(&self, msg_ref: &SentMessageRef) {
        self.emit(AskEvent::Delete {
            id: msg_ref.message_id.clone(),
        });
    }

    fn start_typing(&self, _chat_id: &str) -> JoinHandle<()> {
        // No native "typing" indicator over SSE — progress is already
        // communicated via the Keyboard/EditWithKeyboard "Thinking..." events.
        tokio::spawn(async {})
    }

    fn escape(&self, text: &str) -> String {
        text.to_string()
    }

    fn bold(&self, text: &str) -> String {
        format!("**{text}**")
    }

    fn italic(&self, text: &str) -> String {
        format!("_{text}_")
    }

    fn code(&self, text: &str) -> String {
        format!("`{text}`")
    }

    fn code_block(&self, text: &str) -> String {
        format!("```\n{text}\n```")
    }

    fn link(&self, url: &str, label: &str) -> String {
        format!("[{label}]({url})")
    }

    fn system_context_prefix(&self) -> &'static str {
        "\
[Context: You are responding inside the devm8-client terminal. Your text reply is the ONLY \
output the user sees. Rules:\
\n- When you run a command or read a file, ALWAYS include the actual output verbatim in your \
reply.\
\n- Format code/output in markdown code blocks.\
\n- There are no inline buttons — numbered choices are rendered as a plain list.\
\n- Keep replies concise but complete.]\
\n\n---\n\n"
    }

    fn channel_name(&self) -> &'static str {
        "cli"
    }

    async fn send_in_chunks(&self, chat_id: &str, text: &str) -> Result<()> {
        // No platform message-size limit over SSE/JSON — send as one event.
        self.send(chat_id, text).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_emoji_demote_to_text_glyphs() {
        assert_eq!(plainify_text("Status: \u{2705} clean"), "Status: ✓ clean");
        assert_eq!(
            plainify_text("Status: \u{26a0}\u{fe0f} dirty"),
            "Status: dirty"
        );
        assert_eq!(
            plainify_text("\u{274c} Connection failed: x"),
            "✗ Connection failed: x"
        );
        assert_eq!(
            plainify_text("\u{2b50} Favorite statuses saved."),
            "★ Favorite statuses saved."
        );
    }

    #[test]
    fn pictograph_icons_are_stripped_from_texts() {
        assert_eq!(
            plainify_text("\u{1f4c2} <b>chatflow</b> (<code>EISA</code>) selected."),
            "<b>chatflow</b> (<code>EISA</code>) selected."
        );
        assert_eq!(plainify_text("\u{1f9e0}Thinking..."), "Thinking...");
    }

    #[test]
    fn semantic_arrows_and_text_symbols_survive_in_texts() {
        let text =
            "Generate one at: Jira \u{2192} Account settings \u{2192} Security \u{2192} API tokens";
        assert_eq!(plainify_text(text), text);
        assert_eq!(
            plainify_text("Tap to toggle projects, then tap \u{2713} Done:"),
            "Tap to toggle projects, then tap ✓ Done:"
        );
        assert_eq!(
            plainify_text("  indented code stays"),
            "  indented code stays"
        );
    }

    #[test]
    fn labels_lose_their_icons_but_keep_meaning() {
        assert_eq!(
            plainify_label("\u{2b07}\u{fe0f} Pull latest (0 behind)"),
            "Pull latest (0 behind)"
        );
        assert_eq!(plainify_label("\u{1f33f} New branch"), "New branch");
        assert_eq!(plainify_label("\u{21a9}\u{fe0f} Back"), "Back");
        assert_eq!(plainify_label("Next \u{25b6}\u{fe0f}"), "Next");
        assert_eq!(plainify_label("\u{25b6}\u{fe0f} build"), "build");
        assert_eq!(
            plainify_label("\u{2500}\u{2500} \u{1f4cb} Jira projects \u{2500}\u{2500}"),
            "── Jira projects ──"
        );
        assert_eq!(
            plainify_label("\u{2705} Commit (3 changed)"),
            "✓ Commit (3 changed)"
        );
    }

    #[test]
    fn checkbox_state_survives_in_labels() {
        assert_eq!(plainify_label("\u{2705} EISA"), "✓ EISA");
        assert_eq!(plainify_label("\u{2b1c} EISA"), "[ ] EISA");
        assert_eq!(plainify_label("\u{2b50} EISA"), "★ EISA");
    }

    #[test]
    fn plainify_is_idempotent() {
        let once = plainify_label("\u{2b07}\u{fe0f} Pull (0 behind)");
        assert_eq!(plainify_label(&once), once);
        let once = plainify_text("Status: \u{2705} clean");
        assert_eq!(plainify_text(&once), once);
    }

    #[test]
    fn trailing_spaces_are_trimmed_per_line() {
        assert_eq!(plainify_text("first \u{1f680}\nsecond"), "first\nsecond");
    }

    #[test]
    #[ignore] // visual preview: cargo test --lib -- --ignored --nocapture plainify_preview
    fn plainify_preview() {
        let texts = [
            "\u{1f4c2} <b>chatflow</b> (<code>EISA</code>) selected.\n\nBranch: <code>ok</code>\nStatus: \u{2705} clean\n\nPull latest or type your question:",
            "Suggested branch name: <code>ask/session-.com-50ca</code>\n\nSend a name to use it, or type a different one:",
            "\u{274c} Failed to create branch: refused",
            "Jira \u{2192} Account settings \u{2192} Security \u{2192} API tokens",
        ];
        for t in texts {
            println!("IN : {t:?}");
            println!("OUT: {}\n", plainify_text(t));
        }
        let labels = [
            "\u{2b07}\u{fe0f} Pull latest (0 behind)",
            "\u{1f33f} New branch",
            "\u{1f4dc} Project scripts",
            "\u{1f4bb} CLI",
            "\u{1f5a5} OpenCode",
            "\u{1f4ac} Follow up",
            "\u{1f680} Push (1 ahead)",
            "\u{1f51a} End session",
        ];
        for l in labels {
            println!("{:>6} -> {}", format!("{l:?}"), plainify_label(l));
        }
    }
}
