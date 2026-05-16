# Code Review: section-03-telegram

## CRITICAL

1. **PII leak (CommandContext.userId)** — reviewer noted but confirmed by-design (spec mandates userId in context). Not a code defect.

2. **Double-logging on unknown errors** — `logger.error` fires for unknown, then `logger.info` fires unconditionally for ALL errors. Every unknown error gets two log entries.

3. **startPolling() drops bot.start() Promise** — grammY `bot.start()` returns a Promise; dropping it silently swallows initial auth/network errors as unhandled rejection.

## IMPORTANT

4. **args trailing-space edge case** — `'/cmd '.split(' ').slice(1)` = `['']`. Not tested, spec-verbatim, low priority.

5. **Error middleware reply can itself throw** — if `err.ctx.reply()` fails (rate limit, deleted chat), exception propagates out of `bot.catch` with undefined behavior.

6. **splitMessage ignores single `\n`** — by design (split on `\n\n` only); single-newline lines in paragraphs become part of the same word block.

7. **No test for double-logging** — test asserts reply called but not logger call count.

8. **ClaudeExitError test fragile** — asserts `msg.toContain('1')` instead of `'exit 1'`.

## MINOR

9. **splitMessage paragraph test** — uses `toContain` instead of exact chunk count.
10. **Non-command fallback bypasses sendMessage** — uses `ctx.reply` directly. 36-char message, harmless in practice.
11. **Wide cast on line 50** — over-specified, only `.type` field actually needed.

## NITPICK

12. **No TODO comment for Logger stub** — should mark for replacement when 01-core-daemon is available.
