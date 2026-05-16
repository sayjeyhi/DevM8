# Usage Guide: 04-distribution

## What Was Built

The `04-distribution` sections implement the full release and distribution pipeline for the `jira-assistant` Telegram bot binary.

---

## Quick Start

### Install (one-liner)

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash
```

### Pin to a specific version

```bash
JIRA_ASSISTANT_VERSION=v1.0.0 curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash
```

### Inspect before running

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh -o install.sh
less install.sh
bash install.sh
```

### Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash -s -- --uninstall
```

### Configure

```bash
jira-assistant config
```

---

## Files Created

### Repository root

| File | Description |
|---|---|
| `install.sh` | Full installer: platform detection, download, checksum verify, service registration |
| `README.md` | User-facing documentation |
| `RELEASE_CHECKLIST.md` | 15 manual smoke tests to run before each release |

### `.github/workflows/`

| File | Description |
|---|---|
| `release.yml` | On tag push: build for darwin-arm64, darwin-x64, linux-x64; generate checksums.txt; publish GitHub Release |
| `lint.yml` | On push/PR: shellcheck -S error, bash -n, bats test suite |

### `tests/`

| File | Description |
|---|---|
| `tests/install.bats` | 64 Bats tests for install.sh functions (8 manual-only, 56 automated) |

---

## install.sh Functions

| Function | Purpose |
|---|---|
| `detect_platform` | Sets `$OS` (macos/linux) and `$ARCH` (arm64/x64); rejects unsupported platforms |
| `resolve_version` | Uses `$JIRA_ASSISTANT_VERSION` env var or follows GitHub /releases/latest redirect |
| `download_with_retry` | Downloads with one retry on failure |
| `verify_checksum` | SHA-256 verification against checksums.txt |
| `select_install_dir` | `/usr/local/bin` if writable, else `~/.local/bin` |
| `ensure_path` | Appends PATH export to shell RC files (idempotent) |
| `register_macos_service` | Writes launchd plist with KeepAlive dict form; loads it |
| `register_linux_service` | Writes systemd user unit; daemon-reload + enable --now |
| `do_uninstall` | Removes binary, service files, prints config/PATH advisory |
| `stop_existing_service` | Stops running service (ignores errors; safe on fresh install) |
| `run_config_if_needed` | Runs config wizard if no config and stdin is TTY; skips otherwise |

---

## CI/Release Flow

1. Push tag `v*` → `release.yml` triggers
2. Matrix builds: `darwin-arm64`, `darwin-x64`, `linux-x64` using `bun build --compile --target=bun-<target>`
3. `checksums.txt` generated with `sha256sum`
4. GitHub Release published with all binaries + checksums.txt
5. install.sh downloads binary + checksums.txt, verifies, installs

---

## Running Tests

```bash
bats tests/install.bats
```

Expected output: 56 tests passing, 8 skipped (manual smoke tests).

---

## Supported Platforms

| Platform | Status |
|---|---|
| macOS 12+ arm64 | Supported |
| macOS 12+ x64 | Supported |
| Linux x64 glibc | Supported |
| Linux ARM64 | Not supported |
| Alpine/musl Linux | Not supported |
| Windows | Not supported |
