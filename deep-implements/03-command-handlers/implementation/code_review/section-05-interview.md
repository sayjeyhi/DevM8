# Interview Transcript: Section 05

## Items Triaged

### Let go (non-issues)
- `ctx.replyWithChatAction` — reviewer incorrectly claimed it doesn't exist in grammy. It does. Matches existing create.ts convention. No change.
- Path divergence (src/bot/commands/ vs plan's src/commands/) — accepted since section-01.
- `console.log` instead of structured logger — matches create.ts exactly.
- `help.ts` no try/catch — per plan intent, intentional.
- Arrow → in success reply — Unicode fine in plain text Telegram messages.

### Auto-fixed (no user interview needed)
1. **JiraAuthError handling** — Added explicit catch in move.ts and comment.ts to return sanitized reply. Prevents auth error messages reaching the generic logger.
2. **Missing generic error test** — Added test for plain Error → "something went wrong" reply in both move and comment test files.
3. **Missing JiraAuthError test** — Added test for JiraAuthError path in both move and comment test files.
4. **Duplicate test in comment.test.ts** — Merged "valid args" and "preserves internal spacing" into single consolidated test.
5. **TODO comment** — Added to Clients interface in move.ts and comment.ts.

## Result
- 23 tests pass across move, comment, help test files.
- 256 tests pass full suite. 0 fail.
