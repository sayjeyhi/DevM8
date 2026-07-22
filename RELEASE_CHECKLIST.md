# Release Checklist

Run these manually before publishing each release.

- [ ] **macOS arm64 binary** — `./devm8-macos-arm64 --version`; exit code must not be 137 (SIGKILL = code-signature regression)
- [ ] **macOS x64 binary** — `./devm8-macos-x64 --version`; clean exit
- [ ] **Linux x64 binary** — `./devm8-linux-x64 --version`; clean exit
- [ ] **Binary sizes** — each binary between 10 MB and 500 MB (smaller = likely corrupt download or HTML error page)
- [ ] **macOS codesign** — `codesign -v devm8-macos-arm64` exits 0 (ad-hoc signature present)
- [ ] **curl pipe install on macOS** — one-liner install completes; Gatekeeper does not block execution
- [ ] **curl pipe install on Linux x64** — systemd service starts; `systemctl --user status devm8` shows active
- [ ] **Restart behavior on Linux** — `systemctl --user kill devm8`; service restarts within 5 seconds (RestartSec)
- [ ] **`--uninstall` after clean install** — binary and service files gone; `~/.config/devm8/` preserved
- [ ] **Re-install over existing install** — no errors; service stops, binary replaced, service restarts
- [ ] **`~/.local/bin` fallback** — as non-root or with `/usr/local/bin` read-only, install uses `~/.local/bin` and PATH update fires
- [ ] **Checksum mismatch** — corrupt a downloaded binary byte; "Checksum mismatch" message and exit 1
- [ ] **Non-TTY stdin** — `curl ... | bash`; config wizard deferred; advisory message printed
- [ ] **Linux ARM64 rejection** — on ARM64 Linux, explicit error message and exit 1
- [ ] **Version pinning** — `DEVM8_VERSION=v1.0.0 bash install.sh`; exact version downloaded

## devm8-client

- [ ] **`install.sh --client`** — installs `devm8-client` only, no launchd/systemd service registered
- [ ] **`devm8-client login`** — pairs against a real devm8 server over a Tailscale tailnet using a code from `devm8 migrate-users`
- [ ] **`devm8-client ask`** — single-shot and interactive REPL both stream and print responses correctly
- [ ] **`devm8-client solve <key>`** — streams the analysis for a real Jira issue
- [ ] **`devm8-client history` / `history show <id>`** — lists and displays a real session's transcript
- [ ] **Telegram `/history`** — `/status` "History" button and `/history` command both paginate correctly
- [ ] **`devm8 migrate-users`** — run against a config with existing `allowed_user_ids`; placeholder users get real emails attached
- [ ] **`--client --uninstall`** — removes `devm8-client` only, leaves `~/.config/devm8-client/` in place
