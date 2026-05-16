<!-- PROJECT_CONFIG
runtime: typescript-bun
test_command: bats tests/install.bats
END_PROJECT_CONFIG -->

<!-- SECTION_MANIFEST
section-01-ci-workflow
section-02-binary-targets
section-03-checksums-generation
section-04-install-core
section-05-install-services
section-06-install-uninstall
section-07-readme
section-08-tests
END_MANIFEST -->

YOU ARE FORCED TO IMPLEMENT EVERTHING IN THE ROOT OF THIS PROJECT not 03-command-hanlders folder!

# Implementation Sections Index: 04-distribution

## Dependency Graph

| Section | Depends On | Blocks | Parallelizable |
|---|---|---|---|
| section-01-ci-workflow | — | 03 | Yes (with 02) |
| section-02-binary-targets | — | 03, 04 | Yes (with 01) |
| section-03-checksums-generation | 01, 02 | 04 | No |
| section-04-install-core | 02, 03 | 05, 06, 07, 08 | No |
| section-05-install-services | 04 | 06, 08 | Yes (with 07) |
| section-06-install-uninstall | 04, 05 | 08 | No |
| section-07-readme | 04 | — | Yes (with 05) |
| section-08-tests | 04, 05, 06 | — | No |

## Execution Order

1. `section-01-ci-workflow`, `section-02-binary-targets` — parallel (no dependencies)
2. `section-03-checksums-generation` — after 01 + 02
3. `section-04-install-core` — after 02 + 03
4. `section-05-install-services`, `section-07-readme` — parallel after 04
5. `section-06-install-uninstall` — after 04 + 05
6. `section-08-tests` — after 04 + 05 + 06

## Section Summaries

### section-01-ci-workflow
GitHub Actions release workflow (`.github/workflows/release.yml`) triggered on `v*.*.*` tag push. Two-job structure: matrix build job + release job. Includes `lint.yml` with shellcheck step. `prerelease` flag for RC tags.

### section-02-binary-targets
Build matrix configuration: three targets (darwin-arm64, darwin-x64, linux-x64), Bun v1.3.11 pinning, binary naming convention, `upload-artifact@v4` steps per matrix entry.

### section-03-checksums-generation
`sha256sum` step in the release job (after downloading all artifacts). Generates `checksums.txt` in the standard `<hash>  <filename>` format. Included as a release asset. Documents the security limitation (same-origin with binary).

### section-04-install-core
Core `install.sh` structure: `set -euo pipefail`, `TMP_DIR=""` + `trap`, `main()` wrapper, `--help` flag, `JIRA_ASSISTANT_VERSION` env var, platform detection with explicit rejection of Linux ARM64 / musl / unsupported OS, version resolution, download with retry, checksum verification, binary installation, `xattr` quarantine strip (macOS), PATH update (`ensure_path` with marker comment idempotency). Config wizard TTY detection.

### section-05-install-services
Service registration functions: `register_macos_service()` writes launchd plist with `KeepAlive` dictionary form + `ThrottleInterval=30`; `register_linux_service()` writes systemd user unit with `Type=simple`, `StartLimitIntervalSec`, `StartLimitBurst`. `launchctl unload` before load (upgrade-safe). `loginctl enable-linger` advisory message.

### section-06-install-uninstall
`do_uninstall()` function: stops service (both launchd and systemd, ignoring errors), removes service files, removes binary from both possible install dirs, prints advisory about PATH and config cleanup, exits 0. Upgrade path: stop-before-replace flow called from `main()` on re-install.

### section-07-readme
`README.md` at repo root: one-liner install prominent at top, security-conscious inspect-first alternative, `JIRA_ASSISTANT_VERSION` pinning, requirements, command table, config file location/format, macOS Gatekeeper workaround (manual download), manual build instructions (Bun v1.3.11), uninstall command, checksum limitation note, `loginctl enable-linger` documentation.

### section-08-tests
`tests/install.bats` test file: test stubs for all `detect_platform()`, `resolve_version()`, `download_with_retry()`, `verify_checksum()`, `select_install_dir()`, `ensure_path()`, `register_macos_service()`, `register_linux_service()`, `do_uninstall()` functions using command mocking. shellcheck CI integration in `lint.yml`. Smoke test checklist for per-release manual verification.
