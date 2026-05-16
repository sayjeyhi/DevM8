# Interview Transcript: section-02-auth-middleware

## Auto-fixes applied

1. **Tighten authorized-path logger test**: Assert logger was not called at all (not just no "unauthorized" event).
2. **Add ctx.chat undefined test**: Inline query scenario where ctx.chat is undefined.
3. **Update section doc**: Correct import paths (src/bot/ layout).

## Let go

- chatId: undefined in logs (plan allows it, inline queries are edge case)
- Plan references grammytest/vi.fn() (plan quality issue, not code issue)
