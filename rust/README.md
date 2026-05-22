# devm8 — Rust

Rust port of the devm8 Telegram bot daemon. Same functionality as the TypeScript version: Jira management, Claude AI integration, Slack bridging, and macOS launchd daemon support — compiled to a single native binary.

## Requirements

- Rust 1.80+ (uses `std::sync::LazyLock`)
- macOS (for `start` / `stop` / `status` — launchd-based daemon management)
- `claude` CLI on `$PATH` (or configured path in `~/.config/devm8/config.toml`)
- `gh` CLI on `$PATH` — optional, needed for `/ask openpr`

Install Rust via [rustup](https://rustup.rs):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Build

```sh
cd rust

# Debug build (fast compile, slow binary)
cargo build

# Release build (slow compile, optimised 12 MB binary)
cargo build --release
```

Binaries land at:

| Build | Path |
|-------|------|
| debug | `target/debug/devm8` |
| release | `target/release/devm8` |

## Run

```sh
# Run directly without installing
./target/debug/devm8 --help

# Or install to ~/.cargo/bin and run from anywhere
cargo install --path .
devm8 --help
```

## First-time setup

```sh
devm8 config
```

Launches an interactive wizard that collects:

- Telegram bot token + allowed user IDs
- Jira base URL, API token, email, project keys
- Claude binary path (auto-detected if `claude` is on `$PATH`)
- Optional: Anthropic API key, per-project repo paths, Slack token

Config is written to `~/.config/devm8/config.toml` (mode `0600`).

## CLI commands

```sh
devm8 start          # Register and start the daemon via launchd (macOS)
devm8 stop           # Unload the launchd agent
devm8 status         # Show running state, PID, uptime, config summary
devm8 logs           # Print last 100 log lines
devm8 logs -n 50     # Print last 50 lines
devm8 logs -f        # Follow log output (tail -f style)
devm8 config         # Re-run configuration wizard (edits existing config)
devm8 slackmap       # Configure Slack → Telegram bridge
devm8 update         # Self-update binary from GitHub releases
devm8 daemon         # Run daemon in foreground (used internally by launchd)
```

## Running in the foreground (dev mode)

Skip launchd entirely and run the bot directly:

```sh
cargo run -- daemon
```

Or with the release binary:

```sh
./target/release/devm8 daemon
```

The daemon reads `~/.config/devm8/config.toml`, connects to Telegram, and starts polling. Press `Ctrl-C` to stop.

## Debugging

### Verbose log output

Set `log_level = "debug"` in `~/.config/devm8/config.toml`:

```toml
[app]
log_level = "debug"
```

Or temporarily override via environment (the daemon always writes JSON to the log file regardless):

```sh
RUST_LOG=debug cargo run -- daemon
```

### Attach `rust-lldb` / `rust-gdb`

```sh
cargo build   # debug symbols included by default
rust-lldb target/debug/devm8 -- daemon
```

### Live log tail during development

In one terminal:

```sh
cargo run -- daemon
```

In another:

```sh
devm8 logs -f
# or directly:
tail -f ~/.config/devm8/logs/app.log | jq .
```

### Compile-check without running

```sh
cargo check          # fast, no binary produced
cargo clippy         # lints
```

## Project layout

```
rust/
├── Cargo.toml
└── src/
    ├── main.rs              # CLI entry point (clap subcommands)
    ├── shared/
    │   ├── errors.rs        # All error types (thiserror)
    │   └── paths.rs         # ~/.config/devm8 path constants
    ├── logger/
    │   ├── mod.rs           # Logger trait + FileLogger (JSON / TTY)
    │   └── rotate.rs        # Log rotation (10 MB max, 5 files kept)
    ├── config/
    │   ├── schema.rs        # AppConfig + sub-types (serde)
    │   ├── loader.rs        # TOML read/write (atomic rename, chmod 0600)
    │   ├── validators.rs    # Field validators (regex, path checks)
    │   └── wizard.rs        # Interactive setup wizard (inquire)
    ├── jira/
    │   ├── client.rs        # Jira REST API v3 client (reqwest)
    │   ├── types.rs         # JiraIssue, JiraClientConfig
    │   └── adf.rs           # Atlassian Document Format ↔ plain text
    ├── claude/
    │   ├── client.rs        # Spawns claude CLI, streams JSON events
    │   └── types.rs         # ClaudeClientConfig, AskOptions
    ├── git/
    │   └── mod.rs           # GitClient — branch, stash, commit, push, PR
    ├── slack/
    │   ├── client.rs        # Slack Web API client
    │   ├── poller.rs        # DM polling loop with thread tracking
    │   ├── state.rs         # Cursor state persisted to disk (JSON)
    │   └── types.rs         # SlackMessage, SlackChannel, SlackUser
    ├── bot/
    │   ├── mod.rs           # AppState (JiraClient + ClaudeClient + DashMap)
    │   ├── polling.rs       # Teloxide dispatcher + command/callback routing
    │   ├── state.rs         # Per-chat state (pending comment, ask session…)
    │   ├── commands/        # /create /move /comment /solve /ask /my_tickets /logs /help
    │   ├── handlers/        # Slack forward handler + callback handlers
    │   └── utils/           # parse_args, split_message, keep_typing, escape_html
    ├── commands/            # CLI subcommand implementations
    └── daemon/
        ├── launchd.rs       # Plist generation, launchctl wrapper, status parsing
        ├── pid.rs           # PID file read/write/remove
        └── restart_tracker.rs  # Sliding-window restart limiter
```

## Config file reference

`~/.config/devm8/config.toml`:

```toml
[telegram]
bot_token         = "123456:ABCdef..."
allowed_user_ids  = [123456789]

[jira]
base_url     = "https://yourcompany.atlassian.net"
api_token    = "your-jira-api-token"
email        = "you@yourcompany.com"
project_keys = ["PROJ", "OTHER"]

[claude]
binary_path = "/usr/local/bin/claude"
# api_key = "sk-ant-..."   # optional if already logged in via `claude login`

# Optional: map project keys to local repo paths for /solve and /ask
[repos]
PROJ  = ["/path/to/your/repo"]
OTHER = ["/path/to/other/repo", "/path/to/second/repo"]

[app]
log_level = "info"   # info | debug | error

# Optional Slack bridge
[slack]
user_token       = "xoxp-..."
poll_interval_ms = 30000
```

## Logs

JSON log file: `~/.config/devm8/logs/app.log`

Rotated automatically at 10 MB (up to 5 rotated files: `app.log.1` … `app.log.5`).

Format of each line:

```json
{"level":"info","ts":"2025-01-01T12:00:00.000Z","msg":"jira connected","user":"Jane Doe","email":"jane@co.com"}
```
