# Code Review Interview: section-03-checksums-generation

## Findings Triaged

### Auto-fixes applied (no user input needed)

1. **teardown rm -rf guard** — Added `[ -n "$CHECKSUMS_TMPDIR" ] &&` before `rm -rf` to prevent deleting cwd if mktemp fails.

2. **sha256sum availability guard** — Added `command -v sha256sum > /dev/null || skip "sha256sum not available"` at top of `_make_fixture_checksums`. On macOS (no sha256sum), all 4 tests skip gracefully instead of erroring.

3. **Empty file guard in format test** — Added `[ "$(wc -l < ...)" -ge 3 ]` assertion before the loop. Prevents false-green if checksums.txt is empty.

4. **Bare cd removed** — Replaced `cd "$CHECKSUMS_TMPDIR"` + `run sha256sum` with `run bash -c "cd ... && sha256sum --check checksums.txt"` to avoid polluting cwd for subsequent tests.

### False positive dismissed

5. **release.yml missing checksum step** — Reviewer claimed the step was absent; it was already correctly committed from section-01/02: `cd artifacts && sha256sum * > ../checksums.txt`.

### Let go

- Regex portability: ubuntu-latest bash 5.x handles ERE `{64}` fine
- Corruption test overwrites vs single-byte flip: both cause hash mismatch; outcome identical
- grep word boundary: no realistic false-match in this fixture
- No exact line count: minor gap, acceptable
