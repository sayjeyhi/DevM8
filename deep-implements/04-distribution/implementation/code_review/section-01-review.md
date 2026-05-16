# Code Review: section-01-ci-workflow

## Issues Found

**1. [HIGH] ubuntu-latest for macOS cross-compilation**
Plan explicitly states: "All three builds run on ubuntu-latest using Bun's cross-compilation support. No macOS or ARM runners are needed." — NOT a real issue per plan intent.

**2. [LOW-MEDIUM] lint.yml no branch/path filters**
Plan says lint runs "on every push/PR" — intentional per plan. NOT actionable.

**3. [MEDIUM] softprops/action-gh-release floating @v2 tag**
Plan explicitly: "SHA pinning is intentionally deferred for this personal project." — NOT actionable.

**4. [MEDIUM] sha256sum path prefix breaks user verification**
`sha256sum artifacts/* > checksums.txt` produces paths like `artifacts/jira-assistant-macos-arm64`. Users downloading files to a flat dir and running `sha256sum -c checksums.txt` get "No such file or directory". Fix: `cd artifacts && sha256sum * > ../checksums.txt`.

**5. [LOW] validate-workflow.sh matrix count fragile**
Grep could count 3 duplicates of one target. Could add per-target assertions. Minor.

**6. [LOW] validate-workflow.sh missing merge-multiple: true check**
Plan requires `merge-multiple: true`. Test script doesn't assert it.

**7. [LOW] No checkout in release job**
Intentional — no repo files needed. NOT actionable.

**8. [LOW] No if guard on release job**
Intentional — workflow trigger already enforces it. NOT actionable.

## Triage
- Auto-fix: #4 (sha256sum path prefix), #6 (add merge-multiple assertion)
- Let go: #1, #2, #3, #5, #7, #8 (intentional per plan or minor)
