# Deep Project Interview Transcript

## Project: Jira Assistant (Bun CLI + Telegram Bot + Daemon)

---

## Requirements (inline, converted to requirements.md)

Build a Bun-based CLI app that:
- Compiles to OS-native executables
- Installs via one-line bash from GitHub
- Runs as a background daemon
- Integrates Telegram Bot API, Jira API, local Claude CLI
- Enables: create ticket, move ticket, comment, get AI solution — all via Telegram

---

## Interview Q&A

**Q: Which OS targets are priority, and how should the daemon run?**
A: macOS (launchd) — primary target. Linux/Windows are secondary for now.

**Q: How should the app talk to Claude?**
A: claude CLI subprocess — shell out to the `claude` command already installed locally.

**Q: Telegram command style?**
A: Slash commands — /create, /move, /comment, /solve (structured, not free-form NLP).

**Q: Jira setup?**
A: Jira Cloud + specific project. Single project scope. API token auth.

**Q: Install flow — what should the one-liner do?**
A: 
- Run first-time config wizard (interactive prompts for Telegram token, Jira URL/token/project, Claude path)
- Auto-register launchd service (write plist + launchctl load)
- Create ~/.config/jira-assistant/ config directory with example config.toml

**Q: Any existing code/scaffolding?**
A: Empty repo, start from scratch.

---

## Key Decisions

| Concern | Decision |
|---|---|
| Runtime | Bun (TypeScript) |
| Binary compilation | `bun build --compile` per OS |
| Primary OS | macOS (launchd daemon) |
| Claude integration | Shell out to `claude` CLI subprocess |
| Telegram UX | Structured slash commands |
| Jira scope | Jira Cloud, single project, API token |
| Config format | TOML at ~/.config/jira-assistant/config.toml |
| Install | curl one-liner → binary + wizard + launchd registration |

---

## Natural Boundaries Identified

1. **Core / Daemon infrastructure** — CLI skeleton, config system, daemon lifecycle, launchd, first-run wizard
2. **Integration clients** — Telegram polling, Jira Cloud API, Claude CLI subprocess (thin wrappers)
3. **Command handlers / orchestration** — Slash command routing + business logic wiring integrations
4. **Distribution pipeline** — bun compile, GitHub Actions CI, GitHub Releases, install.sh

## Dependencies

- Core (1) is foundational — no upstream deps
- Integrations (2) depend on Core for config loading
- Handlers (3) depend on Integrations
- Distribution (4) depends on Core for binary name/entrypoint; otherwise parallel

## Uncertainties Noted

- Claude CLI subprocess: exact invocation flags and context passing TBD during planning
- Telegram slash command argument parsing: structured args vs. multi-step conversation TBD
- launchd plist path and permissions: user-level vs. system-level service TBD
