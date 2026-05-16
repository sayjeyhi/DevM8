# Code Review: Section 07 — Registration and Bot

## HIGH

1. **process.env mutation** — Sets ANTHROPIC_API_KEY at startup. User chose to keep as-is for personal bot simplicity.

2. **SIGTERM/SIGINT missing process.exit()** — bot.stop() resolves polling but process may hang on other open handles (JiraClient HTTP). Auto-fix: add process.exit(0).

3. **bot.catch uses console.log not console.error** — errors should go to stderr. Auto-fix.

4. **Missing bot.ts test file** — bot.ts wiring code (startBot) requires grammY long-polling which can't be tested in bun:test without a running server. Test coverage provided through individual component tests. Documented.

## MEDIUM

5. **Import paths** — `../jira/JiraClient` is correct for actual file layout (not `../../02-integration-clients/`). No change.

6. **bot.on("message") fires on all messages** — should filter to only command-like text. Auto-fix: change to only reply on message:text.

7. **Trivial Clients interface tests** — test the mock, not the type. Remove.

8. **setCommands failure is fatal** — wrap in try/catch for startup resilience. Auto-fix.

## LOW

9. **allowedIds** — config.allowedUserIds IS already Set<number> from loadConfig. No issue.

10. **Cross-cutting error tests** — individual handler error tests already cover JiraAuthError etc. Note accepted.

## Auto-fixes planned
- Add process.exit(0) to SIGTERM/SIGINT handlers
- Use console.error for bot.catch
- Filter bot.on to text commands only
- Wrap setCommands in try/catch (non-fatal)
- Remove trivial Clients interface tests
