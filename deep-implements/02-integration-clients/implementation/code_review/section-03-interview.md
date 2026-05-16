# Code Review Interview: section-03-telegram

## User Decisions

**startPolling() Promise handling** → Attach `.catch()` to log startup errors (keep void return type)

## Auto-Fixes Applied

1. Fixed double-logging: extracted `errorType` variable; unknown branch gets `logger.error` then `logger.info`; all other branches get only `logger.info`
2. Added `try/catch` around `err.ctx.reply()` inside error middleware — reply failures are caught and logged
3. `startPolling()` now attaches `.catch((err) => logger.error(...))` to `bot.start()` promise
4. Fixed `ClaudeExitError` test: assertion changed from `toContain('1')` to `toContain('exit 1')`
5. Fixed `mockStart` in tests to return `Promise.resolve(undefined)` (needed for `.catch()` chain)
6. Added TODO comment for Logger stub replacement

## Let Go

- PII: userId in CommandContext is by spec design; handlers are responsible for not logging it
- args trailing-space edge case (spec-verbatim behavior, no test needed)
- splitMessage ignores single `\n` (by design, `\n\n` paragraph split)
- Non-command fallback uses `ctx.reply` directly (36-char message, harmless, spec-verbatim)
- Wide cast: refactored to use `instanceof` narrowing without extra cast fields
- splitMessage paragraph test uses `toContain` (acceptable flexibility)
