# Integration Notes: Opus Review Feedback

## What I'm Integrating and Why

### 1. TTY/stdin handling for `curl | bash` + interactive wizard (BLOCKER — Point 6)
**Integrating.** This is a real functional bug. When piped through bash, stdin is the network pipe, not a terminal. The plan must specify that install.sh checks `[ -t 0 ]` and if not a TTY, skips the interactive wizard — instead printing a message like "Run `jira-assistant config` to complete setup." This is the correct design anyway; the install script shouldn't force an interactive flow.

### 2. `main() { ... }; main "$@"` wrapping (Point 7)
**Integrating.** Critical safety pattern for `curl | bash` — prevents execution of a partially-downloaded truncated script. Zero cost, high safety value.

### 3. `chmod 600` / `umask 077` for config file (Point 4)
**Integrating.** Config contains Telegram and Jira API tokens — must be user-readable only. Add to the plan's config wizard section and install.sh section.

### 4. Upgrade path — stop service before replacing binary (Point 14)
**Integrating.** On Linux, an executing binary can't always be overwritten. On macOS, the old binary stays mapped. Add a "stop service if running" step to install.sh before downloading the binary.

### 5. `xattr -d com.apple.quarantine` in install.sh (Point 28/15)
**Integrating.** Even though `curl` installs don't set the quarantine bit, add `xattr -d com.apple.quarantine "$DEST" 2>/dev/null || true` as a defensive step after install on macOS. This handles edge cases (e.g., if the binary was somehow cached by an intermediate proxy).

### 6. `shellcheck` in CI (Point 24/25)
**Integrating.** Add a `shellcheck install.sh` step to the release workflow (or a separate `lint.yml`). Zero cost, catches most shell scripting issues before users see them.

### 7. Crash-loop throttling in launchd and systemd (Points 11, 12)
**Integrating.** Replace `KeepAlive: true` with the dictionary form `KeepAlive = { Crashed = true; SuccessfulExit = false }` + `ThrottleInterval = 30`. Add `StartLimitIntervalSec=300` and `StartLimitBurst=5` to systemd unit. Also add missing `Type=simple`.

### 8. `launchctl unload` before reload on upgrade (Point 13)
**Integrating.** Add a pre-install step: `launchctl unload ~/Library/LaunchAgents/com.jira-assistant.plist 2>/dev/null || true`.

### 9. Linux ARM64 / Rosetta / musl explicit rejection (Point 8)
**Integrating.** Add explicit check: if `uname -m` returns `aarch64` on Linux, print "Linux ARM64 is not yet supported. Binaries available for x64 only." and exit 1. Add musl detection (check `/etc/os-release` for Alpine or `ldd --version` for musl string). Rosetta note: document in README that M-series Mac users should run in native arm64 shell.

### 10. `TMP_DIR=""` initialization before trap (Point 5)
**Integrating.** Minor but correct: define `TMP_DIR=""` before the trap, then assign via `TMP_DIR=$(mktemp -d)`.

### 11. `--help` flag (Point 23)
**Integrating.** Trivial addition. Add `--help` that prints usage: available flags are `--uninstall` and env var `JIRA_ASSISTANT_VERSION`.

### 12. `JIRA_ASSISTANT_VERSION` env var override + print resolved version (Point 3)
**Integrating.** Allows pinning to a specific version: `JIRA_ASSISTANT_VERSION=v1.0.0 curl ... | bash`. Print the resolved version before download. This also provides auditability.

### 13. `upload-artifact@v4` consistency (Point 25)
**Integrating.** Note explicitly that both upload and download artifact actions must be the same major version (v4).

### 14. Pre-release tag detection (Point 17)
**Integrating.** Add `prerelease: ${{ contains(github.ref_name, '-') }}` to the release action. Tags like `v1.0.0-rc.1` should be marked as pre-releases automatically.

### 15. `loginctl enable-linger` — more emphatic (Point 20)
**Integrating.** Elevate from a footnote to a clearly labeled "If you want the service to start on boot without being logged in" block with the exact command. Many users will wonder why the bot stopped after logout.

### 16. Definitive config file path (Point 22)
**Integrating.** Pick `~/.config/jira-assistant/config.json` as the canonical path (XDG standard, works on both macOS and Linux). Remove "or equivalent" hedging.

### 17. Inspect-first install alternative (Point 2)
**Integrating.** Add to README an alternative form for security-conscious users:
```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh -o install.sh
less install.sh   # review before running
bash install.sh
```

---

## What I'm NOT Integrating and Why

### A. GPG signing of checksums.txt (Point 1)
**Not integrating.** This is a personal tool with a single maintainer. GPG key management infrastructure (key generation, rotation, revocation, distribution) adds significant complexity for a tool that isn't distributing to a public user base. The checksum limitation is real and I'll document it honestly instead.

### B. Action SHA pinning (Point 16/27)
**Not integrating at plan level.** This is a valid supply-chain security practice but adds maintenance burden (SHA rotation on every action update). For a personal project, major version pinning (`@v2`) is an acceptable tradeoff. I'll add a comment in the workflow acknowledging this.

### C. SLSA build provenance attestation (Point 16 — architectural)
**Not integrating.** Overkill for a personal tool. Worth revisiting if this becomes a widely-distributed project.

### D. Disk space check (Point 30)
**Not integrating.** The failure mode (confusing error message) is acceptable for the simplicity tradeoff. Users on nearly-full disks are accustomed to confusing errors.

### E. Log rotation (Point 19)
**Not integrating into this module.** Left as a responsibility of `01-core-daemon` — the daemon itself should manage log rotation if needed. install.sh can't reasonably own this.

### F. Fish/nushell PATH support (Point 10 — fish mention)
**Not integrating.** The three standard RC files (`.zshrc`, `.bashrc`, `.profile`) cover the vast majority of users. Fish users can add PATH manually — they're typically technical enough to know how.

### G. `~/.bash_profile` vs `~/.bashrc` for macOS login shells (Point 10)
**Partially integrating.** Will add `~/.bash_profile` to the list of RC files updated (in addition to `.bashrc`), since macOS login shells source `~/.bash_profile` not `~/.bashrc`. But not covering every edge case.
