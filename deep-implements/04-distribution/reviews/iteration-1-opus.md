# Opus Review

**Model:** claude-opus-4
**Generated:** 2026-05-13T00:00:00Z

---

## Critical Security Issues

### 1. Checksum Verification Bootstrap Problem (Section 7)
The `checksums.txt` file is downloaded from the **same release URL** as the binary. If an attacker can MITM or compromise the release, they control both files, making verification security theater. Consider:
- Pinning a GPG public key in `install.sh` and signing `checksums.txt` (releases can be signed via `softprops/action-gh-release` with `sigstore`/`cosign`)
- At minimum, document this limitation honestly
- Consider checksumming the `install.sh` itself in the README or using a Subresource Integrity-style commit pin in the curl URL

### 2. `curl | bash` Pattern with No Version Pinning (Section 8)
The README install command uses `main` branch:
```
curl -fsSL https://raw.githubusercontent.com/sayjeyhi/jira-assistant/main/install.sh | bash
```
This is dangerous because:
- A repo compromise immediately pwns every new installer
- Users have no way to audit before running
- Plan should provide an alternate `curl ... -o install.sh && less install.sh && bash install.sh` form
- Consider pinning to a tag (`/v1.0.0/install.sh`) and have the script self-update, or document SHA-pinned URL form

### 3. Latest-Download URL is a Moving Target (Section 4, step 5)
`/releases/latest/download/${BINARY}` always resolves to whatever was last tagged. The script should:
- Accept an optional `JIRA_ASSISTANT_VERSION` env var
- Print the resolved version before installing

### 4. No `umask` Set Before Writing Config (Section 4)
The interactive wizard writes credentials to `~/.config/jira-assistant/config.json`. Without explicit `umask 077` or post-write `chmod 600`, the file may be world-readable on multi-user systems.

### 5. `trap` Cleanup Unbound Variable Risk (Section 4)
With `set -u`, an early-exit before `TMP_DIR=$(mktemp -d)` will trigger an unbound variable error inside the trap. Define `TMP_DIR=""` first or set the trap only after `mktemp -d`.

## Footguns and Edge Cases

### 6. Pipe Detection / TTY for Interactive Wizard (Section 4, step 9) — BLOCKER
When users run `curl | bash`, stdin is the curl pipe, not a TTY. The "interactive wizard" cannot read from stdin. The plan must address:
- Detecting `[ -t 0 ]` and either reopening `/dev/tty` for prompts or deferring config to a manual `jira-assistant config` run
- The script claims it "runs the interactive wizard" — this will silently fail or hang under `curl | bash`

This is a major UX bug not addressed in the plan.

### 7. `set -euo pipefail` + Truncated Download (Section 4)
When piping the script via curl, if the network connection drops mid-script, bash may execute a truncated script. Mitigation: wrap everything in a `main() { ... }; main "$@"` function so the script only executes after being fully downloaded.

### 8. Platform Detection Missing Cases (Section 4, step 1)
- `uname -m` on Apple Silicon under Rosetta returns `x86_64` — users will get x64 binary
- Linux ARM64 (Raspberry Pi, AWS Graviton) — no binary exists, should be explicitly rejected
- Alpine/musl Linux — Bun glibc binaries won't run on musl; should be detected and rejected
- WSL — systemd-on-WSL is non-trivial

### 9. `/usr/local/bin` Writability Check Is Flawed (Section 4, step 3)
If a previous install put binary in `/usr/local/bin` and a re-run picks `~/.local/bin`, you have two binaries on PATH. Install should detect and warn about existing installations at different paths.

### 10. PATH Modification Idempotency (Section 4, step 4)
- `~/.bashrc` not sourced by login shells on macOS — `~/.bash_profile` is correct there
- `~/.zprofile` relevant for macOS login shells
- Fish/nushell users not covered
- Use a `# jira-assistant PATH` marker comment for idempotent check (safer than grepping for path literal)

### 11. launchd KeepAlive Crash Loop (Section 6)
`KeepAlive: true` with a binary that crashes immediately creates a tight crash loop. Use the dictionary form:
```
KeepAlive = { SuccessfulExit = false; Crashed = true; }
ThrottleInterval = 30
```

### 12. systemd Missing Fields (Section 6)
Missing: `Type=simple`, `StartLimitIntervalSec`, `StartLimitBurst` for crash-loop backoff, `EnvironmentFile=` for config path.

### 13. `launchctl load` on Upgrade (Section 6)
Script should `launchctl unload` (ignoring errors) before loading on upgrade, or the load will fail.

### 14. Upgrade Path Not Addressed
Service must be stopped before replacing the binary (OS may not allow overwriting an executing binary on Linux). Version migration of config schema not mentioned.

### 15. Quarantine xattr on macOS — Missing from install.sh (Section 4, step 7) — IMPORTANT
Section 8 README acknowledges the quarantine issue for manual downloads, but Section 4 install.sh steps don't include `xattr -d com.apple.quarantine "$DEST" 2>/dev/null || true` after install. Curl downloads don't get the quarantine bit, but the plan should be explicit about this and add the strip as a safety measure.

## Architectural Problems

### 16. Action SHA Pinning (Section 1)
Floating major version tags on third-party actions (`softprops/action-gh-release@v2`, `oven-sh/setup-bun@v2`) is a supply-chain risk. Pin to a commit SHA.

### 17. No Pre-Release / Draft Release Strategy (Section 1)
Tags like `v1.0.0-rc.1` will be treated as full releases. Consider: `prerelease: ${{ contains(github.ref_name, '-') }}` in the release action.

### 18. Bun Version Pinning is Fragile Long-Term (Section 1)
Need a documented process for updating the pin (Renovate/Dependabot, upstream issue tracking link for the v1.3.12 regression).

### 19. No Log Rotation (Section 6)
`~/Library/Logs/jira-assistant.log` will grow unbounded. Recommend either daemon-side rotation or piping through `logger`.

### 20. `loginctl enable-linger` is Critical (Section 6)
Without it, Linux service stops on logout. Should be more emphatic in the plan — this will be a common confusion point.

## Ambiguous / Underspecified

### 21. `jira-assistant config` Subcommand Ownership (Section 4)
Is `config` defined in `01-core-daemon`? The plan should reference that contract explicitly.

### 22. Config File Path Vagueness (Section 4/8)
"`~/.config/jira-assistant/config.json` (or equivalent path)" — pick one path and be definitive.

### 23. No `--help` Flag (Section 4/5)
Scripts accepting `--uninstall` should also accept `--help`.

### 24. No `shellcheck` in CI (Section 9)
At minimum: add `shellcheck install.sh` step to the workflow. Zero-cost, catches most shell scripting bugs before users see them.

### 25. `upload-artifact@v3` vs `v4` Compatibility (Section 1)
Plan only specifies `download-artifact@v4`. Ensure `upload-artifact` is also `@v4` — cross-version compatibility was broken between v3 and v4.

## Top Priorities

1. **TTY/stdin handling** for `curl | bash` + interactive wizard — blocker
2. **`main()` wrapping** to prevent truncated-pipe execution
3. **`chmod 600`** for config file with credentials
4. **Upgrade path** — stop service before replacing binary
5. **Action SHA pinning** for supply-chain hygiene
6. **Linux ARM64 / Rosetta explicit rejection** with clear message
7. **`shellcheck` in CI** as minimum automated guard
8. **Crash-loop throttling** in launchd and systemd
9. **xattr quarantine strip** in install.sh on macOS (even if redundant for curl installs)
10. **Document checksum limitation** honestly or use actual GPG signing
