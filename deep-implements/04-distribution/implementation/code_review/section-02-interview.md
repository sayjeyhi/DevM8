# Code Review Interview: section-02-binary-targets

## Auto-fixes Applied

**Fix 1: BINARY_DIR moved to setup()**
Moved top-level `BINARY_DIR` assignment into `setup()` function for reliable BATS scoping.

**Fix 2: codesign uses `run` pattern**
Changed bare `codesign -v "$bin"` to `run codesign -v "$bin"` + `[ "$status" -eq 0 ]` for clearer failure messages.

**Fix 3: Added missing macOS x64 execution test**
Added `macOS x64 binary: executes without SIGKILL` test — plan requires all three binaries verified.

**Fix 4: Fixed Linux test assertion and name**
Changed to `[ "$status" -eq 0 ]` and renamed to "executes successfully on --version" to match actual intent.

**Fix 5: Added file-existence guards**
Added `[ -f "$bin" ] || skip "Binary not found: $bin"` to all tests for helpful skip messages when binary absent.

## Items Let Go

- **BATS helper libraries**: Over-engineering for these simple smoke tests.
- **No linux-arm64**: Intentional per plan scope.
- **chmod inconsistency**: Only execution tests need it; cosmetic issue doesn't affect correctness.
