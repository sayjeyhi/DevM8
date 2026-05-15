#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_YML="$REPO_ROOT/.github/workflows/release.yml"
LINT_YML="$REPO_ROOT/.github/workflows/lint.yml"

fail() { echo "FAIL: $1" >&2; exit 1; }
pass() { echo "PASS: $1"; }

cd "$REPO_ROOT"

# 1. YAML is syntactically valid
yamllint "$RELEASE_YML" && pass "release.yml yamllint" || fail "release.yml yamllint"
yamllint "$LINT_YML"    && pass "lint.yml yamllint"    || fail "lint.yml yamllint"

# 2. lint.yml contains shellcheck step
grep -q "shellcheck" "$LINT_YML" && pass "lint.yml has shellcheck" || fail "lint.yml missing shellcheck"

# 3. release.yml matrix has exactly 3 entries
count=$(grep -c "bun-darwin-arm64\|bun-darwin-x64\|bun-linux-x64" "$RELEASE_YML")
[ "$count" -eq 3 ] && pass "release.yml matrix has 3 entries" || fail "release.yml matrix entry count: got $count, want 3"

# 4. merge-multiple: true is present in download step
grep -q "merge-multiple: true" "$RELEASE_YML" && pass "merge-multiple: true" || fail "merge-multiple: true missing"

# 5. upload-artifact and download-artifact use @v4
grep -q "upload-artifact@v4"   "$RELEASE_YML" && pass "upload-artifact@v4"   || fail "upload-artifact not @v4"
grep -q "download-artifact@v4" "$RELEASE_YML" && pass "download-artifact@v4" || fail "download-artifact not @v4"
! grep -q "upload-artifact@v3\|download-artifact@v3" "$RELEASE_YML" && pass "no @v3 artifacts" || fail "found @v3 artifact action"

# 5. prerelease expression is present
grep -q "contains(github.ref_name, '-')" "$RELEASE_YML" && pass "prerelease expression" || fail "prerelease expression missing"

# 6. Bun version is pinned to 1.3.11
grep -q "1.3.11" "$RELEASE_YML" && pass "bun pinned to 1.3.11" || fail "bun not pinned to 1.3.11"

# 7. permissions: contents: write is set
grep -q "contents: write" "$RELEASE_YML" && pass "contents: write permission" || fail "contents: write permission missing"

echo ""
echo "All checks passed."
