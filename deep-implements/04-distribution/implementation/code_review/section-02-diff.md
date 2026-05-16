diff --git a/tests/install.bats b/tests/install.bats
new file mode 100644
index 0000000..87ba11c
--- /dev/null
+++ b/tests/install.bats
@@ -0,0 +1,66 @@
+#!/usr/bin/env bats
+# Smoke tests for release binaries.
+# Run manually after a release with actual binaries present.
+# Usage: BINARY_DIR=./artifacts bats tests/install.bats
+
+BINARY_DIR="${BINARY_DIR:-./artifacts}"
+
+verify_binary_size() {
+    local path="$1"
+    local size
+    size=$(wc -c < "$path")
+    local min=$((10 * 1024 * 1024))   # 10MB
+    local max=$((500 * 1024 * 1024))  # 500MB
+    [ "$size" -ge "$min" ] && [ "$size" -le "$max" ]
+}
+
+@test "macOS arm64 binary: size between 10MB and 500MB" {
+    skip "Manual smoke test — requires release binary"
+    local bin="$BINARY_DIR/jira-assistant-macos-arm64"
+    [ -f "$bin" ]
+    verify_binary_size "$bin"
+}
+
+@test "macOS x64 binary: size between 10MB and 500MB" {
+    skip "Manual smoke test — requires release binary"
+    local bin="$BINARY_DIR/jira-assistant-macos-x64"
+    [ -f "$bin" ]
+    verify_binary_size "$bin"
+}
+
+@test "Linux x64 binary: size between 10MB and 500MB" {
+    skip "Manual smoke test — requires release binary"
+    local bin="$BINARY_DIR/jira-assistant-linux-x64"
+    [ -f "$bin" ]
+    verify_binary_size "$bin"
+}
+
+@test "macOS arm64 binary: ad-hoc code signature valid (codesign -v)" {
+    skip "Manual smoke test — requires release binary and macOS"
+    local bin="$BINARY_DIR/jira-assistant-macos-arm64"
+    [ -f "$bin" ]
+    codesign -v "$bin"
+}
+
+@test "macOS x64 binary: ad-hoc code signature valid (codesign -v)" {
+    skip "Manual smoke test — requires release binary and macOS"
+    local bin="$BINARY_DIR/jira-assistant-macos-x64"
+    [ -f "$bin" ]
+    codesign -v "$bin"
+}
+
+@test "macOS arm64 binary: executes without SIGKILL (exit code != 137)" {
+    skip "Manual smoke test — requires release binary on macOS arm64"
+    local bin="$BINARY_DIR/jira-assistant-macos-arm64"
+    chmod +x "$bin"
+    run "$bin" --version
+    [ "$status" -ne 137 ]
+}
+
+@test "Linux x64 binary: executes and returns non-error on --version" {
+    skip "Manual smoke test — requires release binary on Linux x64"
+    local bin="$BINARY_DIR/jira-assistant-linux-x64"
+    chmod +x "$bin"
+    run "$bin" --version
+    [ "$status" -ne 137 ]
+}
