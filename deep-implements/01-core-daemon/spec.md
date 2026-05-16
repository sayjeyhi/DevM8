# Spec: 01-core-daemon

## What This Is

The foundational layer of the Jira Assistant — a Bun-based TypeScript CLI that starts a background daemon (macOS launchd), manages a TOML config file, and runs a first-time setup wizard.

Everything else (`02-integration-clients`, `03-command-handlers`) depends on this.

---

## Requirements Source

See `../requirements.md` for full project overview.

---

## Scope

### CLI Entry Point

Binary name: `jira-assistant` (or `ja`)

Subcommands:
- `jira-assistant start` — register launchd plist, load service, start daemon
- `jira-assistant stop` — unload launchd service
- `jira-assistant status` — show daemon running state and config summary
- `jira-assistant config` — re-run the config wizard

### Config System

- Config file: `~/.config/jira-assistant/config.toml`
- Log file: `~/.config/jira-assistant/logs/app.log`
- PID file: `~/.config/jira-assistant/daemon.pid`
- Config must be fully typed (TypeScript interface)
- Fail fast at startup if required fields are missing (with clear error message)

Required config fields:
```toml
[telegram]
bot_token = ""

[jira]
base_url = ""       # e.g. https://yourorg.atlassian.net
api_token = ""
email = ""
project_key = ""    # e.g. "ENG"

[claude]
binary_path = ""    # e.g. /usr/local/bin/claude

[app]
log_level = "info"  # info | debug | error
```

### First-Run Wizard

Triggered automatically if config file doesn't exist (or on `jira-assistant config`).

Interactive prompts (one field at a time):
1. Telegram bot token
2. Jira base URL
3. Jira email
4. Jira API token
5. Jira project key
6. Claude binary path (with auto-detect from `which claude`)

Validate each input before accepting. Write config.toml on completion.

### launchd Integration

On `start`:
1. Generate plist at `~/Library/LaunchAgents/net.jira-assistant.plist`
2. `launchctl load -w ~/Library/LaunchAgents/net.jira-assistant.plist`
3. Write PID to daemon.pid

On `stop`:
1. `launchctl unload ~/Library/LaunchAgents/net.jira-assistant.plist`
2. Remove PID file

Plist should run `jira-assistant daemon` (internal command that starts the Telegram polling loop).

### Logging

- In daemon mode: structured JSON logs to `~/.config/jira-assistant/logs/app.log`
- In dev/foreground mode: human-readable stdout
- Log levels: error, warn, info, debug

---

## Key Decisions (from interview)

- **Runtime:** Bun (TypeScript), compiled with `bun build --compile`
- **Primary OS:** macOS, launchd daemon (Linux/Windows secondary)
- **Config format:** TOML (not JSON/YAML — user preference implied by toml mention)
- **Config location:** `~/.config/jira-assistant/` (XDG-style)

---

## Interfaces This Provides

For `02-integration-clients` and `03-command-handlers`:

```typescript
// Config interface (typed, loaded at startup)
interface AppConfig {
  telegram: { bot_token: string }
  jira: { base_url: string; api_token: string; email: string; project_key: string }
  claude: { binary_path: string }
  app: { log_level: string }
}

// Logger interface
interface Logger {
  info(msg: string, meta?: object): void
  error(msg: string, meta?: object): void
  debug(msg: string, meta?: object): void
}
```

---

## Uncertainties to Resolve in Planning

- launchd plist: user-level (`~/Library/LaunchAgents`) vs system-level (`/Library/LaunchAgents`) — user-level is simpler and requires no sudo
- Should `jira-assistant daemon` be a hidden internal subcommand or explicit?
- TOML parsing library for Bun: `smol-toml` or built-in? Verify Bun TOML support.
- Config migration strategy if fields change in future versions
