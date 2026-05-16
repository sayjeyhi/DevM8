## section-04-launchd Code Review

### CRITICAL
- `launchctlHint` generic fallback message doesn't match spec (errors.ts:30 returns wrong hint string)

### HIGH (correctness)
- `writePlist` not atomic — should use temp+rename like pid.ts/restart-tracker.ts
- `agentStatus` primary path doesn't parse `state` field or populate `exitCode`
- `restart-tracker.ts`: `> maxRestarts` should be `>= maxRestarts` (off-by-one, test also wrong)

### MEDIUM
- `agentStatus` uid fallback to 0 is dangerous — should throw FriendlyError
- `binaryPath` not XML-escaped before plist interpolation
- `launchctl list` substring match fragile (could match `net.jira-assistant-updater`)

### DESIGN
- `writePlist`, `writePid`, `readPid`, `removePid` expose optional `filePath` not in plan spec
- `pid.test.ts` afterEach cleanup is non-functional noise

### LOW
- No assertion on `exitCode` in agentStatus list-fallback test
- Temp file names use predictable suffix (low risk)
