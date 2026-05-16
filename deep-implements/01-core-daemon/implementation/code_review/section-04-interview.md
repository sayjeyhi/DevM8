## section-04-launchd Code Review Interview Transcript

### Auto-fixed (no user input needed)

1. **`writePlist` not atomic** → Added temp+rename pattern (same as pid.ts / restart-tracker.ts)
2. **`launchctlHint` generic fallback** → Changed to `"launchctl exited with a non-zero status"` per spec; updated existing errors.test.ts
3. **`agentStatus` primary path** → Now parses `state` and `last exit code` fields from `launchctl print`; returns `exitCode` on stopped services
4. **`restart-tracker.ts` off-by-one** → Changed `> maxRestarts` to `>= maxRestarts`; updated test (3rd call triggers true, not 4th)
5. **`agentStatus` uid=0 fallback** → Now throws `FriendlyError` if `process.getuid` is unavailable
6. **`binaryPath` XML injection** → Added `xmlEscape()` before interpolating into plist string
7. **`launchctl list` fragile match** → Changed to exact `parts[2]?.trim() === "net.jira-assistant"` column match
8. **`pid.test.ts` afterEach noise** → Removed non-functional `Bun.file().exists()` line
9. **Added test** for `exitCode` from list fallback (stopped process case)

### User decision

**Optional `filePath` on public functions**: User chose to keep — pragmatic testability, section-05 callers unaffected.

### Let go

- `launchctl load/unload` deprecation: spec-compliant, acceptable until macOS forces removal
- Temp file name predictability: low risk (user-owned directory)
