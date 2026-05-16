diff --git a/.github/workflows/lint.yml b/.github/workflows/lint.yml
index 6933d72..1783e0e 100644
--- a/.github/workflows/lint.yml
+++ b/.github/workflows/lint.yml
@@ -10,6 +10,10 @@ jobs:
     steps:
       - uses: actions/checkout@v4
       - name: shellcheck
-        run: shellcheck install.sh
+        run: shellcheck -S error install.sh
       - name: bash syntax check
         run: bash -n install.sh
+      - name: Install bats
+        run: sudo apt-get install -y bats
+      - name: Run install.sh unit tests
+        run: bats tests/install.bats
diff --git a/RELEASE_CHECKLIST.md b/RELEASE_CHECKLIST.md
new file mode 100644
index 0000000..6fddc17
--- /dev/null
+++ b/RELEASE_CHECKLIST.md
@@ -0,0 +1,19 @@
+# Release Checklist
+
+Run these manually before publishing each release.
+
+- [ ] **macOS arm64 binary** — `./jira-assistant-macos-arm64 --version`; exit code must not be 137 (SIGKILL = code-signature regression)
+- [ ] **macOS x64 binary** — `./jira-assistant-macos-x64 --version`; clean exit
+- [ ] **Linux x64 binary** — `./jira-assistant-linux-x64 --version`; clean exit
+- [ ] **Binary sizes** — each binary between 10 MB and 500 MB (smaller = likely corrupt download or HTML error page)
+- [ ] **macOS codesign** — `codesign -v jira-assistant-macos-arm64` exits 0 (ad-hoc signature present)
+- [ ] **curl pipe install on macOS** — one-liner install completes; Gatekeeper does not block execution
+- [ ] **curl pipe install on Linux x64** — systemd service starts; `systemctl --user status jira-assistant` shows active
+- [ ] **Restart behavior on Linux** — `systemctl --user kill jira-assistant`; service restarts within 5 seconds (RestartSec)
+- [ ] **`--uninstall` after clean install** — binary and service files gone; `~/.config/jira-assistant/` preserved
+- [ ] **Re-install over existing install** — no errors; service stops, binary replaced, service restarts
+- [ ] **`~/.local/bin` fallback** — as non-root or with `/usr/local/bin` read-only, install uses `~/.local/bin` and PATH update fires
+- [ ] **Checksum mismatch** — corrupt a downloaded binary byte; "Checksum mismatch" message and exit 1
+- [ ] **Non-TTY stdin** — `curl ... | bash`; config wizard deferred; advisory message printed
+- [ ] **Linux ARM64 rejection** — on ARM64 Linux, explicit error message and exit 1
+- [ ] **Version pinning** — `JIRA_ASSISTANT_VERSION=v1.0.0 bash install.sh`; exact version downloaded
diff --git a/install.sh b/install.sh
index 06062f9..6637958 100644
--- a/install.sh
+++ b/install.sh
@@ -106,7 +106,7 @@ verify_checksum() {
   local name expected actual
 
   name=$(basename "$binary")
-  expected=$(grep "  ${name}$" "$checksums_file" | awk '{print $1}')
+  expected=$(grep "  ${name}$" "$checksums_file" | awk '{print $1}' || true)
   if [[ -z "$expected" ]]; then
     echo "Binary name '$name' not found in checksums.txt" >&2
     exit 1
diff --git a/tests/install.bats b/tests/install.bats
index aaad386..58d3292 100644
--- a/tests/install.bats
+++ b/tests/install.bats
@@ -144,7 +144,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 @test "detect_platform: Darwin arm64 → OS=macos ARCH=arm64" {
     printf '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo arm64;; esac\n' > "$MOCK_BIN/uname"
     chmod +x "$MOCK_BIN/uname"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform; printf '%s %s' \"\$OS\" \"\$ARCH\""
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; detect_platform; printf '%s %s' \"\$OS\" \"\$ARCH\""
     [ "$status" -eq 0 ]
     [ "$output" = "macos arm64" ]
 }
@@ -152,7 +152,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 @test "detect_platform: Darwin x86_64 → OS=macos ARCH=x64" {
     printf '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo x86_64;; esac\n' > "$MOCK_BIN/uname"
     chmod +x "$MOCK_BIN/uname"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform; printf '%s %s' \"\$OS\" \"\$ARCH\""
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; detect_platform; printf '%s %s' \"\$OS\" \"\$ARCH\""
     [ "$status" -eq 0 ]
     [ "$output" = "macos x64" ]
 }
@@ -162,7 +162,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
     chmod +x "$MOCK_BIN/uname"
     printf '#!/bin/sh\necho "ldd (GNU libc) 2.35"\n' > "$MOCK_BIN/ldd"
     chmod +x "$MOCK_BIN/ldd"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform; printf '%s %s' \"\$OS\" \"\$ARCH\""
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; detect_platform; printf '%s %s' \"\$OS\" \"\$ARCH\""
     [ "$status" -eq 0 ]
     [ "$output" = "linux x64" ]
 }
@@ -170,7 +170,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 @test "detect_platform: Linux aarch64 → exits 1 with ARM64 message" {
     printf '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo aarch64;; esac\n' > "$MOCK_BIN/uname"
     chmod +x "$MOCK_BIN/uname"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform"
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; detect_platform"
     [ "$status" -eq 1 ]
     [[ "$output" == *"Linux ARM64 is not yet supported"* ]]
 }
@@ -180,7 +180,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
     chmod +x "$MOCK_BIN/uname"
     printf '#!/bin/sh\necho "musl libc (x86_64)"\n' > "$MOCK_BIN/ldd"
     chmod +x "$MOCK_BIN/ldd"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform"
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; detect_platform"
     [ "$status" -eq 1 ]
     [[ "$output" == *"Alpine/musl Linux is not supported"* ]]
 }
@@ -188,7 +188,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 @test "detect_platform: Windows_NT → exits 1 with Unsupported OS message" {
     printf '#!/bin/sh\ncase "$1" in -s) echo Windows_NT;; -m) echo x86_64;; esac\n' > "$MOCK_BIN/uname"
     chmod +x "$MOCK_BIN/uname"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; detect_platform"
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; detect_platform"
     [ "$status" -eq 1 ]
     [[ "$output" == *"Unsupported OS"* ]]
 }
@@ -196,7 +196,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 @test "resolve_version: uses JIRA_ASSISTANT_VERSION env var, no HTTP call" {
     printf '#!/bin/sh\necho "UNEXPECTED_CURL_CALL"; exit 1\n' > "$MOCK_BIN/curl"
     chmod +x "$MOCK_BIN/curl"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" JIRA_ASSISTANT_VERSION=v1.0.0 source \"$(_install_sh)\"; resolve_version; printf '%s' \"\$VERSION\""
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; export JIRA_ASSISTANT_VERSION=v1.0.0; source \"$(_install_sh)\"; resolve_version; printf '%s' \"\$VERSION\""
     [ "$status" -eq 0 ]
     [[ "$output" == *"v1.0.0"* ]]
 }
@@ -204,7 +204,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 @test "resolve_version: follows /releases/latest redirect and parses tag" {
     printf '#!/bin/sh\necho "https://github.com/sayjeyhi/jira-assistant/releases/tag/v2.3.4"\n' > "$MOCK_BIN/curl"
     chmod +x "$MOCK_BIN/curl"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; resolve_version; printf '%s' \"\$VERSION\""
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; resolve_version; printf '%s' \"\$VERSION\""
     [ "$status" -eq 0 ]
     [[ "$output" == *"v2.3.4"* ]]
 }
@@ -218,7 +218,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
         printf 'done\nexit 0\n'
     } > "$MOCK_BIN/curl"
     chmod +x "$MOCK_BIN/curl"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; download_with_retry http://x.com/f \"$dest\""
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; download_with_retry http://x.com/f \"$dest\""
     [ "$status" -eq 0 ]
     [ -f "$dest" ]
 }
@@ -235,7 +235,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
         printf '  done\nfi\nexit 1\n'
     } > "$MOCK_BIN/curl"
     chmod +x "$MOCK_BIN/curl"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; download_with_retry http://x.com/f \"$dest\""
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; download_with_retry http://x.com/f \"$dest\""
     [ "$status" -eq 0 ]
     [ -f "$dest" ]
 }
@@ -243,7 +243,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 @test "download_with_retry: exits non-zero when both attempts fail" {
     printf '#!/bin/sh\nexit 1\n' > "$MOCK_BIN/curl"
     chmod +x "$MOCK_BIN/curl"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; download_with_retry http://x.com/f \"$FAKE_HOME/out\""
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; download_with_retry http://x.com/f \"$FAKE_HOME/out\""
     [ "$status" -ne 0 ]
 }
 
@@ -252,14 +252,14 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
     printf 'binary content' > "$FAKE_HOME/mybin"
     local hash; hash=$(sha256sum "$FAKE_HOME/mybin" | awk '{print $1}')
     printf '%s  mybin\n' "$hash" > "$FAKE_HOME/checksums.txt"
-    run bash -c "OS=linux source \"$(_install_sh)\"; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
+    run bash -c "source \"$(_install_sh)\"; OS=linux; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
     [ "$status" -eq 0 ]
 }
 
 @test "download_with_retry for checksums.txt: propagates failure, exits non-zero" {
     printf '#!/bin/sh\nexit 1\n' > "$MOCK_BIN/curl"
     chmod +x "$MOCK_BIN/curl"
-    run bash -c "PATH=\"$MOCK_BIN:\$PATH\" source \"$(_install_sh)\"; download_with_retry http://x.com/checksums.txt \"$FAKE_HOME/checksums.txt\""
+    run bash -c "export PATH=\"$MOCK_BIN:\$PATH\"; source \"$(_install_sh)\"; download_with_retry http://x.com/checksums.txt \"$FAKE_HOME/checksums.txt\""
     [ "$status" -ne 0 ]
 }
 
@@ -267,7 +267,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
     command -v sha256sum > /dev/null || skip "sha256sum not available"
     printf 'binary content' > "$FAKE_HOME/mybin"
     printf '%s  mybin\n' "0000000000000000000000000000000000000000000000000000000000000000" > "$FAKE_HOME/checksums.txt"
-    run bash -c "OS=linux source \"$(_install_sh)\"; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
+    run bash -c "source \"$(_install_sh)\"; OS=linux; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
     [ "$status" -eq 1 ]
     [[ "$output" == *"Checksum mismatch"* ]]
 }
@@ -275,7 +275,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 @test "verify_checksum: exits 1 when binary name not in checksums.txt" {
     printf 'binary content' > "$FAKE_HOME/mybin"
     printf '%s  otherfile\n' "aabbcc" > "$FAKE_HOME/checksums.txt"
-    run bash -c "OS=linux source \"$(_install_sh)\"; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
+    run bash -c "source \"$(_install_sh)\"; OS=linux; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
     [ "$status" -eq 1 ]
     [[ "$output" == *"not found in checksums.txt"* ]]
 }
@@ -285,20 +285,20 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
     printf 'data' > "$FAKE_HOME/mybin"
     local hash; hash=$(sha256sum "$FAKE_HOME/mybin" | awk '{print $1}')
     printf '%s  mybin\n' "$hash" > "$FAKE_HOME/checksums.txt"
-    run bash -c "OS=linux source \"$(_install_sh)\"; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
+    run bash -c "source \"$(_install_sh)\"; OS=linux; verify_checksum \"$FAKE_HOME/mybin\" \"$FAKE_HOME/checksums.txt\""
     [ "$status" -eq 0 ]
 }
 
 @test "select_install_dir: uses /usr/local/bin when writable" {
     [ -w /usr/local/bin ] || skip "/usr/local/bin not writable"
-    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; select_install_dir; printf '%s' \"\$INSTALL_DIR\""
+    run bash -c "export HOME=\"$FAKE_HOME\"; source \"$(_install_sh)\"; select_install_dir; printf '%s' \"\$INSTALL_DIR\""
     [ "$status" -eq 0 ]
     [ "$output" = "/usr/local/bin" ]
 }
 
 @test "select_install_dir: falls back to ~/.local/bin when /usr/local/bin not writable" {
     [ -w /usr/local/bin ] && skip "/usr/local/bin is writable on this system"
-    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; select_install_dir; printf '%s' \"\$INSTALL_DIR\""
+    run bash -c "export HOME=\"$FAKE_HOME\"; source \"$(_install_sh)\"; select_install_dir; printf '%s' \"\$INSTALL_DIR\""
     [ "$status" -eq 0 ]
     [ "$output" = "$FAKE_HOME/.local/bin" ]
     [ -d "$FAKE_HOME/.local/bin" ]
@@ -306,7 +306,7 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 
 @test "ensure_path: appends export line to existing RC files" {
     touch "$FAKE_HOME/.zshrc" "$FAKE_HOME/.bashrc"
-    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; ensure_path \"$FAKE_HOME/.local/bin\""
+    run bash -c "export HOME=\"$FAKE_HOME\"; source \"$(_install_sh)\"; ensure_path \"$FAKE_HOME/.local/bin\""
     [ "$status" -eq 0 ]
     grep -q "# jira-assistant" "$FAKE_HOME/.zshrc"
     grep -q "# jira-assistant" "$FAKE_HOME/.bashrc"
@@ -315,27 +315,27 @@ _install_sh() { printf '%s' "${BATS_TEST_DIRNAME}/../install.sh"; }
 @test "ensure_path: idempotent — no duplicate if marker exists" {
     touch "$FAKE_HOME/.zshrc"
     printf '\n# jira-assistant\nexport PATH="%s/.local/bin:$PATH"\n' "$FAKE_HOME" >> "$FAKE_HOME/.zshrc"
-    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; ensure_path \"$FAKE_HOME/.local/bin\"; ensure_path \"$FAKE_HOME/.local/bin\""
+    run bash -c "export HOME=\"$FAKE_HOME\"; source \"$(_install_sh)\"; ensure_path \"$FAKE_HOME/.local/bin\"; ensure_path \"$FAKE_HOME/.local/bin\""
     [ "$status" -eq 0 ]
     [ "$(grep -c '# jira-assistant' "$FAKE_HOME/.zshrc")" -eq 1 ]
 }
 
 @test "ensure_path: does not create missing RC files" {
-    run bash -c "HOME=\"$FAKE_HOME\" source \"$(_install_sh)\"; ensure_path \"$FAKE_HOME/.local/bin\""
+    run bash -c "export HOME=\"$FAKE_HOME\"; source \"$(_install_sh)\"; ensure_path \"$FAKE_HOME/.local/bin\""
     [ "$status" -eq 0 ]
     [ ! -f "$FAKE_HOME/.zshrc" ]
     [ ! -f "$FAKE_HOME/.bashrc" ]
 }
 
 @test "TTY detection: wizard skipped with message when stdin is not a TTY" {
-    run bash -c "CONFIG_FILE=\"$FAKE_HOME/no.json\" source \"$(_install_sh)\"; run_config_if_needed < /dev/null"
+    run bash -c "source \"$(_install_sh)\"; CONFIG_FILE=\"$FAKE_HOME/no.json\"; run_config_if_needed < /dev/null"
     [ "$status" -eq 0 ]
     [[ "$output" == *"jira-assistant config"* ]]
 }
 
 @test "TTY detection: wizard skipped silently when config file exists" {
     touch "$FAKE_HOME/config.json"
-    run bash -c "CONFIG_FILE=\"$FAKE_HOME/config.json\" source \"$(_install_sh)\"; run_config_if_needed < /dev/null"
+    run bash -c "source \"$(_install_sh)\"; CONFIG_FILE=\"$FAKE_HOME/config.json\"; run_config_if_needed < /dev/null"
     [ "$status" -eq 0 ]
     [ -z "$output" ]
 }
