# Code Review Interview: section-01-ci-workflow

## Auto-fixes Applied

**Fix 1: sha256sum path prefix**
- Changed `sha256sum artifacts/* > checksums.txt` to `cd artifacts && sha256sum * > ../checksums.txt`
- Reason: original command produces paths like `artifacts/jira-assistant-macos-arm64` inside checksums.txt. Users running `sha256sum -c checksums.txt` against downloaded files in a flat directory would get "No such file or directory". Fix produces bare filenames.
- File: `.github/workflows/release.yml`

**Fix 2: Add merge-multiple assertion to validate script**
- Added `grep -q "merge-multiple: true"` assertion to `tests/validate-workflow.sh`
- Reason: plan requires `merge-multiple: true` for flat artifact download; test script didn't verify it.

## Items Let Go

- **ubuntu-latest for macOS cross-compilation**: Plan explicitly states "All three builds run on ubuntu-latest using Bun's cross-compilation support. No macOS or ARM runners are needed." Intentional.
- **lint.yml no path filters**: Plan says "every push/PR". Intentional.
- **Floating @v2 tag**: Plan explicitly defers SHA pinning for this personal project.
- **Matrix count fragility**: Minor; current implementation is correct.
- **No checkout in release job**: Intentional — no repo files needed.
- **No if guard**: Workflow trigger already enforces it. Redundant.
