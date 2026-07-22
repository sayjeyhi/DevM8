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
DEVM8_VERSION=v1.0.0 curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh | bash
```

The installer:
- Detects your platform and downloads the correct binary
- Verifies the SHA-256 checksum against `checksums.txt`
- Installs to `/usr/local/bin` (or `~/.local/bin` if not writable)
- Registers a system service (launchd on macOS, systemd on Linux)
- **On Linux:** installs `bubblewrap` for Claude process sandboxing (see [Security](#security))
- Runs the configuration wizard on first install (skipped in non-interactive environments)

## Usage

```bash
$ devm8
DevM8 — Jira + Claude + Telegram assistant

Usage: devm8 <COMMAND>

Commands:
  daemon         Run the daemon process (internal — invoked by launchd)
  start          Start the daemon (macOS: launchd, Linux: systemd)
  stop           Stop the daemon
  status         Show daemon status
  logs           Show or follow daemon logs
  config         Run the configuration wizard
  update         Check for and apply binary updates
  slackmap       Configure Slack integration
  migrate-users  Map channel identities (Telegram/Slack/Teams IDs) to emails, and
                 optionally issue a devm8-client pairing code
  clone          Clone a repository via SSH
  add-project    Add a local git repository as a project
  version        Print version
  help           Print this message or the help of the given subcommand(s)

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
| `/history` | Browse your past `/ask` and `/solve` conversations, grouped by project (also reachable via the "History" button on `/status`) |

### Admin

| Command | Description |
|---|---|
| `/permissions` | Manage which users can access which projects |
| `/admin` | Admin panel (clone repos, add projects) |
| `/clone` | Clone a git repository |
| `/logs` | View recent bot logs |
| `/status` | Show bot status and config summary |
| `/help` | List available commands |

## devm8-client (Terminal Client)

`devm8-client` is a terminal counterpart to the Telegram bot — ask Claude questions, run `/solve` against a Jira ticket, and browse chat history, all from the command line. It talks to your `devm8` server over the network (typically your [Tailscale](https://tailscale.com/) tailnet), so it can run on a completely different machine than the one hosting the bot — your laptop, for instance, while `devm8` itself runs on a home server or VPS.

### 1. Enable the API server on the `devm8` host

Add an `[api]` block to `~/.config/devm8/config.toml` on the machine running `devm8` (see [Config File](#config-file) below for the full schema), then restart:

```toml
[api]
port = 7887
```

By default this listens on all interfaces; a bearer token is required for every request, and reachability is expected to come from your Tailscale ACLs rather than a public bind address. Plain HTTP is fine over a tailnet (already WireGuard-encrypted) — set `tls_cert_path`/`tls_key_path` (e.g. from `tailscale cert <magicdns-name>`) only if you need real HTTPS.

### 2. Issue a pairing code (on the `devm8` host)

```bash
devm8 migrate-users
```

This walks you through attaching a real email to any Telegram/Slack/Teams user IDs devm8 has only ever seen as raw platform IDs, then offers to issue a short-lived pairing code for a chosen email:

```
Issue a devm8-client pairing code now? Yes
Email to pair: you@example.com

Pairing code for you@example.com: ABCDE-12345
Valid for 10 minutes.
```

### 3. Install and log in (on your own machine)

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh | bash -s -- --client
devm8-client login --server https://myserver.tailnet-name.ts.net:7887 --code ABCDE-12345
```

`--client` installs only the `devm8-client` binary — no launchd/systemd service is registered, since it's an interactive CLI, not a daemon. The issued token is stored in `~/.config/devm8-client/credentials.toml` (mode `0600`).

### Commands

```bash
devm8-client login --server <url> --code <pairing-code>   # pair this machine
devm8-client logout                                        # forget stored credentials
devm8-client whoami                                         # show the logged-in identity
devm8-client projects                                       # list accessible projects
devm8-client ask [QUESTION...]                              # pick a project, then ask Claude (interactive if no question given)
devm8-client solve <ISSUE-KEY>                              # run the /solve analysis flow
devm8-client jira                                           # open the Jira menu (tickets, create, move, comment, account setup)
devm8-client history [--limit N]                            # pick a project, then list its past sessions
devm8-client history show <SESSION_ID>                      # show a session's full transcript
```

Running `devm8-client ask` with no question starts an interactive REPL — Ctrl-D to exit. When a response includes follow-up choices (branch/commit picker, cancel, etc.), they're printed as a numbered list; type the number to select one. `devm8-client jira` uses this same numbered-choice loop to drive the same menu Telegram's `/jira` command shows: My Tickets, Create Ticket, Move Ticket, Add Comment, Solve Ticket, and account settings (connect/reconnect/disconnect a personal Jira account, manage its accessible projects, and pick favorite statuses).

Both `ask` and `history` (the listing form) prompt you to pick a project from your accessible projects before doing anything else.

### Notes

- v1 scope covers `ask`, `solve`, `jira`, and `history`.
- A CLI-only user (paired via email but never set up in Telegram/Slack/Teams) can connect their own Jira account directly from `devm8-client jira` → Settings, or reuse one already configured through a chat channel — either way, the same personal Jira credentials work across all channels.
- Chat history is shared across all channels — a `/solve` run from Telegram shows up in `devm8-client history` too, and vice versa.

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

# Per-project ticket description templates for /jira -> Create Ticket.
# Value is a path to a Markdown file with the instructions Claude follows
# when drafting a description (e.g. a bug-report skeleton with "Steps to
# Reproduce" / "Expected vs Actual" sections). Relative paths resolve
# against ~/.config/devm8. Projects with no entry, or an unreadable file,
# fall back to the built-in default instructions.
[project_ticket_templates]
PROJ = "templates/proj-bug.md"
BZ   = "/absolute/path/to/bz-template.md"

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

# Optional local API server for devm8-client (see "devm8-client (Terminal Client)" above).
# Unset entirely = disabled.
[api]
port = 7887
# bind_addr     = "0.0.0.0"   # default; the real access boundary is the bearer token + Tailscale ACLs
# tls_cert_path = "/path/to/tailscale-cert.pem"   # optional, e.g. from `tailscale cert`
# tls_key_path  = "/path/to/tailscale-key.pem"

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

To uninstall `devm8-client` instead:

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh | bash -s -- --client --uninstall
```

Credentials at `~/.config/devm8-client/` are left in place.

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
# Binaries at: target/release/devm8, target/release/devm8-client
```

## Checksum Verification

The install script downloads `checksums.txt` from the same GitHub Release and verifies the SHA-256 hash before installing. This guards against download corruption. Both files are served from the same release, so this is a corruption guard rather than a tamper-proof guarantee. Users requiring stronger verification should build from source.

## License

MIT
