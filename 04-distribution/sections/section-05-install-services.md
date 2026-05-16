Now I have all the information needed. Let me generate the section content for `section-05-install-services`.

# Section 05: install.sh — Service Registration

## Overview

This section implements the service registration functions in `install.sh`. These functions are called from `main()` after the binary is installed and before the config wizard runs. The dependency is `section-04-install-core`, which must be completed first as it provides the overall script structure, `$OS` variable, `$INSTALL_DIR`, and the `main()` wrapper pattern.

The two functions to implement are:

- `register_macos_service(binary_path)` — writes a launchd plist and loads it
- `register_linux_service(binary_path)` — writes a systemd user unit and enables it

A third helper, `start_service()`, is called from `main()` after the config wizard and simply re-starts the service (or is a no-op if `--now` was already used in `enable`).

---

## Files to Create / Modify

- `/install.sh` — add `register_macos_service`, `register_linux_service`, and `start_service` function definitions inside the script body (before `main()`). These are called from inside `main()` after `install_binary()` completes.

---

## Dependencies

- `section-04-install-core` must be complete. The functions here integrate into the `main()` flow written in that section. Specifically:
  - `$OS` is already set by `detect_platform()` (values: `macos` or `linux`)
  - `$INSTALL_DIR` is set by `select_install_dir()`
  - The `main()` wrapper and `set -euo pipefail` + `trap` are already present

Do not re-implement any of those here.

---

## Tests First

Tests live in `tests/install.bats`. Write stubs before implementing the functions. The standard Bats mocking pattern overrides commands by defining shell functions with the same name and prepending a temp directory to `$PATH`.

### Bats test stubs — `register_macos_service`

```bash
@test "register_macos_service: plist created at correct path" {
  # mock launchctl to no-op
  # call register_macos_service /usr/local/bin/jira-assistant
  # assert file exists at ~/Library/LaunchAgents/com.jira-assistant.plist
}

@test "register_macos_service: plist contains KeepAlive in dictionary form" {
  # assert plist contains <key>KeepAlive</key><dict> (not <true/>)
}

@test "register_macos_service: plist contains ThrottleInterval = 30" {
  # assert plist contains <key>ThrottleInterval</key><integer>30</integer>
}

@test "register_macos_service: plist contains RunAtLoad = true" {
  # assert plist contains <key>RunAtLoad</key><true/>
}

@test "register_macos_service: launchctl unload called before launchctl load" {
  # use ordered mock that records calls
  # assert unload appears before load in call log
}

@test "register_macos_service: launchctl load called with plist path" {
  # assert launchctl load ~/Library/LaunchAgents/com.jira-assistant.plist
}
```

### Bats test stubs — `register_linux_service`

```bash
@test "register_linux_service: service file created at correct path" {
  # mock systemctl to no-op
  # call register_linux_service /usr/local/bin/jira-assistant
  # assert file exists at ~/.config/systemd/user/jira-assistant.service
}

@test "register_linux_service: unit file contains Restart=on-failure" {
  # grep unit file for Restart=on-failure
}

@test "register_linux_service: unit file contains StartLimitIntervalSec=300" {
  # grep unit file for StartLimitIntervalSec=300
}

@test "register_linux_service: unit file contains StartLimitBurst=5" {
  # grep unit file for StartLimitBurst=5
}

@test "register_linux_service: unit file contains Type=simple" {
  # grep unit file for Type=simple
}

@test "register_linux_service: systemctl enable --now called" {
  # assert systemctl --user enable --now jira-assistant was invoked
}
```

### Bats mocking pattern (reference)

```bash
setup() {
  MOCK_DIR="$(mktemp -d)"
  export PATH="$MOCK_DIR:$PATH"
  # write mock executables to $MOCK_DIR
}

teardown() {
  rm -rf "$MOCK_DIR"
}
```

To record call order, mock commands write to a log file and the test asserts on `grep` output of that file.

---

## Implementation Details

### `register_macos_service(binary_path)`

The function signature inside `install.sh`:

```bash
register_macos_service() {
  local binary_path="$1"
  local plist_dir="$HOME/Library/LaunchAgents"
  local plist_path="$plist_dir/com.jira-assistant.plist"
  # mkdir -p plist_dir
  # launchctl unload (ignore errors) — upgrade-safe
  # write plist to plist_path
  # launchctl load plist_path
}
```

The plist content to write:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.jira-assistant</string>
    <key>ProgramArguments</key>
    <array>
        <string>BINARY_PATH_PLACEHOLDER</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>Crashed</key>
        <true/>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>30</integer>
    <key>StandardOutPath</key>
    <string>HOME_PLACEHOLDER/Library/Logs/jira-assistant.log</string>
    <key>StandardErrorPath</key>
    <string>HOME_PLACEHOLDER/Library/Logs/jira-assistant.log</string>
</dict>
</plist>
```

Replace `BINARY_PATH_PLACEHOLDER` with `"$binary_path"` and `HOME_PLACEHOLDER` with `"$HOME"` using a heredoc (preferred) or `sed`. Use a heredoc with the log directory expanded at write time.

Key design decisions:
- `KeepAlive` uses dictionary form `{Crashed = true; SuccessfulExit = false;}` — this means the daemon restarts on crash but NOT on clean exit. A simple `<true/>` would restart even on intentional stop, causing a restart loop the user cannot easily break.
- `ThrottleInterval = 30` prevents runaway crash loops: if misconfigured, the service will not restart more than once every 30 seconds.
- `launchctl unload ... 2>/dev/null || true` before writing the new plist — this handles the upgrade case where a plist is already loaded. Silencing errors is correct here; the plist may not exist on a fresh install.
- Log files go to `~/Library/Logs/jira-assistant.log` (standard macOS location). The directory always exists on macOS.

Load sequence inside the function:
1. `mkdir -p "$plist_dir"`
2. `launchctl unload "$plist_path" 2>/dev/null || true`
3. Write plist heredoc to `"$plist_path"`
4. `launchctl load "$plist_path"`

### `register_linux_service(binary_path)`

The function signature:

```bash
register_linux_service() {
  local binary_path="$1"
  local unit_dir="$HOME/.config/systemd/user"
  local unit_path="$unit_dir/jira-assistant.service"
  # mkdir -p unit_dir
  # write unit file
  # systemctl --user daemon-reload
  # systemctl --user enable --now jira-assistant
  # print loginctl linger advisory
}
```

The unit file content:

```ini
[Unit]
Description=Jira Assistant Telegram Bot
After=network.target

[Service]
Type=simple
ExecStart=BINARY_PATH_PLACEHOLDER start
Restart=on-failure
RestartSec=5
StartLimitIntervalSec=300
StartLimitBurst=5

[Install]
WantedBy=default.target
```

Replace `BINARY_PATH_PLACEHOLDER` with the value of `"$binary_path"`.

Key design decisions:
- `Type=simple` — the binary runs in the foreground (the daemon loop started by `jira-assistant start` does not fork).
- `Restart=on-failure` + `RestartSec=5` — restarts after 5 seconds on non-zero exit. Does not restart on clean exit (code 0).
- `StartLimitIntervalSec=300` + `StartLimitBurst=5` — if the service fails and restarts 5 times within 5 minutes, systemd stops retrying automatically. This prevents a misconfigured service from hammering the system.
- `After=network.target` — ensures the network is ready before the bot starts; important because the bot makes outbound HTTPS connections immediately on start.
- `systemctl --user daemon-reload` before enable — required when a unit file is written for the first time or changed on disk.
- `systemctl --user enable --now jira-assistant` — enables the service to start at login AND starts it immediately in one command.

### `loginctl enable-linger` advisory

After `register_linux_service()` completes, print this message (not execute it automatically):

```
Optional: to start jira-assistant at boot even when you are not logged in, run:
  loginctl enable-linger $USER
Note: this may require sudo on some systems.
```

This is printed from inside `register_linux_service()`, not from `main()`, so the message is close to the service registration output in the terminal.

### `start_service()` stub

`start_service()` is called from `main()` after the config wizard. On Linux, `--now` in `enable --now` already started the service, so this is a no-op or a status check. On macOS, `launchctl load` already started the service via `RunAtLoad`. The function can be a lightweight status reporter:

```bash
start_service() {
  # On macOS: launchctl list | grep com.jira-assistant
  # On Linux: systemctl --user is-active jira-assistant
  # Print whether the service is running
}
```

Full implementation is intentional minimal — the services should already be running by this point.

### Placement in `main()`

After `install_binary()` and `strip_quarantine()` succeed, add:

```bash
if [[ "$OS" == "macos" ]]; then
  register_macos_service "$INSTALL_DIR/jira-assistant"
else
  register_linux_service "$INSTALL_DIR/jira-assistant"
fi
```

Then the config wizard (`run_config_if_needed`) runs, and finally `start_service` and `print_success`.

---

## Error Handling Notes

- `launchctl load` can fail if the binary path is wrong or the plist is malformed. Since `set -euo pipefail` is active, any failure will abort the script with a clear exit code. Do not suppress errors on the `load` call (only suppress errors on the preceding `unload`).
- `systemctl --user` requires a running user session (D-Bus). On headless Linux CI machines this may not be available. Tests must mock `systemctl` to avoid this dependency.
- Both functions receive `binary_path` as an absolute path (set by `select_install_dir()` in section-04). Do not call `which jira-assistant` inside these functions.

---

## Acceptance Checklist

- [x] `~/Library/LaunchAgents/com.jira-assistant.plist` is created with correct content on macOS
- [x] `KeepAlive` is in dictionary form, not a simple boolean
- [x] `ThrottleInterval` is 30
- [x] `launchctl unload` is called before `launchctl load` (upgrade-safe)
- [x] `~/.config/systemd/user/jira-assistant.service` is created with correct content on Linux
- [x] `StartLimitIntervalSec=300` and `StartLimitBurst=5` are present
- [x] `systemctl --user daemon-reload && enable --now` is called
- [x] `loginctl enable-linger` advisory is printed but not executed
- [x] All Bats test stubs in `tests/install.bats` pass with mocked commands
- [x] `shellcheck -S error install.sh` reports no errors after adding these functions

## Implementation Notes (actual vs planned)

- Added `mkdir -p "$HOME/Library/Logs"` before plist write (not in plan; needed on fresh macOS)
- `${USER}` replaced with `$(id -un)` for POSIX portability
- `echo ""` replaced with bare `echo`
- Test pattern uses `export HOME=...` before `source` (not command-prefix assignment, which reverts after source for shell builtins)
- 17 total new tests added (vs 12 planned stubs): added daemon-reload ordering test and 4 `start_service` tests covering all branches
- Tests 39–55 in `tests/install.bats`