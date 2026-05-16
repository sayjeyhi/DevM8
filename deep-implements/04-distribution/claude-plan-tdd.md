# TDD Plan: 04-distribution

## Testing Approach

This module contains a GitHub Actions workflow, a Bash install script, and a README. Automated testing uses:
- **shellcheck** — static analysis of `install.sh` (run in CI on every push)
- **Bats** (Bash Automated Testing System) — unit/integration tests for `install.sh` functions
- **CI self-verification** — the workflow itself fails fast if any binary fails to compile

Test file location: `tests/install.bats`

Bats tests can mock system commands by defining functions that override PATH. This is the standard pattern for testing shell scripts without running on real systems.

---

## Section 1: GitHub Actions Release Workflow

**Tests to write first:**

- Verify YAML is syntactically valid (`yamllint .github/workflows/release.yml`)
- Verify `shellcheck` step exists in `lint.yml` workflow
- Verify matrix has exactly 3 entries (darwin-arm64, darwin-x64, linux-x64)
- Verify `upload-artifact` and `download-artifact` use the same major version (`@v4`)
- Verify `prerelease: ${{ contains(github.ref_name, '-') }}` is present in the release step
- Verify `bun-version: "1.3.11"` is pinned in the setup-bun step
- Verify `permissions: contents: write` is present and `read-all` is not used

These are structural lint checks, not runtime tests. Implement as a CI `validate-workflow.sh` script or `yamllint` + `grep` assertions.

---

## Section 2: Binary Build Configuration

**Tests to write first (smoke tests, run manually per release):**

- Smoke: macOS arm64 binary executes without SIGKILL (exit code ≠ 137)
- Smoke: macOS x64 binary executes without SIGKILL
- Smoke: Linux x64 binary executes and returns non-error on `--version` or equivalent flag
- Smoke: each binary's size is reasonable (between 10MB and 500MB — rules out empty file or HTML error page)
- Smoke: macOS binaries are ad-hoc signed (`codesign -v jira-assistant-macos-arm64` exits 0)

---

## Section 3: checksums.txt Generation

**Tests to write first (Bats):**

- Test: `checksums.txt` format — each line matches `<64 hex chars>  <filename>` pattern
- Test: all three binary names appear in `checksums.txt`
- Test: `sha256sum --check checksums.txt` exits 0 when run in the directory containing the binaries
- Test: corrupting one binary byte causes `sha256sum --check` to exit non-zero

---

## Section 4: install.sh — Script Structure

**Tests to write first (shellcheck + Bats):**

Static:
- `shellcheck -S error install.sh` passes with no errors
- `bash -n install.sh` passes (syntax check)

`detect_platform()`:
- Test: `uname -s=Darwin, uname -m=arm64` → `OS=macos, ARCH=arm64`
- Test: `uname -s=Darwin, uname -m=x86_64` → `OS=macos, ARCH=x64`
- Test: `uname -s=Linux, uname -m=x86_64` → `OS=linux, ARCH=x64`
- Test: `uname -s=Linux, uname -m=aarch64` → exits 1 with "Linux ARM64 is not yet supported"
- Test: `uname -s=Linux` + musl ldd → exits 1 with "Alpine/musl Linux is not supported"
- Test: `uname -s=Windows_NT` → exits 1 with "Unsupported OS"

`resolve_version()`:
- Test: `JIRA_ASSISTANT_VERSION=v1.0.0` → `VERSION=v1.0.0` (no HTTP call)
- Test: without env var, script follows redirect to extract version from URL

`download_with_retry(url, dest)`:
- Test: successful download on first attempt → dest file exists
- Test: first attempt fails (mock curl to fail), second succeeds → dest file exists
- Test: both attempts fail → function exits non-zero

`verify_checksum(binary, checksums_file)`:
- Test: correct hash → exits 0
- Test: wrong hash → exits 1 with "Checksum mismatch" message
- Test: binary name not found in checksums.txt → exits 1 with clear error

`select_install_dir()`:
- Test: `/usr/local/bin` writable → `INSTALL_DIR=/usr/local/bin`
- Test: `/usr/local/bin` not writable → `INSTALL_DIR=$HOME/.local/bin`

`ensure_path(dir)`:
- Test: dir not in `~/.zshrc` → line appended
- Test: dir already in `~/.zshrc` → no duplicate appended (idempotent)
- Test: `~/.zshrc` doesn't exist → file not created (only updates existing files)

TTY detection:
- Test: when stdin is not a TTY (`[ ! -t 0 ]`), wizard is skipped and message printed
- Test: when `~/.config/jira-assistant/config.json` already exists, wizard is skipped

`main()` wrapping:
- Test: piping a truncated version of the script through bash does not execute any side effects (verify the `main()` wrapper prevents partial execution)

---

## Section 5: install.sh — Uninstall Path

**Tests to write first (Bats):**

- Test: `--uninstall` removes binary from `/usr/local/bin/jira-assistant` if present
- Test: `--uninstall` removes binary from `~/.local/bin/jira-assistant` if present
- Test: `--uninstall` removes `~/Library/LaunchAgents/com.jira-assistant.plist` on macOS
- Test: `--uninstall` removes `~/.config/systemd/user/jira-assistant.service` on Linux
- Test: `--uninstall` does NOT remove `~/.config/jira-assistant/` (config preserved)
- Test: `--uninstall` exits 0 even when no binary is installed (graceful no-op)
- Test: `--uninstall` calls `launchctl unload` (mocked) before removing plist

---

## Section 6: Service Registration

**Tests to write first (Bats with mocked launchctl/systemctl):**

`register_macos_service(binary_path)`:
- Test: plist file created at `~/Library/LaunchAgents/com.jira-assistant.plist`
- Test: plist contains `KeepAlive` in dictionary form (not simple `true`)
- Test: plist contains `ThrottleInterval = 30`
- Test: plist contains `RunAtLoad = true`
- Test: `launchctl unload` is called before `launchctl load` (upgrade-safe)
- Test: `launchctl load` is called with the plist path

`register_linux_service(binary_path)`:
- Test: service file created at `~/.config/systemd/user/jira-assistant.service`
- Test: unit file contains `Restart=on-failure`
- Test: unit file contains `StartLimitIntervalSec=300`
- Test: unit file contains `StartLimitBurst=5`
- Test: unit file contains `Type=simple`
- Test: `systemctl --user enable --now jira-assistant` called (mocked)

---

## Section 7: Checksum Verification in install.sh

(Covered in Section 4 above — `verify_checksum` Bats tests.)

Additional:
- Test: correct hash tool used per OS — `sha256sum` on Linux, `shasum -a 256` on macOS
- Test: download failure for `checksums.txt` exits with clear error (not silently skips)

---

## Section 8: README

No automated tests. Manual review checklist:
- One-liner install command is prominent and correct
- Security-conscious alternative form is present
- `JIRA_ASSISTANT_VERSION` env var is documented
- Gatekeeper `xattr` workaround is documented under "manual download" section
- Checksum limitation is honestly disclosed
- `loginctl enable-linger` is documented with clear context
- Uninstall command is present

---

## Section 9: Testing Strategy

The testing strategy section describes the tests — it is not itself tested. Verify by reading that:
- `shellcheck` is listed as automated
- All Bats test scenarios correspond to test stubs in sections 4-7 above
- Smoke test checklist is complete enough to catch the v1.3.12 regression
