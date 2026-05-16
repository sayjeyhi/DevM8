diff --git a/.github/workflows/lint.yml b/.github/workflows/lint.yml
new file mode 100644
index 0000000..6933d72
--- /dev/null
+++ b/.github/workflows/lint.yml
@@ -0,0 +1,15 @@
+name: Lint
+
+on:
+  push:
+  pull_request:
+
+jobs:
+  shellcheck:
+    runs-on: ubuntu-latest
+    steps:
+      - uses: actions/checkout@v4
+      - name: shellcheck
+        run: shellcheck install.sh
+      - name: bash syntax check
+        run: bash -n install.sh
diff --git a/.github/workflows/release.yml b/.github/workflows/release.yml
new file mode 100644
index 0000000..9c81994
--- /dev/null
+++ b/.github/workflows/release.yml
@@ -0,0 +1,50 @@
+name: Release
+
+on:
+  push:
+    tags:
+      - 'v*.*.*'
+
+permissions:
+  contents: write
+
+jobs:
+  build:
+    runs-on: ubuntu-latest
+    strategy:
+      matrix:
+        include:
+          - target: bun-darwin-arm64
+            outfile: jira-assistant-macos-arm64
+          - target: bun-darwin-x64
+            outfile: jira-assistant-macos-x64
+          - target: bun-linux-x64
+            outfile: jira-assistant-linux-x64
+    steps:
+      - uses: actions/checkout@v4
+      - uses: oven-sh/setup-bun@v2
+        with:
+          bun-version: "1.3.11"
+      - run: bun install
+      - run: bun build --compile --target=${{ matrix.target }} --outfile=${{ matrix.outfile }} src/index.ts
+      - uses: actions/upload-artifact@v4
+        with:
+          name: ${{ matrix.outfile }}
+          path: ${{ matrix.outfile }}
+
+  release:
+    needs: build
+    runs-on: ubuntu-latest
+    steps:
+      - uses: actions/download-artifact@v4
+        with:
+          merge-multiple: true
+          path: artifacts/
+      - run: sha256sum artifacts/* > checksums.txt
+      - uses: softprops/action-gh-release@v2
+        with:
+          generate_release_notes: true
+          prerelease: ${{ contains(github.ref_name, '-') }}
+          files: |
+            artifacts/*
+            checksums.txt
diff --git a/.yamllint.yml b/.yamllint.yml
new file mode 100644
index 0000000..835dba1
--- /dev/null
+++ b/.yamllint.yml
@@ -0,0 +1,8 @@
+---
+extends: default
+rules:
+  document-start: disable
+  line-length:
+    max: 120
+  truthy:
+    allowed-values: ['true', 'false', 'on', 'off', 'yes', 'no']
diff --git a/tests/validate-workflow.sh b/tests/validate-workflow.sh
new file mode 100644
index 0000000..41989ad
--- /dev/null
+++ b/tests/validate-workflow.sh
@@ -0,0 +1,39 @@
+#!/usr/bin/env bash
+set -euo pipefail
+
+REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
+RELEASE_YML="$REPO_ROOT/.github/workflows/release.yml"
+LINT_YML="$REPO_ROOT/.github/workflows/lint.yml"
+
+fail() { echo "FAIL: $1" >&2; exit 1; }
+pass() { echo "PASS: $1"; }
+
+cd "$REPO_ROOT"
+
+# 1. YAML is syntactically valid
+yamllint "$RELEASE_YML" && pass "release.yml yamllint" || fail "release.yml yamllint"
+yamllint "$LINT_YML"    && pass "lint.yml yamllint"    || fail "lint.yml yamllint"
+
+# 2. lint.yml contains shellcheck step
+grep -q "shellcheck" "$LINT_YML" && pass "lint.yml has shellcheck" || fail "lint.yml missing shellcheck"
+
+# 3. release.yml matrix has exactly 3 entries
+count=$(grep -c "bun-darwin-arm64\|bun-darwin-x64\|bun-linux-x64" "$RELEASE_YML")
+[ "$count" -eq 3 ] && pass "release.yml matrix has 3 entries" || fail "release.yml matrix entry count: got $count, want 3"
+
+# 4. upload-artifact and download-artifact use @v4
+grep -q "upload-artifact@v4"   "$RELEASE_YML" && pass "upload-artifact@v4"   || fail "upload-artifact not @v4"
+grep -q "download-artifact@v4" "$RELEASE_YML" && pass "download-artifact@v4" || fail "download-artifact not @v4"
+! grep -q "upload-artifact@v3\|download-artifact@v3" "$RELEASE_YML" && pass "no @v3 artifacts" || fail "found @v3 artifact action"
+
+# 5. prerelease expression is present
+grep -q "contains(github.ref_name, '-')" "$RELEASE_YML" && pass "prerelease expression" || fail "prerelease expression missing"
+
+# 6. Bun version is pinned to 1.3.11
+grep -q "1.3.11" "$RELEASE_YML" && pass "bun pinned to 1.3.11" || fail "bun not pinned to 1.3.11"
+
+# 7. permissions: contents: write is set
+grep -q "contents: write" "$RELEASE_YML" && pass "contents: write permission" || fail "contents: write permission missing"
+
+echo ""
+echo "All checks passed."
