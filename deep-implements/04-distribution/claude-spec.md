# Complete Specification: 04-distribution

## What This Module Does

Provides the build pipeline, GitHub Release automation, and installation experience for the jira-assistant binary. Users install the tool with a single `curl | bash` command; the script downloads the correct pre-compiled binary, installs it, registers it as a system service, and runs the first-time configuration wizard. The end state after running the installer is a fully configured, auto-starting Telegram bot daemon.

---

## Source Documents

- Initial spec: `spec.md`
- Research: `claude-research.md`
- Interview: `claude-interview.md`

---

## Repository

`sayjeyhi/jira-assistant` on GitHub.

---

## Binary Targets

Three targets (Windows skipped as stretch goal):

| Binary name | Platform | Bun target flag |
|---|---|---|
| `jira-assistant-macos-arm64` | macOS Apple Silicon | `bun-darwin-arm64` |
| `jira-assistant-macos-x64` | macOS Intel | `bun-darwin-x64` |
| `jira-assistant-linux-x64` | Linux x64 | `bun-linux-x64` |

Build command:
```bash
bun build src/index.ts --compile --target=<TARGET> --outfile=<NAME>
```

**Critical:** Pin to Bun **v1.3.11** in CI. v1.3.12 has a SIGKILL regression for macOS arm64 cross-compiled binaries (broken code signature — SuperBlob header length mismatch).

---

## GitHub Actions Workflow (`.github/workflows/release.yml`)

Trigger: `push` of tag matching `v*.*.*`

Structure:
1. **Build job** (matrix, 3 targets) on `ubuntu-latest`:
   - Checkout, setup Bun v1.3.11, `bun install`
   - Build the target binary
   - Upload artifact
   - Generate `checksums.txt` (SHA256 for all artifacts — done in a separate step after all builds)

2. **Release job** (depends on build):
   - Download all artifacts
   - Create `checksums.txt` by SHA256-hashing all binaries
   - Create GitHub Release using `softprops/action-gh-release@v2`
   - Attach all 3 binaries + `checksums.txt` as release assets
   - `generate_release_notes: true`

Permissions: `contents: write` only. Uses `GITHUB_TOKEN` automatically.

---

## install.sh

Entry point:
```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash
```

### Script structure

1. `set -euo pipefail` + `trap` for temp file cleanup
2. Detect OS (`uname -s`: Darwin/Linux) and arch (`uname -m`: arm64/aarch64/x86_64)
3. Map to binary name (e.g., `jira-assistant-macos-arm64`)
4. Determine install dir: `/usr/local/bin` if writable; else `~/.local/bin`
5. If `~/.local/bin`, run `ensure_path` to idempotently update `~/.zshrc`, `~/.bashrc`, `~/.profile`
6. Download binary via `/releases/latest/download/<NAME>` redirect (no GitHub API — no rate limit risk)
7. Retry once on download failure
8. Download `checksums.txt` from same release
9. Verify SHA256 of downloaded binary against `checksums.txt`
10. Install binary (`mv` + `chmod +x`)
11. Register system service (see below)
12. If no config file exists: run `jira-assistant config` (interactive wizard)
13. Start service
14. Print success + next steps

### `--uninstall` flag

When called with `--uninstall`:
1. Stop and unload service (launchd or systemd)
2. Remove service files
3. Remove binary from install dir
4. Print note about manual PATH cleanup
5. Exit 0

### Error handling

- Unsupported OS/arch → print message and `exit 1`
- Download failure after retry → print error and `exit 1`
- Checksum mismatch → print warning and `exit 1`
- No write permission + `~/.local/bin` exists but not in PATH → update RC files and notify

### macOS launchd service

File: `~/Library/LaunchAgents/com.jira-assistant.plist`

Key properties:
- `Label`: `com.jira-assistant`
- `ProgramArguments`: path to binary + `start`
- `RunAtLoad`: `true`
- `KeepAlive`: `true` (restart on crash)
- `StandardOutPath` / `StandardErrorPath`: `~/Library/Logs/jira-assistant.log`

Load with: `launchctl load ~/Library/LaunchAgents/com.jira-assistant.plist`

### Linux systemd user service

File: `~/.config/systemd/user/jira-assistant.service`

Key properties:
- `[Unit] Description`: `Jira Assistant Telegram Bot`
- `[Service] ExecStart`: path to binary + `start`
- `[Service] Restart`: `on-failure`
- `[Install] WantedBy`: `default.target`

Enable with: `systemctl --user enable --now jira-assistant`
Requires `loginctl enable-linger` for start-on-boot without a logged-in session.

---

## checksums.txt Generation in CI

After all three binaries are built, a post-build step runs:
```bash
sha256sum jira-assistant-macos-arm64 jira-assistant-macos-x64 jira-assistant-linux-x64 > checksums.txt
```
(On macOS runner: `shasum -a 256`; but all builds run on ubuntu-latest, so `sha256sum` is available.)

`checksums.txt` is uploaded as a release asset alongside the binaries.

---

## README Structure

1. **One-liner install** (prominent, at the top)
2. Requirements (macOS 12+ or Linux; no Bun needed — binary is standalone)
3. Available Telegram commands
4. Config file location and format
5. macOS Gatekeeper note (for users who download manually via browser):
   ```bash
   xattr -d com.apple.quarantine /usr/local/bin/jira-assistant
   ```
6. Manual build instructions (for contributors — requires Bun v1.3.11)
7. Uninstall instructions (`curl ... | bash -- --uninstall` or manual steps)

---

## macOS Gatekeeper Strategy

Binaries distributed via `install.sh` (which uses `curl`) do **not** receive the `com.apple.quarantine` extended attribute. Gatekeeper only blocks quarantined files, so the curl install path bypasses the problem entirely.

For users who manually download from the Releases page:
- Provide `xattr -d com.apple.quarantine` instructions in README
- macOS 15.1+ requires Privacy & Security → "Open Anyway" for blocked binaries

Bun's `--compile` applies ad-hoc signing automatically (satisfies ARM64 minimum signing requirement).

---

## Resolved Uncertainties

| Uncertainty | Resolution |
|---|---|
| Bun cross-compilation from Linux | Works since v1.1.5; pin to v1.3.11 to avoid v1.3.12 regression |
| GitHub API rate limits for version fetch | Use `/releases/latest/download/FILENAME` redirect — no API call needed |
| macOS Gatekeeper unsigned binary | curl install bypasses quarantine; README covers manual download case |
| PATH update in install.sh | Idempotent update to ~/.zshrc, ~/.bashrc, ~/.profile; also export in current session |
| Uninstall flag | `--uninstall` in install.sh handles full cleanup |
