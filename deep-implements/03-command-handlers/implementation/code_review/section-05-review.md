# Code Review: Section 05 — Move, Comment, Help Handlers

## HIGH

1. **ctx.replyWithChatAction vs ctx.sendChatAction** — Reviewer flagged replyWithChatAction as non-existent. INCORRECT: grammy's Context has both methods; replyWithChatAction auto-fills chat_id. Existing create.ts uses the same. No fix needed.

2. **JiraAuthError not caught** — move.ts and comment.ts fall through to the generic handler for JiraAuthError, which logs err.message. JiraAuthError message could contain credential hints. Add a distinct catch returning a sanitized reply.

3. **Generic error handler logs raw err.message** — Only reached for errors that are NOT InvalidTransitionError / JiraNotFoundError (they are caught earlier). Still, defensive: add JiraAuthError catch to prevent any auth-related message reaching the log.

## MEDIUM

4. **Path divergence from plan** — Actual path is src/bot/commands/ vs plan's src/commands/. Known and accepted since section-01.

5. **Duplicate test in comment.test.ts** — "valid args" and "preserves internal spacing" tests use identical input. Consolidate.

6. **Missing test: generic error path** — No test for plain Error thrown from transitionIssue/addComment. Should test that "Something went wrong" reply fires.

7. **Missing test: JiraAuthError path** — No test for JiraAuthError on move or comment. (Auto-fix after adding handler.)

## LOW

8. **Missing TODO for Clients interface** — Known temp; add inline comment.

9. **console.log convention** — Matches create.ts exactly. No change needed.

10. **help.ts no try/catch** — Per plan intent. Fine.

## Auto-fixes planned
- Add JiraAuthError handling in move.ts and comment.ts
- Add generic error test and JiraAuthError test for both handlers
- Remove duplicate comment test
- Add TODO comment to Clients interface
