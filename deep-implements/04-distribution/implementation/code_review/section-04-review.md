# Code Review: section-04-install-core

## Critical Issues

### 1. verify_checksum grep not anchored — matches partial filenames
`grep "$name" checksums.txt` matches any line containing the name as substring.
**Fix:** `grep " ${name}$"` — two spaces then exact basename then end-of-line.

### 2. TMP_DIR empty in error message when function called directly in tests
`"...Delete $TMP_DIR and retry."` — TMP_DIR="" when called outside main().
**Fix:** `${TMP_DIR:-<temp download directory>}`.

### 3. print_success() omits service status — spec violation
Spec explicitly lists "Service status" as required output.
**Fix:** Add `echo "  Service: registered"` (stub; section-05 will update).

### 4. Missing Bats test: checksums.txt download failure exits 1
Spec requires this test case explicitly.
**Fix:** Add test stub checking download_with_retry failure propagates.

## Let Go
- Trap timing after flag-parsing loop: safe, temp dir not created on early exit paths
- Mock curl precision in resolve_version test: output check still validates the right thing
- local RELEASE_URL inside main(): valid bash, not a real issue  
- PATH_MODIFIED coupling: architectural; not a current bug
- select_install_dir env-dependent skips: architectural limitation, acceptable for now
- Truncation test name misleading: cosmetic
