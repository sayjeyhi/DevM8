diff --git a/README.md b/README.md
new file mode 100644
index 0000000..d2edf3f
--- /dev/null
+++ b/README.md
@@ -0,0 +1,143 @@
+# jira-assistant
+
+A Telegram bot that lets you manage Jira issues from your phone — create, move, comment, and resolve issues without opening a browser.
+
+## Install
+
+```bash
+curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash
+```
+
+Prefer to review the script before running:
+
+```bash
+curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh -o install.sh
+less install.sh    # review before running
+bash install.sh
+```
+
+Pin to a specific release (for reproducible installs):
+
+```bash
+JIRA_ASSISTANT_VERSION=v1.0.0 curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash
+```
+
+The installer:
+- Detects your platform and downloads the correct binary
+- Verifies the checksum against `checksums.txt`
+- Installs to `/usr/local/bin` (or `~/.local/bin` if that is not writable)
+- Registers a system service (launchd on macOS, systemd on Linux)
+- Runs a configuration wizard on first install (when stdin is a TTY)
+
+## Requirements
+
+| Platform | Support |
+|---|---|
+| macOS 12+ arm64 (M-series) | Supported |
+| macOS 12+ x64 (Intel) | Supported |
+| Linux x64 glibc | Supported |
+| Linux ARM64 | Not supported |
+| Alpine / musl Linux | Not supported — Bun binaries require glibc |
+| Windows | Not supported |
+
+**Note for M-series Mac users running under Rosetta:** if `uname -m` returns `x86_64`, you will get the x64 binary. It works, but run in a native arm64 shell for best performance.
+
+No runtime is required — the binary is self-contained (compiled with `bun --compile`).
+
+**Prerequisites:**
+- A Telegram bot token (create one with [@BotFather](https://t.me/BotFather))
+- A Jira Cloud API token (generate at `https://id.atlassian.com/manage-profile/security/api-tokens`)
+
+## Telegram Commands
+
+| Command | Description |
+|---|---|
+| `/create` | Create a new Jira issue |
+| `/move` | Move an issue to a different status |
+| `/comment` | Add a comment to an issue |
+| `/solve` | Mark an issue as resolved |
+| `/help` | List available commands |
+
+## Config File
+
+**Location:** `~/.config/jira-assistant/config.json`
+
+The install script runs a configuration wizard on first install. For non-interactive environments (piped `curl | bash`), the wizard is skipped and you are prompted to run `jira-assistant config` to complete setup.
+
+```json
+{
+  "telegram": {
+    "botToken": "YOUR_TELEGRAM_BOT_TOKEN"
+  },
+  "jira": {
+    "baseUrl": "https://yourcompany.atlassian.net",
+    "email": "you@example.com",
+    "apiToken": "YOUR_JIRA_API_TOKEN",
+    "projectKey": "PROJ"
+  }
+}
+```
+
+The config file is created with `chmod 600` (user-read-only) by the configuration wizard.
+
+To reconfigure at any time:
+
+```bash
+jira-assistant config
+```
+
+## Uninstall
+
+```bash
+curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash -s -- --uninstall
+```
+
+The uninstall command:
+- Stops and removes the service (launchd on macOS, systemd on Linux)
+- Removes the binary from `/usr/local/bin` or `~/.local/bin`
+- Does **not** remove config files at `~/.config/jira-assistant/` — remove those manually if desired
+- Does **not** clean up PATH entries added to shell RC files — remove those manually
+
+## macOS Gatekeeper (Manual Downloads Only)
+
+> This step is **not needed** when using the install script above — the script strips the quarantine attribute automatically.
+
+If you download a binary manually from the [GitHub Releases](https://github.com/sayjeyhi/jira-assistant/releases) page, macOS may block it. To remove the quarantine flag:
+
+```bash
+xattr -d com.apple.quarantine /usr/local/bin/jira-assistant
+```
+
+## Linux: Start at Boot Without Login
+
+By default, systemd user services only run while an active session exists. To start the bot at system boot even when no user is logged in (optional):
+
+```bash
+loginctl enable-linger $USER
+```
+
+This may require sudo on some systems. The install script does not run this automatically — it only prints an advisory message.
+
+## Checksum Verification
+
+The install script downloads `checksums.txt` from the same GitHub Release and verifies the binary's SHA-256 hash before installing. This guards against accidental download corruption or truncation.
+
+**Limitation:** Both the binary and `checksums.txt` are served from the same GitHub Release. A compromised release would serve both files, making this a corruption guard rather than a tamper-proof guarantee. Users who require stronger verification should build from source. GPG signing is not currently implemented.
+
+## Build from Source
+
+Requires **Bun v1.3.11** (pin this version — v1.3.12 has a regression that produces invalid macOS ARM64 code signatures).
+
+```bash
+git clone https://github.com/sayjeyhi/jira-assistant.git
+cd jira-assistant
+bun install
+# Build for your current platform:
+bun build --compile src/index.ts --outfile jira-assistant
+```
+
+For cross-compilation targets, refer to the CI workflow matrix (`darwin-arm64`, `darwin-x64`, `linux-x64`) which uses the corresponding `--target=bun-<target>` flags.
+
+## License
+
+MIT
