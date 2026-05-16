# Synthesized Spec: 01-core-daemon

## Overview

A Bun-compiled TypeScript CLI (`jira-assistant` / `ja`) that:
1. Manages a macOS launchd background daemon (Telegram polling loop)
2. Handles a TOML config file with typed validation
3. Runs a first-time interactive setup wizard
4. Provides start/stop/status/config/daemon subcommands

This is the foundation layer. All other modules (`02-integration-clients`, `03-command-handlers`) depend on the config and logger interfaces this layer exposes.

---

## Binary

- **Names:** `jira-assistant` (primary) and `ja` (symlink or second compiled binary)
- **Runtime:** Bun, compiled with `bun build --compile --minify --sourcemap --bytecode`
- **Distribution:** GitHub Releases (direct download)
- **Target platforms:** macOS primary; Linux secondary

---

## Subcommands

| Command | Description |
|---|---|
| `start` | Register launchd plist, load service. If already running: restart. If config missing: run wizard first. |
| `stop` | Unload launchd service (plist stays on disk). |
| `status` | Human-readable summary: running state, PID, uptime, config path, key config values. |
| `config` | Run the setup wizard. Full re-run; pre-fill with existing config values. |
| `daemon` | Start the Telegram polling loop in foreground. Listed in `--help` as advanced option. |

---

## Config System

**File:** `~/.config/jira-assistant/config.toml`  
**Log file:** `~/.config/jira-assistant/logs/app.log`  
**Error log:** `~/.config/jira-assistant/logs/app.err`  
**PID file:** `~/.config/jira-assistant/daemon.pid`  
**Parsing:** `smol-toml` + `zod` schema validation

```toml
[telegram]
bot_token = ""

[jira]
base_url = ""
api_token = ""
email = ""
project_key = ""

[claude]
binary_path = ""

[app]
log_level = "info"   # info | debug | error
```

Fail fast on startup if required fields are missing, with a clear error message naming the missing field.

---

## First-Run Wizard

**Trigger:** Automatic if config doesn't exist; explicit via `jira-assistant config`.  
**Library:** `@clack/prompts`  
**Behavior on existing config:** Full re-run with all fields pre-filled from current config.  
**TTY guard:** If not a TTY, print error and exit non-zero.

Prompts (one at a time, with per-field validation):
1. Telegram bot token (min length validation)
2. Jira base URL (URL format validation)
3. Jira email (email format validation)
4. Jira API token (non-empty)
5. Jira project key (non-empty, uppercase hint)
6. Claude binary path (auto-detect via `Bun.which("claude")`; pre-fill if found)

On completion: write `config.toml`.

---

## launchd Integration

**Plist path:** `~/Library/LaunchAgents/net.jira-assistant.plist`

Key plist fields:
- `Label`: `net.jira-assistant`
- `ProgramArguments`: `["/path/to/jira-assistant", "daemon"]`
- `RunAtLoad`: `true`
- `KeepAlive`: `true`
- `StandardOutPath`: `~/.config/jira-assistant/logs/app.log`
- `StandardErrorPath`: `~/.config/jira-assistant/logs/app.err`
- `ThrottleInterval`: `10`

**`start` flow:**
1. Check if config exists → if not, run wizard
2. Check if daemon is running → if yes, run `stop` first
3. Validate/write plist
4. `launchctl load -w ~/Library/LaunchAgents/net.jira-assistant.plist`
5. Wait briefly, verify daemon is running

**`stop` flow:**
1. `launchctl unload ~/Library/LaunchAgents/net.jira-assistant.plist`
2. Remove PID file if present
3. Plist remains on disk

**launchctl error handling:** Display raw stderr + a friendly hint suggesting what went wrong.

---

## Daemon Process

The `daemon` subcommand starts the Telegram polling loop (implemented in `02-integration-clients`).

- **Log format:** TTY check → human-readable in terminal, JSON in launchd/non-TTY mode
- **Crash restart logic:** Daemon tracks its own restart count. After **10 restarts** within a sliding window, exits with code `0` (causing launchd to stop restarting per `KeepAlive.SuccessfulExit = false` semantics).
- **PID file:** Written by the daemon process on startup; removed on clean exit.

---

## Logging

**In-process logging (Logger interface):**

```typescript
interface Logger {
  info(msg: string, meta?: object): void
  error(msg: string, meta?: object): void
  warn(msg: string, meta?: object): void
  debug(msg: string, meta?: object): void
}
```

- **Terminal mode** (TTY): human-readable, colored output
- **Daemon mode** (non-TTY): structured JSON per line

**Log rotation:** Size-based. Roll `app.log` at 10MB. Keep last 5 rotated files. Implemented in-process (no external dependency).

---

## Interfaces Exported to Other Modules

```typescript
// Fully typed config
interface AppConfig {
  telegram: { bot_token: string }
  jira: { base_url: string; api_token: string; email: string; project_key: string }
  claude: { binary_path: string }
  app: { log_level: "info" | "debug" | "error" }
}

// Logger
interface Logger {
  info(msg: string, meta?: object): void
  error(msg: string, meta?: object): void
  warn(msg: string, meta?: object): void
  debug(msg: string, meta?: object): void
}

// Exported functions
loadConfig(path?: string): Promise<AppConfig>
createLogger(level: string, mode: "tty" | "json"): Logger
```

---

## Key Decisions

| Decision | Choice | Reason |
|---|---|---|
| Runtime | Bun + `bun build --compile` | Single binary, fast startup, TypeScript-native |
| Config format | TOML | User preference |
| TOML parser | smol-toml + zod | Runtime parsing + typed validation; Bun native TOML lacks stringify/runtime parse |
| CLI framework | citty | Structured subcommands, lazy loading, zero deps |
| Prompt library | @clack/prompts | TypeScript-native, Bun-compatible, minimal |
| Daemon manager | macOS launchd (user-level) | No sudo; integrates with macOS session lifecycle |
| Log format | TTY-adaptive | Same code path, different format based on TTY detection |
| Crash recovery | In-process counter, exit 0 after 10 | launchd KeepAlive stops on successful exit |
| Log rotation | In-process size-based (10MB) | No external dependency needed |

---

## Resolved Uncertainties (from spec)

1. **launchd level:** User-level (`~/Library/LaunchAgents`) — no sudo required ✓
2. **`daemon` subcommand:** Explicit public command listed in `--help` ✓
3. **TOML library:** `smol-toml` — Bun built-in lacks runtime parse API ✓
4. **Config migration:** Not needed for v1; Zod schema with `.default()` handles optional new fields gracefully ✓
