# Code Review Interview: section-06-install-uninstall

## False positives in CRITICAL findings
- CRITICAL #1 (stop_existing_service missing disable): reviewer misread spec — spec explicitly says only `stop` in stop_existing_service, `disable` only in do_uninstall
- CRITICAL #2 (ordering): reviewer misread line numbers — detect_platform (279) < stop_existing_service (290), ordering is correct

## Auto-fix applied
- Added advisory message assertions to "exits 0 gracefully" test: `[[ "$output" == *"Config files"* ]]` and `[[ "$output" == *"PATH entries"* ]]`

## Let go (nitpicks)
- `for arg` loop vs `${1:-}`: functionally equivalent, no change needed
- Advisory uses literal `~`: matches spec verbatim
- `/usr/local/bin` test is "rm called" not "file deleted" — acceptable since we cannot write to /usr/local/bin in tests
