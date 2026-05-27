# DevM8

A Telegram bot that lets you manage Jira tickets and run AI-assisted dev workflows from your phone — create, move, comment, solve issues, ask Claude questions about your code, and run CLI commands, all without opening a browser or laptop.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh | bash
```

Prefer to review the script before running:

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh -o install.sh
less install.sh
bash install.sh
```

Pin to a specific release:

```bash
DEV_MATE_VERSION=v1.0.0 curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh | bash
```

The installer:
- Detects your platform and downloads the correct binary
- Verifies the SHA-256 checksum against `checksums.txt`
- Installs to `/usr/local/bin` (or `~/.local/bin` if not writable)
- Registers a system service (launchd on macOS, systemd on Linux)
- **On Linux:** installs `bubblewrap` for Claude process sandboxing (see [Security](#security))
- Runs the configuration wizard on first install (skipped in non-interactive environments)

## Usage

```
$ devm8
DevM8 — Jira + Claude + Telegram assistant

Usage: devm8 <COMMAND>

Commands:
  daemon       Run the daemon process (internal — invoked by launchd)
  start        Start the daemon (macOS: launchd, Linux: systemd)
  stop         Stop the daemon
  status       Show daemon status
  logs         Show or follow daemon logs
  config       Run the configuration wizard
  update       Check for and apply binary updates
  slackmap     Configure Slack integration
  clone        Clone a repository via SSH
  add-project  Add a local git repository as a project
  version      Print version
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Requirements

| Platform | Support |
|---|---|
| macOS 12+ arm64 (M-series) | Supported |
| macOS 12+ x64 (Intel) | Supported |
| Linux x64 glibc | Supported |
| Linux ARM64 | Not supported |
| Alpine / musl Linux | Not supported |
| Windows | Not supported |

No runtime required — the binary is self-contained.

**Prerequisites:**
- A Telegram bot token ([@BotFather](https://t.me/BotFather))
- A Jira Cloud API token (`https://id.atlassian.com/manage-profile/security/api-tokens`)
- [Claude Code CLI](https://claude.ai/code) installed and authenticated (`claude login`)

## Telegram Commands

### Jira

| Command | Description |
|---|---|
| `/create` | Create a new Jira issue |
| `/move` | Move an issue to a different status |
| `/comment` | Add a comment to an issue |
| `/my_tickets` | Browse your assigned tickets with pagination |
| `/jira` | Interactive Jira panel (create, move, comment, solve from one menu) |

### Claude / AI

| Command | Description |
|---|---|
| `/ask` | Ask Claude a question about a repo, or run a CLI command inside the sandbox |
| `/solve` | Analyze a Jira ticket with Claude and get implementation steps |

### Admin

| Command | Description |
|---|---|
| `/permissions` | Manage which users can access which projects |
| `/admin` | Admin panel (clone repos, add projects) |
| `/clone` | Clone a git repository |
| `/logs` | View recent bot logs |
| `/status` | Show bot status and config summary |
| `/help` | List available commands |

## Config File

**Location:** `~/.config/devm8/config.toml`

Run `devm8 config` to launch the interactive wizard at any time.

```toml
[telegram]
bot_token = "YOUR_TELEGRAM_BOT_TOKEN"

# Users allowed to use the bot. If empty, all users are allowed.
allowed_user_ids = [123456789]

# Only this user can run admin commands (/permissions, /admin, /logs, /clone).
admin_user_id = 123456789

# Per-project access control: project key -> list of allowed user IDs.
# Users listed here but NOT in allowed_user_ids are restricted to only their granted projects.
# If a project key is absent, all allowed_user_ids can access it.
[telegram.project_access]
PROJ = [111111111, 222222222]
BZ   = [111111111]

[jira]
base_url     = "https://yourcompany.atlassian.net"
email        = "you@example.com"
api_token    = "YOUR_JIRA_API_TOKEN"
project_keys = ["PROJ", "BZ"]

[claude]
binary_path = "/usr/local/bin/claude"
# api_key = "sk-ant-..."   # optional if already authenticated via `claude login`
# sandbox = true           # default: true on Linux, false on macOS (see Security)
# timeout_ms = 300000

# Per-project repo paths for /ask and /solve.
[repos]
PROJ = ["/home/you/code/myrepo"]
BZ   = ["/home/you/code/blaze", "/home/you/code/blaze-infra"]

# Optional Slack integration
[slack]
user_token       = "xoxp-..."
poll_interval_ms = 30000

[app]
log_level = "info"  # info | debug | error
```

## Permission Management

The `/permissions` command opens an interactive menu for the admin to control project-level access:

- **Add a user** by Telegram user ID
- **Toggle project access** per user — Jira projects and git repos shown separately
- **Revoke all access** for a user in one tap
- Changes persist to the config file immediately

Access rules:
- Users in `allowed_user_ids` have unrestricted access to all projects.
- Users added only via `/permissions` (in `project_access`) are restricted to exactly the projects granted to them — they cannot access other projects via any command.
- The admin (`admin_user_id`) always has full access regardless of `project_access`.

## Security

On Linux, every Claude subprocess and CLI command run via `/ask` is isolated with **bubblewrap** (`bwrap`), a lightweight Linux namespace sandbox. The install script installs it automatically.

### What the sandbox enforces

Each Claude or shell invocation runs in a fresh namespace:

| Resource | Inside sandbox |
|---|---|
| Project directory | Mounted read-write at `/tmp/workspace` |
| `~/.claude` (auth token) | Mounted read-only |
| All other home dirs | Replaced with empty tmpfs — SSH keys, credentials, other projects invisible |
| `/root` | Replaced with empty tmpfs |
| System binaries / libs | Mounted read-only (needed for Claude to run) |
| Environment variables | Cleared — only `HOME`, `PATH`, `TMPDIR`, `ANTHROPIC_API_KEY` re-injected |
| Network | Unrestricted — Claude must reach the Anthropic API |
| PID / UTS / IPC namespaces | Isolated |

### Result

A user with access to project A cannot use `/ask` or a CLI command to read project B, `~/.ssh`, `.env` files, database credentials, or any path outside their granted project directory.

### Disabling the sandbox

Set `claude.sandbox = false` in the config to disable sandboxing (useful for debugging). On macOS, sandboxing is always off.

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh | bash -s -- --uninstall
```

- Stops and removes the service
- Removes the binary from `/usr/local/bin` or `~/.local/bin`
- Config files at `~/.config/devm8/` are left in place — remove manually if desired

## macOS Gatekeeper (Manual Downloads Only)

> Not needed when using the install script — it strips the quarantine attribute automatically.

If you download a binary manually from the [Releases](https://github.com/sayjeyhi/DevM8/releases) page:

```bash
xattr -d com.apple.quarantine /usr/local/bin/devm8
```

## Linux: Start at Boot Without Login

By default, systemd user services only run while an active session exists. To start at system boot without a login:

```bash
loginctl enable-linger $USER
```

This may require sudo on some systems. The install script prints an advisory but does not run this automatically.

## Build from Source

Requires the [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/sayjeyhi/DevM8.git
cd DevM8
cargo build --release
# Binary at: target/release/devm8
```

## Checksum Verification

The install script downloads `checksums.txt` from the same GitHub Release and verifies the SHA-256 hash before installing. This guards against download corruption. Both files are served from the same release, so this is a corruption guard rather than a tamper-proof guarantee. Users requiring stronger verification should build from source.

## License

MIT
