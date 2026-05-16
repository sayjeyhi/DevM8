<!-- SPLIT_MANIFEST
01-core-daemon
02-integration-clients
03-command-handlers
04-distribution
END_MANIFEST -->

# Project Manifest — Jira Assistant

Bun-based CLI daemon. Talks to Telegram, Jira Cloud, and local Claude CLI. Installs via one-line bash on macOS.

---

## Split Structure

### 01-core-daemon
**Purpose:** Foundation layer — everything the app needs to boot, configure, and run as a daemon.

Covers:
- Bun project skeleton (TypeScript, tsconfig, package.json)
- CLI entry point with subcommands: `start`, `stop`, `status`, `config`
- Config system: TOML file at `~/.config/jira-assistant/config.toml`, typed config loader
- First-run wizard: interactive prompts for Telegram token, Jira base URL, Jira API token, Jira project key, Claude binary path
- launchd integration: generate plist, `launchctl load/unload`, auto-register on `start`
- PID file management and graceful shutdown

**Inputs needed:** None (foundational)  
**Outputs:** Config system, typed config interface, CLI scaffold, daemon lifecycle API

---

### 02-integration-clients
**Purpose:** Thin, typed API clients for the three external systems.

Covers:
- **Telegram client:** Long-polling loop, update handler, message sender, slash command detection
- **Jira Cloud client:** REST v3 API wrapper — create issue, transition issue, add comment, get issue; scoped to configured project; API token auth
- **Claude CLI client:** Subprocess adapter — spawn `claude` with prompt, capture stdout, handle errors and timeouts

**Inputs needed:** Config system from `01-core-daemon` (credentials, Claude path)  
**Outputs:** `TelegramClient`, `JiraClient`, `ClaudeClient` — typed interfaces for `03-command-handlers`

---

### 03-command-handlers
**Purpose:** Slash command routing and orchestration logic — the "brain" of the bot.

Covers:
- Command router: parse incoming Telegram slash commands
- `/create <title> [description]` → ask Claude to enrich the ticket → create in Jira → reply with ticket link
- `/move <ticket> <status>` → transition Jira issue → confirm via Telegram
- `/comment <ticket> <text>` → add comment to Jira → confirm
- `/solve <ticket>` → fetch ticket details from Jira → feed to Claude → reply with AI solution
- Error handling and user-facing error messages
- Command help: `/help`

**Inputs needed:** Integration clients from `02-integration-clients`  
**Outputs:** Running bot that handles all slash commands

---

### 04-distribution
**Purpose:** Build pipeline, GitHub Releases, and one-line install script.

Covers:
- `bun build --compile` targets: macOS arm64, macOS x64, Linux x64 (Windows optional)
- GitHub Actions workflow: build on tag push, upload binaries to GitHub Release
- `install.sh`: detect OS/arch, download correct binary from latest release, run first-run wizard, auto-register launchd, create config dir
- README with one-liner install command

**Inputs needed:** Entry point and binary name from `01-core-daemon`  
**Outputs:** Installable binaries on GitHub Releases, working `curl | bash` install

---

## Execution Order

```
01-core-daemon          ← build first, everything depends on it
       |
02-integration-clients  ← needs config system
       |
03-command-handlers     ← needs integration clients
       
04-distribution         ← can be planned in parallel with 02/03,
                          needs entry point from 01
```

Sequential path: `01 → 02 → 03`  
Distribution (`04`) can be planned after `01` is defined, parallel to `02/03`.

---

## Cross-cutting Concerns

- **Error handling:** All integration clients should throw typed errors; command handlers catch and convert to user-friendly Telegram messages
- **Logging:** Structured logs to `~/.config/jira-assistant/logs/app.log` (daemon mode) and stdout (dev mode)
- **Config validation:** Fail fast at startup if required config fields are missing

---

## /deep-plan Commands (after approval)

```
/deep-plan @01-core-daemon/spec.md
/deep-plan @02-integration-clients/spec.md
/deep-plan @03-command-handlers/spec.md
/deep-plan @04-distribution/spec.md
```
