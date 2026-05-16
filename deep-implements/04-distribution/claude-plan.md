# Implementation Plan: 04-distribution

## Overview

This module produces three artifacts: (1) a GitHub Actions workflow that compiles cross-platform binaries and publishes them as GitHub Release assets, (2) an `install.sh` script that users pipe through `bash` to install and configure the tool in one command, and (3) a README that documents installation, commands, and manual build steps.

The build toolchain is Bun's `--compile` feature, which bundles a TypeScript entry point into a standalone binary that requires no runtime on the target machine. All three builds (macOS arm64, macOS x64, Linux x64) run from a single `ubuntu-latest` GitHub Actions runner using Bun's cross-compilation support. A known regression in v1.3.12 breaks macOS ARM64 cross-compiled binaries with an invalid code signature; the workflow pins Bun to v1.3.11 to avoid this.

The install experience is designed so that a user on macOS or Linux x64 can run one `curl | bash` command and end up with a configured, auto-starting Telegram bot daemon.

---

## Section 1: GitHub Actions Release Workflow

The workflow file lives at `.github/workflows/release.yml` and triggers on any tag push matching `v*.*.*`. It requires only `contents: write` permission, which allows `GITHUB_TOKEN` to create releases and upload assets without additional secrets.

The workflow has two jobs:

**Build job** runs in a 3-way matrix on `ubuntu-latest`. Each matrix entry defines a Bun target flag and an output filename. The job checks out the repository, installs Bun v1.3.11 via `oven-sh/setup-bun@v2`, runs `bun install`, then runs `bun build --compile` for the matrix target. The resulting binary is uploaded as a workflow artifact using `actions/upload-artifact@v4`. **Important:** both upload and download artifact steps must use the same major version (`@v4`) — v3 and v4 are not cross-compatible.

**Release job** waits for all three build matrix entries to succeed (`needs: build`). It downloads all artifacts using `actions/download-artifact@v4` with `merge-multiple: true`. It generates `checksums.txt` via `sha256sum`. It calls `softprops/action-gh-release@v2` with:
- `generate_release_notes: true`
- `files: artifacts/* checksums.txt`
- `prerelease: ${{ contains(github.ref_name, '-') }}` — tags like `v1.0.0-rc.1` are automatically marked pre-release

A separate `lint.yml` workflow (or a step in `release.yml`) runs `shellcheck install.sh` to catch shell scripting issues before they reach users.

Note on action pinning: `softprops/action-gh-release@v2` and `oven-sh/setup-bun@v2` use floating major version tags. SHA pinning (e.g., `@abc123`) is best practice for supply-chain hardening but is deferred for this personal project.

File layout:
```
.github/
  workflows/
    release.yml
    lint.yml       ← shellcheck
```

Workflow structure:
```
trigger: push tags v*.*.*
permissions: contents: write
jobs:
  build(matrix: [darwin-arm64, darwin-x64, linux-x64]):
    - checkout@v4
    - setup-bun@v2 (version: 1.3.11)
    - bun install
    - bun build --compile --target=... --outfile=...
    - upload-artifact@v4 (name: matrix.outfile, path: matrix.outfile)
  release(needs: build):
    - download-artifact@v4 (merge-multiple: true, path: artifacts/)
    - sha256sum artifacts/* > checksums.txt
    - action-gh-release@v2 (generate_release_notes, prerelease, files)
```

---

## Section 2: Binary Build Configuration

The three build targets:

| Bun target | Output filename |
|---|---|
| `bun-darwin-arm64` | `jira-assistant-macos-arm64` |
| `bun-darwin-x64` | `jira-assistant-macos-x64` |
| `bun-linux-x64` | `jira-assistant-linux-x64` |

Entry point: `src/index.ts` (from `01-core-daemon`).

The `-baseline` variants are not needed unless users report "Illegal instruction" on older hardware. Windows is excluded from the initial release — adding it later requires only a new matrix entry.

Bun's `--compile` automatically applies an ad-hoc code signature to macOS binaries, satisfying the ARM64 minimum signing requirement. With Bun v1.3.11 (pre-regression), this signature is correctly formed.

**Bun version update process:** The v1.3.12 regression (GitHub issue #29120, PR #29272) should be tracked. When the fix ships, run the macOS arm64 smoke test before updating the pinned version.

---

## Section 3: checksums.txt Generation and Limitations

After all binaries are downloaded in the release job, `sha256sum` generates the checksum file. Its format (`<hash>  <filename>`) is what install.sh parses during verification.

**Security limitation:** both the binary and `checksums.txt` are downloaded from the same release. A compromised release serves both, making the checksum a corruption/truncation guard rather than a tamper-proof guarantee. GPG signing would address this but is out of scope for this personal tool. This limitation is documented honestly in the README.

---

## Section 4: install.sh — Script Structure

The script uses several critical safety patterns:

1. `#!/usr/bin/env bash` — ensures bash regardless of user's default shell
2. `set -euo pipefail` — aborts on any error, unbound variable, or pipe failure
3. `TMP_DIR=""` defined before `trap` — avoids unbound variable error if script exits before `mktemp -d`
4. `TMP_DIR=$(mktemp -d); trap 'rm -rf "$TMP_DIR"' EXIT` — guaranteed temp cleanup
5. `main() { ... }; main "$@"` wrapping — **critical for `curl | bash`**: the entire script body is inside `main()`, which is only called after the script is fully downloaded. This prevents bash from executing a partially-downloaded truncated script.

All logic lives inside `main()`. The structure of `main()`:

1. **Handle flags** — if first arg is `--uninstall`, call `do_uninstall` and exit. If `--help`, print usage and exit.
2. **Determine version** — if `JIRA_ASSISTANT_VERSION` env var is set, use it; otherwise use `/releases/latest`. Print the resolved version before any downloads.
3. **Platform detection** — reads `uname -s` and `uname -m`. Explicit rejection cases:
   - `uname -s` not Darwin or Linux → "Unsupported OS: ... Only macOS and Linux x64 are supported."
   - Linux + `aarch64` → "Linux ARM64 is not yet supported. Only x64 binaries are available for Linux."
   - Linux + Alpine/musl (detected via `/etc/os-release` or `ldd --version` containing "musl") → "Alpine/musl Linux is not supported. Bun binaries require glibc."
   - Note in README: M-series Mac users on Rosetta (`uname -m` returns `x86_64`) will get the x64 binary — this is functional but suboptimal; they should run in a native arm64 shell.
4. **Binary name construction** — `jira-assistant-${OS}-${ARCH}`
5. **Stop existing service if running** — `launchctl unload ... 2>/dev/null || true` (macOS) or `systemctl --user stop ... 2>/dev/null || true` (Linux). This must happen before downloading to a path that may be in use.
6. **Install directory selection** — `/usr/local/bin` if writable; else `~/.local/bin`. If previous install was at different dir, print a warning.
7. **PATH update** — if using `~/.local/bin`, call `ensure_path` to idempotently append export line to `~/.zshrc`, `~/.bashrc`, `~/.bash_profile`, and `~/.profile`, using a `# jira-assistant` marker comment to detect existing entries. Also export for the current session.
8. **Download binary** — fetch to `$TMP_DIR/$BINARY`. On failure, retry once. Use `curl -fsSL --fail-with-body` to catch HTTP error responses.
9. **Download checksums.txt** — fetch to `$TMP_DIR/checksums.txt`.
10. **Verify checksum** — grep expected hash from checksums.txt, compute actual hash of downloaded binary, compare. Exit 1 on mismatch with clear error message.
11. **Install binary** — `mv $TMP_DIR/$BINARY $INSTALL_DIR/jira-assistant && chmod +x ...`
12. **Strip quarantine** (macOS only) — `xattr -d com.apple.quarantine "$INSTALL_DIR/jira-assistant" 2>/dev/null || true`. Belt-and-suspenders: curl installs don't set the quarantine bit, but this handles edge cases.
13. **Register service** — see Section 6
14. **Config wizard** — check if `~/.config/jira-assistant/config.json` exists. **If stdin is not a TTY** (`[ ! -t 0 ]`), skip the wizard and instead print: "Run `jira-assistant config` to complete setup." If stdin IS a TTY, run the wizard (which reopens `/dev/tty` internally if needed). Ensure config file is created with `chmod 600` / `umask 077` in the wizard.
15. **Start service**
16. **Success message** — print installed version, install dir, service status, and shell restart reminder if PATH was modified

Functions:
```
main(args...)
do_uninstall()
detect_platform() → sets $OS, $ARCH
build_binary_name() → sets $BINARY
resolve_version() → sets $VERSION (env var or /releases/latest redirect)
stop_existing_service()
select_install_dir() → sets $INSTALL_DIR
ensure_path(dir)
download_with_retry(url, dest)
verify_checksum(binary, checksums_file)
install_binary(src, dest)
strip_quarantine(path)  ← macOS only
register_macos_service(binary_path)
register_linux_service(binary_path)
run_config_if_needed()
start_service()
print_success()
```

---

## Section 5: install.sh — Uninstall Path (`do_uninstall`)

Called when `--uninstall` is the first argument:

1. Stop and unload service — `launchctl unload` (macOS) or `systemctl --user stop && disable` (Linux), both ignoring errors
2. Remove service file — `~/Library/LaunchAgents/com.jira-assistant.plist` or `~/.config/systemd/user/jira-assistant.service`
3. Remove binary — check both `/usr/local/bin/jira-assistant` and `~/.local/bin/jira-assistant`, remove whichever exists
4. Print note: "Config files at `~/.config/jira-assistant/` were left in place. Remove manually if desired."
5. Print note: "PATH entries in shell RC files must be cleaned up manually."
6. Exit 0

Config files are deliberately not removed to preserve user data.

---

## Section 6: Service Registration

### macOS — launchd

Written to `~/Library/LaunchAgents/com.jira-assistant.plist` (no sudo needed).

Key properties:
- `Label`: `com.jira-assistant`
- `ProgramArguments`: `[<binary_path>, "start"]`
- `RunAtLoad`: `true`
- `KeepAlive`: dictionary form `{ Crashed = true; SuccessfulExit = false; }` — restarts on crash but not on clean exit. Avoids restart loops when the user stops the service intentionally.
- `ThrottleInterval`: `30` — minimum 30 seconds between restart attempts, preventing runaway crash loops if misconfigured
- `StandardOutPath` / `StandardErrorPath`: `~/Library/Logs/jira-assistant.log`

Before loading: `launchctl unload ~/Library/LaunchAgents/com.jira-assistant.plist 2>/dev/null || true` (handles upgrade over existing install).

Load: `launchctl load ~/Library/LaunchAgents/com.jira-assistant.plist`

### Linux — systemd user service

Written to `~/.config/systemd/user/jira-assistant.service` (no sudo needed).

Unit properties:
- `[Unit] Description`: `Jira Assistant Telegram Bot`
- `[Unit] After`: `network.target`
- `[Service] Type`: `simple`
- `[Service] ExecStart`: `<binary_path> start`
- `[Service] Restart`: `on-failure`
- `[Service] RestartSec`: `5`
- `[Service] StartLimitIntervalSec`: `300`
- `[Service] StartLimitBurst`: `5` — after 5 restart attempts within 5 minutes, systemd stops retrying
- `[Install] WantedBy`: `default.target`

Enable and start: `systemctl --user enable --now jira-assistant`

**Important — auto-start on boot without login:** By default, systemd user services only run while a user session is active. To run on boot without being logged in:
```bash
loginctl enable-linger $USER
```
The install script prints this as a clearly-labeled optional step: "If you want jira-assistant to start at boot even when you're not logged in, run: `loginctl enable-linger $USER`". This requires sudo on some systems and is not run automatically.

---

## Section 7: Checksum Verification in install.sh

After downloading the binary to a temp file:
1. Download `checksums.txt` from the same release path
2. Extract the expected SHA256 hash for the current binary name via `grep`
3. Compute actual SHA256 — `sha256sum` on Linux, `shasum -a 256` on macOS
4. Compare strings; if mismatch: `"Checksum mismatch — download may be corrupted. Delete $TMP_DIR and retry."` and exit 1

**Limitation note:** since both the binary and `checksums.txt` come from the same release, this guards against accidental corruption or truncation but not against a compromised release. GPG signing is the proper solution; it is out of scope for this tool.

---

## Section 8: README

**Section 1 — Install (at the top):**
```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash
```
With a security-conscious alternative:
```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh -o install.sh
less install.sh    # review before running
bash install.sh
```
And optional version pinning: `JIRA_ASSISTANT_VERSION=v1.0.0 curl ... | bash`

**Section 2 — Requirements:** macOS 12+ (arm64 or x64) or Linux x64 (glibc, not Alpine). Telegram bot token, Jira Cloud API token. No runtime needed — binary is self-contained.

**Section 3 — Available Telegram commands:** table of `/create`, `/move`, `/comment`, `/solve`, `/help`.

**Section 4 — Config file:** canonical location `~/.config/jira-assistant/config.json`, format with all required keys.

**Section 5 — macOS Gatekeeper (manual download only):** the install script handles this automatically. For manual downloads from the Releases page:
```bash
xattr -d com.apple.quarantine /usr/local/bin/jira-assistant
```

**Section 6 — Manual build:** requires Bun v1.3.11.

**Section 7 — Uninstall:**
```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash -s -- --uninstall
```

**Section 8 — Checksum note:** the `checksums.txt` file guards against download corruption. It does not use GPG signing; users who need stronger verification should build from source.

---

## Section 9: Testing Strategy

**Automated (CI):**
- `shellcheck install.sh` in a `lint.yml` workflow on every push — catches the most common shell scripting bugs (unquoted variables, missing `-r` on read, incorrect test operators, etc.)
- `bash -n install.sh` syntax check as part of the same lint step
- The release workflow itself validates all three binaries compile successfully

**Smoke tests (manual, per release):**
- Run each binary on its target platform; confirm it starts without crashing
- On macOS arm64: verify binary is not SIGKILL'd (confirms no code signature regression)
- On macOS: confirm `curl | bash` install works end-to-end without Gatekeeper interference
- On Linux: confirm systemd service starts, and `systemctl --user kill jira-assistant` triggers restart within `RestartSec`

**install.sh scenarios (manual):**
- `--uninstall` after clean install
- Re-install over existing install (upgrade path — service stops, binary replaced, service restarts)
- `~/.local/bin` fallback (make `/usr/local/bin` temporarily non-writable)
- Checksum mismatch (corrupt binary bytes before verify step)
- Non-TTY stdin (pipe script through `cat | bash` to simulate `curl | bash` — verify config wizard is deferred)
- Linux ARM64 machine — verify explicit rejection message
- `JIRA_ASSISTANT_VERSION=v0.9.0 bash install.sh` — verify pinned version installs correctly
