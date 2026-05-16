# Code Review Interview: section-04-install-core

## All fixes auto-applied (no user input needed)

### Fix 1: verify_checksum grep anchored to exact filename
Changed: `grep "$name"` → `grep "  ${name}$"`
Two spaces + end-of-line anchor prevents partial matches (e.g., `mybin-x64` matching `mybin`).

### Fix 2: TMP_DIR fallback in error message
Changed: `Delete $TMP_DIR and retry.` → `Delete ${TMP_DIR:-<temp download directory>} and retry.`
When verify_checksum is called directly in tests (outside main()), TMP_DIR is empty.

### Fix 3: Service status in print_success()
Added `echo "  Service: ${SERVICE_STATUS:-registered}"` to satisfy spec requirement.
Section-05 will set SERVICE_STATUS when implementing real service registration.

### Fix 4: Added missing checksums.txt download failure test
New test: "download_with_retry for checksums.txt: propagates failure, exits non-zero"
Satisfies spec-mandated test case: "checksums.txt download failure → exits 1 with error".

## Let Go
- Trap timing before/after flag parsing: safe, TMP_DIR not created on early exits
- Mock curl precision: test outcome validates the right thing
- local RELEASE_URL inside main(): valid bash
- PATH_MODIFIED coupling with ensure_path: architectural, section-04 scope only
- select_install_dir env-dependent skips: architectural, skip guards are appropriate
- Truncation test name: cosmetic, BASH_SOURCE guard is the correct safety mechanism
