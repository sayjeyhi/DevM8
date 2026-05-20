# DevM8

An AFK tool that lets you manage Tickets from your phone — create, move, comment, and resolve issues without opening a browser.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh | bash
```

Prefer to review the script before running:

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh -o install.sh
less install.sh    # review before running
bash install.sh
```

Pin to a specific release (for reproducible installs):

```bash
DEV_MATE_VERSION=v1.0.0 curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh | bash
```

The installer:
- Detects your platform and downloads the correct binary
- Verifies the checksum against `checksums.txt`
- Installs to `/usr/local/bin` (or `~/.local/bin` if that is not writable)
- Registers a system service (launchd on macOS, systemd on Linux)
- Runs a configuration wizard on first install (when no config exists and stdin is a TTY; skipped on re-installs and non-interactive environments)

## Requirements

| Platform | Support |
|---|---|
| macOS 12+ arm64 (M-series) | Supported |
| macOS 12+ x64 (Intel) | Supported |
| Linux x64 glibc | Supported |
| Linux ARM64 | Not supported |
| Alpine / musl Linux | Not supported — Bun binaries require glibc |
| Windows | Not supported |

**Note for M-series Mac users running under Rosetta:** if `uname -m` returns `x86_64`, you will get the x64 binary. It works, but run in a native arm64 shell for best performance.

No runtime is required — the binary is self-contained (compiled with `bun --compile`).

**Prerequisites:**
- A Telegram bot token (create one with [@BotFather](https://t.me/BotFather))
- A Jira Cloud API token (generate at `https://id.atlassian.com/manage-profile/security/api-tokens`)

## Telegram Commands

| Command | Description |
|---|---|
| `/create` | Create a new Jira issue |
| `/move` | Move an issue to a different status |
| `/comment` | Add a comment to an issue |
| `/solve` | Mark an issue as resolved |
| `/help` | List available commands |

## Config File

**Location:** `~/.config/devm8/config.toml`

The install script runs a configuration wizard on first install. For non-interactive environments (piped `curl | bash`), the wizard is skipped and you are prompted to run `devm8 config` to complete setup.

```toml
[telegram]
bot_token = "YOUR_TELEGRAM_BOT_TOKEN"
allowed_user_ids = [123456789]

[jira]
base_url = "https://yourcompany.atlassian.net"
email = "you@example.com"
api_token = "YOUR_JIRA_API_TOKEN"
project_keys = ["PROJ", "BZ"]

[claude]
binary_path = "/usr/local/bin/claude"
# api_key = "sk-ant-..."   # optional if already logged in via `claude login`

# Per-project repo paths for /solve — omit a project to disable solve for it.
# Each project can have multiple repos (comma-separated in the wizard).
[repos]
PROJ = ["/home/you/code/myrepo"]
BZ   = ["/home/you/code/blaze", "/home/you/code/blaze-infra"]

[app]
log_level = "info"  # info | debug | error
```

The config file is created with `chmod 600` (user-read-only) by the configuration wizard.

To reconfigure at any time:

```bash
devm8 config
```

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/DevM8/main/install.sh | bash -s -- --uninstall
```

The uninstall command:
- Stops and removes the service (launchd on macOS, systemd on Linux)
- Removes the binary from `/usr/local/bin` or `~/.local/bin`
- Does **not** remove config files at `~/.config/devm8/` — remove those manually if desired
- Does **not** clean up PATH entries added to shell RC files — remove those manually

## macOS Gatekeeper (Manual Downloads Only)

> This step is **not needed** when using the install script above — the script strips the quarantine attribute automatically.

If you download a binary manually from the [GitHub Releases](https://github.com/sayjeyhi/DevM8/releases) page, macOS may block it. To remove the quarantine flag:

```bash
xattr -d com.apple.quarantine /usr/local/bin/devm8
```

## Linux: Start at Boot Without Login

By default, systemd user services only run while an active session exists. To start the bot at system boot even when no user is logged in (optional):

```bash
loginctl enable-linger $USER
```

This may require sudo on some systems. The install script does not run this automatically — it only prints an advisory message.

## Checksum Verification

The install script downloads `checksums.txt` from the same GitHub Release and verifies the binary's SHA-256 hash before installing. This guards against accidental download corruption or truncation.

**Limitation:** Both the binary and `checksums.txt` are served from the same GitHub Release. A compromised release would serve both files, making this a corruption guard rather than a tamper-proof guarantee. Users who require stronger verification should build from source. GPG signing is not currently implemented.

## Build from Source

Requires **Bun v1.3.11** (pin this version — v1.3.12 has a regression that produces invalid macOS ARM64 code signatures).

```bash
git clone https://github.com/sayjeyhi/DevM8.git
cd DevM8
bun install
# Build for your current platform:
bun build --compile src/index.ts --outfile devm8
```

For cross-compilation targets, refer to the CI workflow matrix (`darwin-arm64`, `darwin-x64`, `linux-x64`) which uses the corresponding `--target=bun-<target>` flags.

## License

MIT
