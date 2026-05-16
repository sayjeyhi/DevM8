diff --git a/install.sh b/install.sh
index 2bfa1d4..06062f9 100644
--- a/install.sh
+++ b/install.sh
@@ -243,7 +243,19 @@ start_service() {
 }
 
 do_uninstall() {
-  : # implemented in section-06-install-uninstall
+  detect_platform
+  stop_existing_service
+  if [[ "$OS" == "macos" ]]; then
+    rm -f "${HOME}/Library/LaunchAgents/com.jira-assistant.plist"
+  else
+    systemctl --user disable jira-assistant 2>/dev/null || true
+    rm -f "${HOME}/.config/systemd/user/jira-assistant.service"
+  fi
+  rm -f /usr/local/bin/jira-assistant
+  rm -f "${HOME}/.local/bin/jira-assistant"
+  echo "Config files at ~/.config/jira-assistant/ were left in place. Remove manually if desired."
+  echo "PATH entries added to shell RC files must be cleaned up manually."
+  exit 0
 }
 
 main() {
diff --git a/tests/install.bats b/tests/install.bats
index 69b10c8..8ec653f 100644
--- a/tests/install.bats
+++ b/tests/install.bats
@@ -512,3 +512,122 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
     [ "$status" -eq 0 ]
     [[ "$output" == *"not detected"* ]]
 }
+
+# ─── Section-06 install.sh uninstall tests ──────────────────────────────────
+
+_mock_uname_darwin() {
+    printf '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo arm64;; esac\n' > "$MOCK_BIN/uname"
+    chmod +x "$MOCK_BIN/uname"
+}
+
+_mock_uname_linux() {
+    printf '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n' > "$MOCK_BIN/uname"
+    chmod +x "$MOCK_BIN/uname"
+}
+
+@test "do_uninstall: removes ~/.local/bin/jira-assistant" {
+    mkdir -p "$FAKE_HOME/.local/bin"
+    touch "$FAKE_HOME/.local/bin/jira-assistant"
+    _mock_uname_darwin
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; do_uninstall"
+    [ "$status" -eq 0 ]
+    [ ! -f "$FAKE_HOME/.local/bin/jira-assistant" ]
+}
+
+@test "do_uninstall: removes ~/Library/LaunchAgents/com.jira-assistant.plist on macOS" {
+    mkdir -p "$FAKE_HOME/Library/LaunchAgents"
+    touch "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist"
+    _mock_uname_darwin
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; do_uninstall"
+    [ "$status" -eq 0 ]
+    [ ! -f "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist" ]
+}
+
+@test "do_uninstall: removes ~/.config/systemd/user/jira-assistant.service on Linux" {
+    mkdir -p "$FAKE_HOME/.config/systemd/user"
+    touch "$FAKE_HOME/.config/systemd/user/jira-assistant.service"
+    _mock_uname_linux
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
+    chmod +x "$MOCK_BIN/systemctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; do_uninstall"
+    [ "$status" -eq 0 ]
+    [ ! -f "$FAKE_HOME/.config/systemd/user/jira-assistant.service" ]
+}
+
+@test "do_uninstall: does NOT remove ~/.config/jira-assistant/" {
+    mkdir -p "$FAKE_HOME/.config/jira-assistant"
+    touch "$FAKE_HOME/.config/jira-assistant/config.json"
+    _mock_uname_darwin
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; do_uninstall"
+    [ "$status" -eq 0 ]
+    [ -f "$FAKE_HOME/.config/jira-assistant/config.json" ]
+}
+
+@test "do_uninstall: exits 0 gracefully when nothing is installed" {
+    _mock_uname_darwin
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; do_uninstall"
+    [ "$status" -eq 0 ]
+}
+
+@test "do_uninstall: calls launchctl unload before removing plist" {
+    local call_log="$FAKE_HOME/call_log"
+    mkdir -p "$FAKE_HOME/Library/LaunchAgents"
+    touch "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist"
+    _mock_uname_darwin
+    {
+        printf '#!/bin/sh\n'
+        printf 'printf "launchctl %%s\\n" "$*" >> "%s"\n' "$call_log"
+        printf 'exit 0\n'
+    } > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    {
+        printf '#!/bin/sh\n'
+        printf 'printf "rm %%s\\n" "$*" >> "%s"\n' "$call_log"
+        printf 'exit 0\n'
+    } > "$MOCK_BIN/rm"
+    chmod +x "$MOCK_BIN/rm"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; do_uninstall"
+    [ "$status" -eq 0 ]
+    local unload_line rm_line
+    unload_line=$(grep -n "launchctl unload" "$call_log" | head -1 | cut -d: -f1)
+    rm_line=$(grep -n "rm.*com.jira-assistant.plist" "$call_log" | head -1 | cut -d: -f1)
+    [ "$unload_line" -lt "$rm_line" ]
+}
+
+@test "do_uninstall: removes /usr/local/bin/jira-assistant" {
+    local call_log="$FAKE_HOME/rm_calls"
+    _mock_uname_darwin
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    {
+        printf '#!/bin/sh\n'
+        printf 'printf "%%s\\n" "$*" >> "%s"\n' "$call_log"
+        printf 'exit 0\n'
+    } > "$MOCK_BIN/rm"
+    chmod +x "$MOCK_BIN/rm"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; do_uninstall"
+    [ "$status" -eq 0 ]
+    grep -q "/usr/local/bin/jira-assistant" "$call_log"
+}
+
+@test "stop_existing_service: calls launchctl unload on macOS and ignores errors" {
+    printf '#!/bin/sh\nexit 1\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; OS=macos stop_existing_service"
+    [ "$status" -eq 0 ]
+}
+
+@test "stop_existing_service: calls systemctl --user stop on Linux and ignores errors" {
+    printf '#!/bin/sh\nexit 1\n' > "$MOCK_BIN/systemctl"
+    chmod +x "$MOCK_BIN/systemctl"
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; OS=linux stop_existing_service"
+    [ "$status" -eq 0 ]
+}
