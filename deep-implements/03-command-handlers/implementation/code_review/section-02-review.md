# Code Review: section-02-auth-middleware

## Issues

### 1. PATH DRIFT (medium)
Plan says `src/middleware/auth.ts`; actual is `src/bot/middleware/auth.ts`. Section-07 will import from the plan's path. Must update plan doc to document the correct path.

### 2. AUTHORIZED PATH — logger test too loose (low-medium)
Test 6 only checks logger was NOT called with `event: unauthorized`. Should assert logger was not called AT ALL on authorized path.

### 3. chatId: undefined when ctx.chat absent (low)
Inline queries etc. have no `ctx.chat` → logs `{ chatId: undefined }`. Per plan this is allowed, but worth noting.

### 4. Plan references grammytest/vi.fn() (informational)
Plan is wrong — bun:test/mock() is correct. Update plan doc.

### 5. ctx.chat undefined not tested (missing coverage)
No test where `ctx.chat` is undefined (inline query). Low priority.
