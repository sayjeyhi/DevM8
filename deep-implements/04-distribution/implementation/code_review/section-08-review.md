# Code Review: section-08-tests

## CRITICAL #1 — `|| true` placement (FALSE POSITIVE)
Reviewer claimed `|| true` after `awk` doesn't suppress pipefail. **Incorrect.** `||` has lower precedence than `|`, so `grep ... | awk ... || true` = `(grep ... | awk ...) || true`. The `|| true` converts the failed pipe exit code to 0 inside the command substitution. Test 29 passes, confirming correctness.

## HIGH #3 — `bats` vs `bats-core` package name (NON-ISSUE on ubuntu-latest)
Ubuntu 22.04+ (`ubuntu-latest`) ships bats 1.2.1 via `sudo apt-get install -y bats`. The 0.4.0 concern applies to Ubuntu 18.04 only. Current workflow is correct.

## HIGH #4 — No macOS CI lane (OUT OF SCOPE)
Section plan explicitly says "Add Bats install + run step to `lint.yml` if the CI runner supports it" and uses `ubuntu-latest`. A macOS lane would be an enhancement beyond the section scope.

## MEDIUM #5 — INSTALL_SH_SOURCED guard (FALSE POSITIVE)
The `if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then main "$@"; fi` guard already prevents `main` from running when sourced. Test 38 ("main() wrap safety") confirms this.

## LOW #9 — Service registration tests (FALSE POSITIVE)
`register_macos_service` and `register_linux_service` tests (tests 39-55) were implemented in sections 05-06. They exist in `tests/install.bats` and pass. The diff only shows section-08 changes.

## Verdict
No fixes required. All reviewer CRITICAL/HIGH/MEDIUM findings are false positives or out of scope. Tests 1-64 pass (8 skipped as manual-only). `shellcheck -S error` passes.
