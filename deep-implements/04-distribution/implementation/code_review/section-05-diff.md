diff --git a/install.sh b/install.sh
index e75fb11..b297756 100644
--- a/install.sh
+++ b/install.sh
@@ -157,15 +157,88 @@ print_success() {
 }
 
 register_macos_service() {
-  : # implemented in section-05-install-services
+  local binary_path="$1"
+  local plist_dir="$HOME/Library/LaunchAgents"
+  local plist_path="$plist_dir/com.jira-assistant.plist"
+  mkdir -p "$plist_dir"
+  launchctl unload "$plist_path" 2>/dev/null || true
+  cat > "$plist_path" <<EOF
+<?xml version="1.0" encoding="UTF-8"?>
+<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
+    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
+<plist version="1.0">
+<dict>
+    <key>Label</key>
+    <string>com.jira-assistant</string>
+    <key>ProgramArguments</key>
+    <array>
+        <string>${binary_path}</string>
+        <string>start</string>
+    </array>
+    <key>RunAtLoad</key>
+    <true/>
+    <key>KeepAlive</key>
+    <dict>
+        <key>Crashed</key>
+        <true/>
+        <key>SuccessfulExit</key>
+        <false/>
+    </dict>
+    <key>ThrottleInterval</key>
+    <integer>30</integer>
+    <key>StandardOutPath</key>
+    <string>${HOME}/Library/Logs/jira-assistant.log</string>
+    <key>StandardErrorPath</key>
+    <string>${HOME}/Library/Logs/jira-assistant.log</string>
+</dict>
+</plist>
+EOF
+  launchctl load "$plist_path"
 }
 
 register_linux_service() {
-  : # implemented in section-05-install-services
+  local binary_path="$1"
+  local unit_dir="$HOME/.config/systemd/user"
+  local unit_path="$unit_dir/jira-assistant.service"
+  mkdir -p "$unit_dir"
+  cat > "$unit_path" <<EOF
+[Unit]
+Description=Jira Assistant Telegram Bot
+After=network.target
+
+[Service]
+Type=simple
+ExecStart=${binary_path} start
+Restart=on-failure
+RestartSec=5
+StartLimitIntervalSec=300
+StartLimitBurst=5
+
+[Install]
+WantedBy=default.target
+EOF
+  systemctl --user daemon-reload
+  systemctl --user enable --now jira-assistant
+  echo ""
+  echo "Optional: to start jira-assistant at boot even when you are not logged in, run:"
+  echo "  loginctl enable-linger ${USER}"
+  echo "Note: this may require sudo on some systems."
 }
 
 start_service() {
-  : # implemented in section-05-install-services
+  if [[ "$OS" == "macos" ]]; then
+    if launchctl list 2>/dev/null | grep -q "com.jira-assistant"; then
+      echo "Service running (launchd)."
+    else
+      echo "Service not detected in launchd — check ~/Library/LaunchAgents/com.jira-assistant.plist"
+    fi
+  else
+    if systemctl --user is-active jira-assistant &>/dev/null; then
+      echo "Service running (systemd)."
+    else
+      echo "Service not detected — check: systemctl --user status jira-assistant"
+    fi
+  fi
 }
 
 do_uninstall() {
@@ -209,9 +282,9 @@ main() {
 
   if [[ "$OS" == "macos" ]]; then
     strip_quarantine "$INSTALL_DIR/jira-assistant"
-    register_macos_service
+    register_macos_service "$INSTALL_DIR/jira-assistant"
   else
-    register_linux_service
+    register_linux_service "$INSTALL_DIR/jira-assistant"
   fi
 
   run_config_if_needed
diff --git a/tests/install.bats b/tests/install.bats
index f102671..2e15098 100644
--- a/tests/install.bats
+++ b/tests/install.bats
@@ -345,3 +345,119 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
     [ "$status" -eq 0 ]
     [ "$output" = "SOURCED_OK" ]
 }
+
+# ─── Section-05 install.sh service registration tests ───────────────────────
+
+@test "register_macos_service: plist created at correct path" {
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    [ -f "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist" ]
+}
+
+@test "register_macos_service: plist contains KeepAlive in dictionary form" {
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    grep -q '<key>KeepAlive</key>' "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist"
+    grep -q '<dict>' "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist"
+}
+
+@test "register_macos_service: plist contains ThrottleInterval = 30" {
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    grep -q '<integer>30</integer>' "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist"
+}
+
+@test "register_macos_service: plist contains RunAtLoad = true" {
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    grep -q '<key>RunAtLoad</key>' "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist"
+    grep -q '<true/>' "$FAKE_HOME/Library/LaunchAgents/com.jira-assistant.plist"
+}
+
+@test "register_macos_service: launchctl unload called before launchctl load" {
+    local call_log="$FAKE_HOME/launchctl_calls"
+    {
+        printf '#!/bin/sh\n'
+        printf 'printf "%%s\\n" "$1" >> "%s"\n' "$call_log"
+        printf 'exit 0\n'
+    } > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    [ "$(sed -n '1p' "$call_log")" = "unload" ]
+    [ "$(sed -n '2p' "$call_log")" = "load" ]
+}
+
+@test "register_macos_service: launchctl load called with plist path" {
+    local call_log="$FAKE_HOME/launchctl_calls"
+    {
+        printf '#!/bin/sh\n'
+        printf 'printf "%%s\\n" "$*" >> "%s"\n' "$call_log"
+        printf 'exit 0\n'
+    } > "$MOCK_BIN/launchctl"
+    chmod +x "$MOCK_BIN/launchctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_macos_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    grep -q "load.*com.jira-assistant.plist" "$call_log"
+}
+
+@test "register_linux_service: service file created at correct path" {
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
+    chmod +x "$MOCK_BIN/systemctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    [ -f "$FAKE_HOME/.config/systemd/user/jira-assistant.service" ]
+}
+
+@test "register_linux_service: unit file contains Restart=on-failure" {
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
+    chmod +x "$MOCK_BIN/systemctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    grep -q 'Restart=on-failure' "$FAKE_HOME/.config/systemd/user/jira-assistant.service"
+}
+
+@test "register_linux_service: unit file contains StartLimitIntervalSec=300" {
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
+    chmod +x "$MOCK_BIN/systemctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    grep -q 'StartLimitIntervalSec=300' "$FAKE_HOME/.config/systemd/user/jira-assistant.service"
+}
+
+@test "register_linux_service: unit file contains StartLimitBurst=5" {
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
+    chmod +x "$MOCK_BIN/systemctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    grep -q 'StartLimitBurst=5' "$FAKE_HOME/.config/systemd/user/jira-assistant.service"
+}
+
+@test "register_linux_service: unit file contains Type=simple" {
+    printf '#!/bin/sh\nexit 0\n' > "$MOCK_BIN/systemctl"
+    chmod +x "$MOCK_BIN/systemctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    grep -q 'Type=simple' "$FAKE_HOME/.config/systemd/user/jira-assistant.service"
+}
+
+@test "register_linux_service: systemctl enable --now called" {
+    local call_log="$FAKE_HOME/systemctl_calls"
+    {
+        printf '#!/bin/sh\n'
+        printf 'printf "%%s\\n" "$*" >> "%s"\n' "$call_log"
+        printf 'exit 0\n'
+    } > "$MOCK_BIN/systemctl"
+    chmod +x "$MOCK_BIN/systemctl"
+    run bash -c "export HOME=\"$FAKE_HOME\"; export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; register_linux_service \"/usr/local/bin/jira-assistant\""
+    [ "$status" -eq 0 ]
+    grep -q -- '--user enable --now' "$call_log"
+}
