diff --git a/tests/install.bats b/tests/install.bats
index 0c03f8f..dbadf10 100644
--- a/tests/install.bats
+++ b/tests/install.bats
@@ -5,6 +5,11 @@
 
 setup() {
     BINARY_DIR="${BINARY_DIR:-./artifacts}"
+    CHECKSUMS_TMPDIR="$(mktemp -d)"
+}
+
+teardown() {
+    rm -rf "$CHECKSUMS_TMPDIR"
 }
 
 verify_binary_size() {
@@ -79,3 +84,40 @@ verify_binary_size() {
     run "$bin" --version
     [ "$status" -eq 0 ]
 }
+
+_make_fixture_checksums() {
+    local dir="$1"
+    printf 'data1' > "$dir/jira-assistant-macos-arm64"
+    printf 'data2' > "$dir/jira-assistant-macos-x64"
+    printf 'data3' > "$dir/jira-assistant-linux-x64"
+    (cd "$dir" && sha256sum * > checksums.txt)
+}
+
+@test "checksums.txt: each line matches <64 hex chars>  <filename> format" {
+    _make_fixture_checksums "$CHECKSUMS_TMPDIR"
+    while IFS= read -r line; do
+        [[ "$line" =~ ^[0-9a-f]{64}\ \ [^[:space:]]+$ ]]
+    done < "$CHECKSUMS_TMPDIR/checksums.txt"
+}
+
+@test "checksums.txt: all three binary names appear" {
+    _make_fixture_checksums "$CHECKSUMS_TMPDIR"
+    grep -q "jira-assistant-macos-arm64" "$CHECKSUMS_TMPDIR/checksums.txt"
+    grep -q "jira-assistant-macos-x64"  "$CHECKSUMS_TMPDIR/checksums.txt"
+    grep -q "jira-assistant-linux-x64"  "$CHECKSUMS_TMPDIR/checksums.txt"
+}
+
+@test "sha256sum --check exits 0 when binaries are intact" {
+    _make_fixture_checksums "$CHECKSUMS_TMPDIR"
+    cd "$CHECKSUMS_TMPDIR"
+    run sha256sum --check checksums.txt
+    [ "$status" -eq 0 ]
+}
+
+@test "sha256sum --check exits non-zero after binary corruption" {
+    _make_fixture_checksums "$CHECKSUMS_TMPDIR"
+    cd "$CHECKSUMS_TMPDIR"
+    printf 'corrupted' > jira-assistant-macos-arm64
+    run sha256sum --check checksums.txt
+    [ "$status" -ne 0 ]
+}
