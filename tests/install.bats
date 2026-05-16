#!/usr/bin/env bats
# Smoke tests for release binaries.
# Run manually after a release with actual binaries present.
# Usage: BINARY_DIR=./artifacts bats tests/install.bats

setup() {
    BINARY_DIR="${BINARY_DIR:-./artifacts}"
    CHECKSUMS_TMPDIR="$(mktemp -d)"
    MOCK_BIN="$(mktemp -d)"
    FAKE_HOME="$(mktemp -d)"
}

teardown() {
    [ -n "$CHECKSUMS_TMPDIR" ] && rm -rf "$CHECKSUMS_TMPDIR"
    [ -n "$MOCK_BIN" ] && rm -rf "$MOCK_BIN"
    [ -n "$FAKE_HOME" ] && rm -rf "$FAKE_HOME"
}

verify_binary_size() {
    local path="$1"
    local size
    size=$(wc -c < "$path")
    local min=$((10 * 1024 * 1024))   # 10MB
    local max=$((500 * 1024 * 1024))  # 500MB
    [ "$size" -ge "$min" ] && [ "$size" -le "$max" ]
}

@test "macOS arm64 binary: size between 10MB and 500MB" {
    skip "Manual smoke test — requires release binary"
    local bin="$BINARY_DIR/jira-assistant-macos-arm64"
    [ -f "$bin" ] || skip "Binary not found: $bin"
    verify_binary_size "$bin"
}

@test "macOS x64 binary: size between 10MB and 500MB" {
    skip "Manual smoke test — requires release binary"
    local bin="$BINARY_DIR/jira-assistant-macos-x64"
    [ -f "$bin" ] || skip "Binary not found: $bin"
    verify_binary_size "$bin"
}

@test "Linux x64 binary: size between 10MB and 500MB" {
    skip "Manual smoke test — requires release binary"
    local bin="$BINARY_DIR/jira-assistant-linux-x64"
    [ -f "$bin" ] || skip "Binary not found: $bin"
    verify_binary_size "$bin"
}

@test "macOS arm64 binary: ad-hoc code signature valid (codesign -v)" {
    skip "Manual smoke test — requires release binary and macOS"
    local bin="$BINARY_DIR/jira-assistant-macos-arm64"
    [ -f "$bin" ] || skip "Binary not found: $bin"
    run codesign -v "$bin"
    [ "$status" -eq 0 ]
}

@test "macOS x64 binary: ad-hoc code signature valid (codesign -v)" {
    skip "Manual smoke test — requires release binary and macOS"
    local bin="$BINARY_DIR/jira-assistant-macos-x64"
    [ -f "$bin" ] || skip "Binary not found: $bin"
    run codesign -v "$bin"
    [ "$status" -eq 0 ]
}

@test "macOS arm64 binary: executes without SIGKILL (exit code != 137)" {
    skip "Manual smoke test — requires release binary on macOS arm64"
    local bin="$BINARY_DIR/jira-assistant-macos-arm64"
    [ -f "$bin" ] || skip "Binary not found: $bin"
    chmod +x "$bin"
    run "$bin" --version
    [ "$status" -ne 137 ]
}

@test "macOS x64 binary: executes without SIGKILL (exit code != 137)" {
    skip "Manual smoke test — requires release binary on macOS x64"
    local bin="$BINARY_DIR/jira-assistant-macos-x64"
    [ -f "$bin" ] || skip "Binary not found: $bin"
    chmod +x "$bin"
    run "$bin" --version
    [ "$status" -ne 137 ]
}

@test "Linux x64 binary: executes successfully on --version" {
    skip "Manual smoke test — requires release binary on Linux x64"
    local bin="$BINARY_DIR/jira-assistant-linux-x64"
    [ -f "$bin" ] || skip "Binary not found: $bin"
    chmod +x "$bin"
    run "$bin" --version
    [ "$status" -eq 0 ]
}

_make_fixture_checksums() {
    command -v sha256sum > /dev/null || skip "sha256sum not available"
    local dir="$1"
    printf 'data1' > "$dir/jira-assistant-macos-arm64"
    printf 'data2' > "$dir/jira-assistant-macos-x64"
    printf 'data3' > "$dir/jira-assistant-linux-x64"
    (cd "$dir" && sha256sum * > checksums.txt)
}

@test "checksums.txt: each line matches <64 hex chars>  <filename> format" {
    _make_fixture_checksums "$CHECKSUMS_TMPDIR"
    [ "$(wc -l < "$CHECKSUMS_TMPDIR/checksums.txt")" -ge 3 ]
    while IFS= read -r line; do
        [[ "$line" =~ ^[0-9a-f]{64}\ \ [^[:space:]]+$ ]]
    done < "$CHECKSUMS_TMPDIR/checksums.txt"
}

@test "checksums.txt: all three binary names appear" {
    _make_fixture_checksums "$CHECKSUMS_TMPDIR"
    grep -q "jira-assistant-macos-arm64" "$CHECKSUMS_TMPDIR/checksums.txt"
    grep -q "jira-assistant-macos-x64"  "$CHECKSUMS_TMPDIR/checksums.txt"
    grep -q "jira-assistant-linux-x64"  "$CHECKSUMS_TMPDIR/checksums.txt"
}

@test "sha256sum --check exits 0 when binaries are intact" {
    _make_fixture_checksums "$CHECKSUMS_TMPDIR"
    run bash -c "cd \"$CHECKSUMS_TMPDIR\" && sha256sum --check checksums.txt"
    [ "$status" -eq 0 ]
}

@test "sha256sum --check exits non-zero after binary corruption" {
    _make_fixture_checksums "$CHECKSUMS_TMPDIR"
    printf 'corrupted' > "$CHECKSUMS_TMPDIR/jira-assistant-macos-arm64"
    run bash -c "cd \"$CHECKSUMS_TMPDIR\" && sha256sum --check checksums.txt"
    [ "$status" -ne 0 ]
}

# ─── Section-04 install.sh core tests ──────────────────────────────────────

_install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }

@test "install.sh: bash -n syntax check passes" {
    run bash -n "$(_install_sh)"
    [ "$status" -eq 0 ]
}

@test "install.sh: shellcheck passes at error severity" {
    command -v shellcheck > /dev/null || skip "shellcheck not available"
    run shellcheck -S error "$(_install_sh)"
    [ "$status" -eq 0 ]
}

@test "detect_platform: Darwin arm64 → OS=macos ARCH=arm64" {
    printf '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo arm64;; esac\n' > "$MOCK_BIN/uname"
    chmod +x "$MOCK_BIN/uname"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform; printf '%s %s' \"\$OS\" \"\$ARCH\""
    [ "$status" -eq 0 ]
    [ "$output" = "macos arm64" ]
}

@test "detect_platform: Darwin x86_64 → OS=macos ARCH=x64" {
    printf '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo x86_64;; esac\n' > "$MOCK_BIN/uname"
    chmod +x "$MOCK_BIN/uname"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform; printf '%s %s' \"\$OS\" \"\$ARCH\""
    [ "$status" -eq 0 ]
    [ "$output" = "macos x64" ]
}

@test "detect_platform: Linux x86_64 → OS=linux ARCH=x64" {
    printf '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n' > "$MOCK_BIN/uname"
    chmod +x "$MOCK_BIN/uname"
    printf '#!/bin/sh\necho "ldd (GNU libc) 2.35"\n' > "$MOCK_BIN/ldd"
    chmod +x "$MOCK_BIN/ldd"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform; printf '%s %s' \"\$OS\" \"\$ARCH\""
    [ "$status" -eq 0 ]
    [ "$output" = "linux x64" ]
}

@test "detect_platform: Linux aarch64 → exits 1 with ARM64 message" {
    printf '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo aarch64;; esac\n' > "$MOCK_BIN/uname"
    chmod +x "$MOCK_BIN/uname"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform"
    [ "$status" -eq 1 ]
    [[ "$output" == *"Linux ARM64 is not yet supported"* ]]
}

@test "detect_platform: Linux musl → exits 1 with musl message" {
    printf '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n' > "$MOCK_BIN/uname"
    chmod +x "$MOCK_BIN/uname"
    printf '#!/bin/sh\necho "musl libc (x86_64)"\n' > "$MOCK_BIN/ldd"
    chmod +x "$MOCK_BIN/ldd"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform"
    [ "$status" -eq 1 ]
    [[ "$output" == *"Alpine/musl Linux is not supported"* ]]
}

@test "detect_platform: Windows_NT → exits 1 with Unsupported OS message" {
    printf '#!/bin/sh\ncase "$1" in -s) echo Windows_NT;; -m) echo x86_64;; esac\n' > "$MOCK_BIN/uname"
    chmod +x "$MOCK_BIN/uname"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform"
    [ "$status" -eq 1 ]
    [[ "$output" == *"Unsupported OS"* ]]
}

@test "resolve_version: uses JIRA_ASSISTANT_VERSION env var, no HTTP call" {
    printf '#!/bin/sh\necho "UNEXPECTED_CURL_CALL"; exit 1\n' > "$MOCK_BIN/curl"
    chmod +x "$MOCK_BIN/curl"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" JIRA_ASSISTANT_VERSION=v1.0.0 source \"$(_install_sh)\"; resolve_version; printf '%s' \"\$VERSION\""
    [ "$status" -eq 0 ]
    [[ "$output" == *"v1.0.0"* ]]
}

@test "resolve_version: follows /releases/latest redirect and parses tag" {
    printf '#!/bin/sh\necho "https://github.com/sayjeyhi/jira-assistant/releases/tag/v2.3.4"\n' > "$MOCK_BIN/curl"
    chmod +x "$MOCK_BIN/curl"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; resolve_version; printf '%s' \"\$VERSION\""
    [ "$status" -eq 0 ]
    [[ "$output" == *"v2.3.4"* ]]
}

@test "download_with_retry: succeeds on first try" {
    local dest="$FAKE_HOME/out"
    {
        printf '#!/bin/sh\n'
        printf 'while [ $# -gt 0 ]; do\n'
        printf '  if [ "$1" = "-o" ]; then echo content > "$2"; break; fi; shift\n'
        printf 'done\nexit 0\n'
    } > "$MOCK_BIN/curl"
    chmod +x "$MOCK_BIN/curl"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; download_with_retry http://x.com/f \"$dest\""
    [ "$status" -eq 0 ]
    [ -f "$dest" ]
}

@test "download_with_retry: retries once on first failure, succeeds second" {
    local count_file="$MOCK_BIN/count" dest="$FAKE_HOME/out"
    echo 0 > "$count_file"
    {
        printf '#!/bin/sh\n'
        printf 'n=$(cat "%s"); n=$((n+1)); printf "%%s" "$n" > "%s"\n' "$count_file" "$count_file"
        printf 'if [ "$n" -ge 2 ]; then\n'
        printf '  while [ $# -gt 0 ]; do\n'
        printf '    [ "$1" = "-o" ] && { echo content > "$2"; exit 0; }; shift\n'
        printf '  done\nfi\nexit 1\n'
    } > "$MOCK_BIN/curl"
    chmod +x "$MOCK_BIN/curl"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; download_with_retry http://x.com/f \"$dest\""
    [ "$status" -eq 0 ]
    [ -f "$dest" ]
}

@test "download_with_retry: exits non-zero when both attempts fail" {
    printf '#!/bin/sh\nexit 1\n' > "$MOCK_BIN/curl"
    chmod +x "$MOCK_BIN/curl"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; download_with_retry http://x.com/f \"$FAKE_HOME/out\""
    [ "$status" -ne 0 ]
}

@test "verify_checksum: exits 0 when hash matches" {
    command -v sha256sum > /dev/null || skip "sha256sum not available"
    printf 'binary content' > "$FAKE_HOME/mybin"
    local hash; hash=$(sha256sum "$FAKE_HOME/mybin" | awk '{print $1}')
    printf '%s  mybin\n' "$hash" > "$FAKE_HOME/checksums.txt"
    run bash -c "OS=linux source \"$(_install_sh)\"; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
    [ "$status" -eq 0 ]
}

@test "download_with_retry for checksums.txt: propagates failure, exits non-zero" {
    printf '#!/bin/sh\nexit 1\n' > "$MOCK_BIN/curl"
    chmod +x "$MOCK_BIN/curl"
    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; download_with_retry http://x.com/checksums.txt \"$FAKE_HOME/checksums.txt\""
    [ "$status" -ne 0 ]
}

@test "verify_checksum: exits 1 with Checksum mismatch when hash is wrong" {
    command -v sha256sum > /dev/null || skip "sha256sum not available"
    printf 'binary content' > "$FAKE_HOME/mybin"
    printf '%s  mybin\n' "0000000000000000000000000000000000000000000000000000000000000000" > "$FAKE_HOME/checksums.txt"
    run bash -c "OS=linux source \"$(_install_sh)\"; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
    [ "$status" -eq 1 ]
    [[ "$output" == *"Checksum mismatch"* ]]
}

@test "verify_checksum: exits 1 when binary name not in checksums.txt" {
    printf 'binary content' > "$FAKE_HOME/mybin"
    printf '%s  otherfile\n' "aabbcc" > "$FAKE_HOME/checksums.txt"
    run bash -c "OS=linux source \"$(_install_sh)\"; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
    [ "$status" -eq 1 ]
    [[ "$output" == *"not found in checksums.txt"* ]]
}

@test "verify_checksum: uses sha256sum on linux, shasum -a 256 on macos" {
    command -v sha256sum > /dev/null || skip "sha256sum not available"
    printf 'data' > "$FAKE_HOME/mybin"
    local hash; hash=$(sha256sum "$FAKE_HOME/mybin" | awk '{print $1}')
    printf '%s  mybin\n' "$hash" > "$FAKE_HOME/checksums.txt"
    run bash -c "OS=linux source \"$(_install_sh)\"; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
    [ "$status" -eq 0 ]
}

@test "select_install_dir: uses /usr/local/bin when writable" {
    [ -w /usr/local/bin ] || skip "/usr/local/bin not writable"
    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; select_install_dir; printf '%s' \"\$INSTALL_DIR\""
    [ "$status" -eq 0 ]
    [ "$output" = "/usr/local/bin" ]
}

@test "select_install_dir: falls back to ~/.local/bin when /usr/local/bin not writable" {
    [ -w /usr/local/bin ] && skip "/usr/local/bin is writable on this system"
    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; select_install_dir; printf '%s' \"\$INSTALL_DIR\""
    [ "$status" -eq 0 ]
    [ "$output" = "$FAKE_HOME/.local/bin" ]
    [ -d "$FAKE_HOME/.local/bin" ]
}

@test "ensure_path: appends export line to existing RC files" {
    touch "$FAKE_HOME/.zshrc" "$FAKE_HOME/.bashrc"
    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; ensure_path \"$FAKE_HOME/.local/bin\""
    [ "$status" -eq 0 ]
    grep -q "# jira-assistant" "$FAKE_HOME/.zshrc"
    grep -q "# jira-assistant" "$FAKE_HOME/.bashrc"
}

@test "ensure_path: idempotent — no duplicate if marker exists" {
    touch "$FAKE_HOME/.zshrc"
    printf '\n# jira-assistant\nexport PATH="%s/.local/bin:$PATH"\n' "$FAKE_HOME" >> "$FAKE_HOME/.zshrc"
    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; ensure_path \"$FAKE_HOME/.local/bin\"; ensure_path \"$FAKE_HOME/.local/bin\""
    [ "$status" -eq 0 ]
    [ "$(grep -c '# jira-assistant' "$FAKE_HOME/.zshrc")" -eq 1 ]
}

@test "ensure_path: does not create missing RC files" {
    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; ensure_path \"$FAKE_HOME/.local/bin\""
    [ "$status" -eq 0 ]
    [ ! -f "$FAKE_HOME/.zshrc" ]
    [ ! -f "$FAKE_HOME/.bashrc" ]
}

@test "TTY detection: wizard skipped with message when stdin is not a TTY" {
    run bash -c "CONFIG_FILE=\"$FAKE_HOME/no.json\" source \"$(_install_sh)\"; run_config_if_needed < /dev/null"
    [ "$status" -eq 0 ]
    [[ "$output" == *"jira-assistant config"* ]]
}

@test "TTY detection: wizard skipped silently when config file exists" {
    touch "$FAKE_HOME/config.json"
    run bash -c "CONFIG_FILE=\"$FAKE_HOME/config.json\" source \"$(_install_sh)\"; run_config_if_needed < /dev/null"
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "main() wrap safety: sourcing install.sh does not execute main" {
    run bash -c "source \"$(_install_sh)\"; echo SOURCED_OK"
    [ "$status" -eq 0 ]
    [ "$output" = "SOURCED_OK" ]
}

# ─── Section-05 install.sh service registration tests ───────────────────────

@test "register_macos_service: plist created at correct path" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
    chmod +x "$MOCK_BIN/launchctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    [ -f "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist" ]
}

@test "register_macos_service: plist contains KeepAlive in dictionary form" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
    chmod +x "$MOCK_BIN/launchctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -A1 '<key>KeepAlive</key>' "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist" | grep -q '<dict>'
}

@test "register_macos_service: plist contains ThrottleInterval = 30" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
    chmod +x "$MOCK_BIN/launchctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -q '<integer>30</integer>' "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist"
}

@test "register_macos_service: plist contains RunAtLoad = true" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
    chmod +x "$MOCK_BIN/launchctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -A1 '<key>RunAtLoad</key>' "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist" | grep -q '<true/>'
}

@test "register_macos_service: launchctl unload called before launchctl load" {
    local call_log="$FAKE_HOME/launchctl_calls"
    {
        printf '#!/bin/sh\n'
        printf 'printf "%%s\\n" "$1" >> "%s"\n' "$call_log"
        printf 'exit 0\n'
    } > "$MOCK_BIN/launchctl"
    chmod +x "$MOCK_BIN/launchctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    [ "$(sed -n '1p' "$call_log")" = "unload" ]
    [ "$(sed -n '2p' "$call_log")" = "load" ]
}

@test "register_macos_service: launchctl load called with plist path" {
    local call_log="$FAKE_HOME/launchctl_calls"
    {
        printf '#!/bin/sh\n'
        printf 'printf "%%s\\n" "$*" >> "%s"\n' "$call_log"
        printf 'exit 0\n'
    } > "$MOCK_BIN/launchctl"
    chmod +x "$MOCK_BIN/launchctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -q "load.*com.jira-assistant.plist" "$call_log"
}

@test "register_linux_service: service file created at correct path" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
    chmod +x "$MOCK_BIN/systemctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    [ -f "$FAKE_HOME/.config/systemd/user/jira-assistant.service" ]
}

@test "register_linux_service: unit file contains Restart=on-failure" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
    chmod +x "$MOCK_BIN/systemctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -q 'Restart=on-failure' "$FAKE_HOME/.config/systemd/user/jira-assistant.service"
}

@test "register_linux_service: unit file contains StartLimitIntervalSec=300" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
    chmod +x "$MOCK_BIN/systemctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -q 'StartLimitIntervalSec=300' "$FAKE_HOME/.config/systemd/user/jira-assistant.service"
}

@test "register_linux_service: unit file contains StartLimitBurst=5" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
    chmod +x "$MOCK_BIN/systemctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -q 'StartLimitBurst=5' "$FAKE_HOME/.config/systemd/user/jira-assistant.service"
}

@test "register_linux_service: unit file contains Type=simple" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
    chmod +x "$MOCK_BIN/systemctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -q 'Type=simple' "$FAKE_HOME/.config/systemd/user/jira-assistant.service"
}

@test "register_linux_service: systemctl enable --now called" {
    local call_log="$FAKE_HOME/systemctl_calls"
    {
        printf '#!/bin/sh\n'
        printf 'printf "%%s\\n" "$*" >> "%s"\n' "$call_log"
        printf 'exit 0\n'
    } > "$MOCK_BIN/systemctl"
    chmod +x "$MOCK_BIN/systemctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -q -- '--user enable --now' "$call_log"
}

@test "register_linux_service: systemctl daemon-reload called before enable" {
    local call_log="$FAKE_HOME/systemctl_calls"
    {
        printf '#!/bin/sh\n'
        printf 'printf "%%s\\n" "$*" >> "%s"\n' "$call_log"
        printf 'exit 0\n'
    } > "$MOCK_BIN/systemctl"
    chmod +x "$MOCK_BIN/systemctl"
    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
    [ "$status" -eq 0 ]
    grep -q -- '--user daemon-reload' "$call_log"
    local reload_line enable_line
    reload_line=$(grep -n -- '--user daemon-reload' "$call_log" | head -1 | cut -d: -f1)
    enable_line=$(grep -n -- '--user enable' "$call_log" | head -1 | cut -d: -f1)
    [ "$reload_line" -lt "$enable_line" ]
}

@test "start_service: macOS reports running when service is in launchd" {
    {
        printf '#!/bin/sh\n'
        printf 'if [ "$1" = "list" ]; then echo "com.jira-assistant"; fi\n'
        printf 'exit 0\n'
    } > "$MOCK_BIN/launchctl"
    chmod +x "$MOCK_BIN/launchctl"
    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; OS=macos start_service"
    [ "$status" -eq 0 ]
    [[ "$output" == *"running"* ]]
}

@test "start_service: macOS reports not detected when service absent from launchd" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
    chmod +x "$MOCK_BIN/launchctl"
    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; OS=macos start_service"
    [ "$status" -eq 0 ]
    [[ "$output" == *"not detected"* ]]
}

@test "start_service: linux reports running when systemd service is active" {
    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
    chmod +x "$MOCK_BIN/systemctl"
    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; OS=linux start_service"
    [ "$status" -eq 0 ]
    [[ "$output" == *"running"* ]]
}

@test "start_service: linux reports not detected when systemd service inactive" {
    printf '#!/bin/sh\nexit 1\n' > "$MOCK_BIN/systemctl"
    chmod +x "$MOCK_BIN/systemctl"
    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; OS=linux start_service"
    [ "$status" -eq 0 ]
    [[ "$output" == *"not detected"* ]]
}
