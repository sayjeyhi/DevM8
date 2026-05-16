# Code Review: section-05-install-services

## CRITICAL

1. **Binary paths with spaces**: `ExecStart=${binary_path} start` and the plist `<string>` block don't protect against paths with spaces. Low likelihood since install dir is `/usr/local/bin` or `~/.local/bin`, but edge case.

2. **`&>/dev/null` bashism** in `start_service`: inconsistent with `>/dev/null 2>&1` style used elsewhere. Not flagged by shellcheck at error severity since shebang is bash.

## IMPORTANT

3. **KeepAlive/RunAtLoad tests**: grep checks are independent; `<dict>` and `<true/>` match other parts of the plist. Should use adjacency check.

4. **No `daemon-reload` test**: plan acceptance checklist requires `systemctl --user daemon-reload && enable --now` — only `enable --now` is tested.

5. **`start_service` untested**: plan's stubs requirement implies all 3 functions need tests.

6. **`~/Library/Logs/` may not exist**: on fresh macOS, launchd silently drops log output. Should `mkdir -p "$HOME/Library/Logs"` before writing plist.

## MINOR

7. **`echo ""`**: replace with bare `echo` (idiomatic).
8. **`${USER}` portability**: `$(id -un)` is more portable.

## NITPICK

9. Test setup repeated inline 10+ times; shared helper would reduce duplication.
10. No test that `loginctl` is never invoked (only advisory printed).
