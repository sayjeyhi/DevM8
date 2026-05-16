# Code Review: Section 06 — /solve Handler

## HIGH

1. **replyWithChatAction vs sendChatAction** — Plan text says sendChatAction; reviewer acknowledges implementation is CORRECT per grammY convention. No fix.

2. **XML injection in description** — description substituted raw into prompt. If description contains `</description>`, XML framing breaks. Accepted per plan: "XML tags signal data not instructions." Personal bot, Jira is trusted source. No fix. But add `?? ''` for null safety.

3. **null/undefined description** — `issue.description` may be null on issues without description. `.replace('{DESCRIPTION}', issue.description ?? '')` is safer.

## MEDIUM

4. **bun:test vs vitest** — bun:test is correct per project conventions. No change.

5. **Interval setup before try block** — If replyWithChatAction rejects, interval leaks. Move setup inside try. Low real risk but easy to fix cleanly.

6. **console.log convention** — Matches create.ts. No change.

7. **Test ordering not strict** — Testing temporal ordering is complex with current mock approach. Acceptable coverage. Let go.

8. **Missing generic error test** — No test for plain Error path ("Something went wrong"). Auto-fix.

## LOW

9-11. Let go (Clients TODO, clearInterval testing, makeCtx fragility).

## Auto-fixes planned
- Add `?? ''` for null description in prompt construction
- Move typingInterval setup inside try block
- Add generic error test to solve.test.ts
