Now I have all the context I need. Let me generate the section content for `section-06-install-uninstall`.

# Section 06: Install/Uninstall — `do_uninstall()` and Upgrade Flow

## Overview

This section implements two related flows in `install.sh`:

1. **`do_uninstall()` function** — called when the user passes `--uninstall` as the first argument. Cleanly removes the binary, service registration, and service files. Config files are intentionally preserved.
2. **Upgrade/re-install flow** — called from `main()` when a binary is already installed. Stops the running service before replacing the binary, then re-starts after install.

## Dependencies

- **section-04-install-core** must be complete: the `install.sh` file structure, `set -euo pipefail`, `TMP_DIR + trap`, `main()` wrapper, `detect_platform()`, and `select_install_dir()` are all defined there. This section adds functions to that same file.
- **section-05-install-services** must be complete: `register_macos_service()` and `register_linux_service()` are defined there, and the service file paths and `launchctl`/`systemctl` command patterns established there are reused here in reverse.

## File to Modify

`install.sh` — add `do_uninstall()` and `stop_existing_service()` functions, and wire `--uninstall` dispatch in `main()`.

## Tests First

From `tests/install.bats` — stub these test cases before implementing the functions.

### `do_uninstall()` Bats Tests

```bash
# Test: removes binary from /usr/local/bin if present
@test "do_uninstall removes /usr/local/bin/jira-assistant" { ... }

# Test: removes binary from ~/.local/bin if present
@test "do_uninstall removes ~/.local/bin/jira-assistant" { ... }

# Test: removes macOS launchd plist
@test "do_uninstall removes ~/Library/LaunchAgents/com.jira-assistant.plist on macOS" { ... }

# Test: removes Linux systemd service file
@test "do_uninstall removes ~/.config/systemd/user/jira-assistant.service on Linux" { ... }

# Test: config directory is preserved
@test "do_uninstall does NOT remove ~/.config/jira-assistant/" { ... }

# Test: exits 0 even when no binary is installed
@test "do_uninstall exits 0 gracefully when nothing is installed" { ... }

# Test: calls launchctl unload before removing plist
@test "do_uninstall calls launchctl unload before removing plist" { ... }
```

Bats mock pattern — override commands in test scope by prepending a `PATH`-priority bin directory:

```bash
setup() {
  MOCK_BIN="$(mktemp -d)"
  export PATH="$MOCK_BIN:$PATH"
  # create mock launchctl, systemctl that record calls
  printf '#!/usr/bin/env bash\necho "launchctl $*" >> "$MOCK_BIN/calls.log"' > "$MOCK_BIN/launchctl"
  chmod +x "$MOCK_BIN/launchctl"
}
```

### `stop_existing_service()` Bats Tests

```bash
# Test: on macOS, calls launchctl unload with plist path, ignores errors
@test "stop_existing_service calls launchctl unload on macOS, ignores errors" { ... }

# Test: on Linux, calls systemctl --user stop, ignores errors
@test "stop_existing_service calls systemctl --user stop on Linux, ignores errors" { ... }
```

## Implementation Details

### `stop_existing_service()` Function

This function is called from two places:
1. From `do_uninstall()` — to stop service before removing files.
2. From `main()` — to stop service before downloading and installing (upgrade path).

Behavior:
- On macOS: `launchctl unload ~/Library/LaunchAgents/com.jira-assistant.plist 2>/dev/null || true`
- On Linux: `systemctl --user stop jira-assistant 2>/dev/null || true`
- All errors silently ignored — the service may not be running or may not be registered yet.
- The function must already know `$OS` (set by `detect_platform()` in section-04), so `detect_platform` must be called before `stop_existing_service`.

Function stub:

```bash
stop_existing_service() {
  # Stops the running service on macOS (launchctl) or Linux (systemctl).
  # Silently ignores errors — service may not be installed or running.
  # Requires $OS to be set by detect_platform() before calling.
}
```

### `do_uninstall()` Function

Called via `main --uninstall`. Executes the full removal sequence and exits 0.

Sequence:
1. Call `detect_platform()` to set `$OS` — needed for OS-conditional logic.
2. Stop and unload service:
   - macOS: `launchctl unload ~/Library/LaunchAgents/com.jira-assistant.plist 2>/dev/null || true`
   - Linux: `systemctl --user stop jira-assistant 2>/dev/null || true` then `systemctl --user disable jira-assistant 2>/dev/null || true`
3. Remove service file:
   - macOS: `rm -f ~/Library/LaunchAgents/com.jira-assistant.plist`
   - Linux: `rm -f ~/.config/systemd/user/jira-assistant.service`
4. Remove binary — check both possible install dirs. Remove whichever exists:
   - `/usr/local/bin/jira-assistant`
   - `~/.local/bin/jira-assistant`
   - Use `rm -f` so that missing files are not errors.
5. Print advisory (do not remove automatically):
   - `"Config files at ~/.config/jira-assistant/ were left in place. Remove manually if desired."`
   - `"PATH entries added to shell RC files must be cleaned up manually."`
6. `exit 0`

Function stub:

```bash
do_uninstall() {
  # Removes the jira-assistant binary, service files, and service registration.
  # Config files (~/.config/jira-assistant/) are intentionally NOT removed.
  # PATH entries in shell RC files are NOT removed — user must clean those manually.
  # Exits 0 regardless of whether the binary or service was present.
}
```

### Wiring in `main()`

At the top of `main()`, before any other logic (version resolution, platform detection for install, etc.), check the first argument:

```bash
main() {
  if [[ "${1:-}" == "--uninstall" ]]; then
    do_uninstall
  fi
  if [[ "${1:-}" == "--help" ]]; then
    # print usage, exit 0
  fi
  # ... rest of install flow ...
}
```

The `do_uninstall` call exits internally, so no explicit `return` or branching is needed after it.

### Upgrade Path (Stop-Before-Replace)

When a user re-runs the install script over an existing install, the binary on disk may be in use by the running service. The upgrade flow in `main()` must stop the service before downloading a replacement.

Place this call in `main()` after platform detection and before downloading:

```bash
# In main(), after detect_platform(), before download:
stop_existing_service
```

This is safe even on a fresh install because `stop_existing_service` ignores all errors.

### Service File Paths (Reference, from section-05)

These paths are defined in `section-05-install-services` and reused verbatim here:

| Platform | Service file location |
|---|---|
| macOS | `~/Library/LaunchAgents/com.jira-assistant.plist` |
| Linux | `~/.config/systemd/user/jira-assistant.service` |

### Binary Install Locations (Reference, from section-04)

| Location | Condition |
|---|---|
| `/usr/local/bin/jira-assistant` | When `/usr/local/bin` was writable at install time |
| `~/.local/bin/jira-assistant` | Fallback when `/usr/local/bin` was not writable |

`do_uninstall()` must check and remove from both locations unconditionally, because it cannot know which one was used. Use `rm -f` on each — missing file is not an error.

## Manual Test Scenarios

These scenarios cannot be automated with Bats and require manual execution against a real or VM environment:

| Scenario | Expected result |
|---|---|
| `bash install.sh --uninstall` after clean install on macOS | Binary removed from install dir, plist removed, service unloaded, exit 0 |
| `bash install.sh --uninstall` after clean install on Linux | Binary removed, `.service` file removed, `systemctl --user stop && disable` called, exit 0 |
| `bash install.sh --uninstall` when nothing is installed | No errors, advisory messages printed, exit 0 |
| Re-run `bash install.sh` over existing install | Service stops, binary replaced, service restarts. No file-in-use errors. |
| `curl ... \| bash -s -- --uninstall` | Same as above — `main "$@"` passes `--uninstall` through correctly |
| `--uninstall` when binary is in `~/.local/bin` | Removes from `~/.local/bin`, not just `/usr/local/bin` |
| `--uninstall` after interrupted install (no service file) | Proceeds without error, removes binary if present |

## Implementation TODO List

1. [x] In `tests/install.bats`: add Bats test cases for all `do_uninstall()` and `stop_existing_service()` scenarios. Tests 56–64.
2. [x] In `install.sh`: `stop_existing_service()` was already implemented in section-04. Verified OS-conditional, all errors silently ignored.
3. [x] In `install.sh`: implemented `do_uninstall()` — calls `detect_platform`, `stop_existing_service`, removes service file (platform-conditional), removes binary from both locations, prints advisories, exits 0.
4. [x] In `install.sh`: `--uninstall` dispatch wired via `for arg` loop at top of `main()` (before version resolution).
5. [x] In `install.sh`: `stop_existing_service` already placed in `main()` after `detect_platform()` (line 279) and before download (line 290).
6. [x] `shellcheck -S error install.sh` passes cleanly.
7. [x] `bash -n install.sh` passes.

## Implementation Notes

- `do_uninstall` calls `stop_existing_service` for the stop step (code reuse), then `systemctl disable` (Linux only) directly before removing the unit file.
- 10 new tests added (tests 56–64): 7 for `do_uninstall`, 2 for `stop_existing_service`.
- Test helpers `_mock_uname_darwin` and `_mock_uname_linux` added at file scope to reduce duplication.
- Advisory message assertions added to "exits 0 gracefully" test.