# Code Review: section-03-utils

## Auto-fix
- Simplify pushLongText: single `return remaining` 
- Add ctx.match type-narrowing comment for RegExpMatchArray case
- Improve paragraph-preservation test to assert chunk content verbatim
- Add test asserting word-boundary split doesn't mid-cut words
- Add test for ctx.match === undefined

## Let go
- Space dropping in word-split (correct behavior — space is the separator)
- pushLongText side-effect pattern (works correctly, not worth refactoring)
- No 100+ chunk guard (personal bot, plan says "up to 99")
- Leading space in remainder (edge case, undocumented but acceptable)
- Module duplication with src/telegram/splitMessage.ts (intentional, different semantics)
- Test runner mismatch note (intentional bun:test)
