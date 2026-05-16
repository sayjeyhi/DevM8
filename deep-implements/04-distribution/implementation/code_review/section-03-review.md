# Code Review: section-03-checksums-generation

## Critical Issues

### 1. Format-check loop passes silently on empty file
The `while IFS= read -r line` loop never executes if checksums.txt is empty — test trivially passes.
**Fix:** Add `[ $(wc -l < "$CHECKSUMS_TMPDIR/checksums.txt") -ge 3 ]` before the loop.

### 2. Bare `cd` in test body contaminates subsequent tests' working directory
`cd "$CHECKSUMS_TMPDIR"` in the `sha256sum --check` tests changes cwd for all later tests. Pre-existing tests use relative `./artifacts` path which would break.
**Fix:** Use `run bash -c "cd \"$CHECKSUMS_TMPDIR\" && sha256sum --check checksums.txt"`.

### 3. No sha256sum availability guard for macOS
`sha256sum` absent on macOS — local dev gets 4 failures with `command not found` instead of informative skips.
**Fix:** Add `command -v sha256sum || skip "sha256sum not available"` in `_make_fixture_checksums`.

### 4. teardown rm -rf unsafe if CHECKSUMS_TMPDIR is empty
If `mktemp -d` fails, `rm -rf ""` could delete cwd.
**Fix:** Guard: `[ -n "$CHECKSUMS_TMPDIR" ] && rm -rf "$CHECKSUMS_TMPDIR"`.

## False Positive

### 5. Reviewer claimed release.yml missing checksum step
Reviewer missed that `.github/workflows/release.yml` was already committed with the correct `cd artifacts && sha256sum * > ../checksums.txt` step from section-01/02. No change needed.

## Let Go
- Regex portability on older bash: ubuntu-latest runs bash 5.x, ERE `{64}` is fine
- `printf 'corrupted'` vs single-byte flip: both cause hash mismatch; outcome identical
- grep not anchored to word boundary: no realistic false-match in this fixture
- No exact line count assertion: minor completeness gap, acceptable
