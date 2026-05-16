diff --git a/src/bot/middleware/auth.ts b/src/bot/middleware/auth.ts
index 336ce12..caa2c99 100644
--- a/src/bot/middleware/auth.ts
+++ b/src/bot/middleware/auth.ts
@@ -1 +1,27 @@
-export {}
+import type { Context, MiddlewareFn } from "grammy"
+
+/**
+ * Creates a grammY middleware that silently drops updates from users not in
+ * the allowlist. Never replies to unauthorized users — silence is intentional
+ * to avoid confirming the bot's existence.
+ *
+ * @param allowedIds - Set<number> of permitted Telegram user IDs. O(1) lookup.
+ * @param logger - Injectable logger. Receives plain objects only; never receives
+ *   userId (PII). Defaults to console.log.
+ */
+export function createAuthMiddleware(
+  allowedIds: Set<number>,
+  logger: (entry: Record<string, unknown>) => void = e => console.log(e),
+): MiddlewareFn<Context> {
+  return async (ctx, next) => {
+    const userId = ctx.from?.id
+
+    if (userId === undefined || !allowedIds.has(userId)) {
+      // Log chatId only — never userId (PII)
+      logger({ event: "unauthorized", chatId: ctx.chat?.id })
+      return
+    }
+
+    return next()
+  }
+}
diff --git a/tests/bot/middleware/auth.test.ts b/tests/bot/middleware/auth.test.ts
new file mode 100644
index 0000000..05a55fe
--- /dev/null
+++ b/tests/bot/middleware/auth.test.ts
@@ -0,0 +1,70 @@
+import { describe, it, expect, mock } from "bun:test"
+import { createAuthMiddleware } from "../../../src/bot/middleware/auth"
+
+function makeCtx(userId: number | undefined, chatId: number | undefined = 1) {
+  return {
+    from: userId !== undefined ? { id: userId } : undefined,
+    chat: chatId !== undefined ? { id: chatId } : undefined,
+    reply: mock().mockResolvedValue({}),
+  }
+}
+
+describe("createAuthMiddleware", () => {
+  const allowedIds = new Set([12345, 67890])
+
+  it("authorized user — next() is called", async () => {
+    const middleware = createAuthMiddleware(allowedIds)
+    const ctx = makeCtx(12345, 99)
+    const next = mock().mockResolvedValue(undefined)
+    await middleware(ctx as never, next)
+    expect(next).toHaveBeenCalledTimes(1)
+  })
+
+  it("unauthorized user — next() is NOT called, no reply sent", async () => {
+    const middleware = createAuthMiddleware(allowedIds)
+    const ctx = makeCtx(99999, 99)
+    const next = mock()
+    await middleware(ctx as never, next)
+    expect(next).not.toHaveBeenCalled()
+    expect(ctx.reply).not.toHaveBeenCalled()
+  })
+
+  it("ctx.from is undefined — treated as unauthorized, no crash", async () => {
+    const middleware = createAuthMiddleware(allowedIds)
+    const ctx = makeCtx(undefined, 99)
+    const next = mock()
+    await expect(middleware(ctx as never, next)).resolves.toBeUndefined()
+    expect(next).not.toHaveBeenCalled()
+  })
+
+  it("empty Set — all users unauthorized", async () => {
+    const middleware = createAuthMiddleware(new Set<number>())
+    const ctx = makeCtx(12345, 99)
+    const next = mock()
+    await middleware(ctx as never, next)
+    expect(next).not.toHaveBeenCalled()
+  })
+
+  it("unauthorized attempt — logger receives { event, chatId } without userId", async () => {
+    const logger = mock()
+    const middleware = createAuthMiddleware(new Set([999]), logger)
+    const ctx = makeCtx(12345, 42)
+    await middleware(ctx as never, mock())
+    expect(logger).toHaveBeenCalledTimes(1)
+    const logged = logger.mock.calls[0][0] as Record<string, unknown>
+    expect(logged.event).toBe("unauthorized")
+    expect(logged.chatId).toBe(42)
+    expect("userId" in logged).toBe(false)
+  })
+
+  it("authorized attempt — logger NOT called with unauthorized event", async () => {
+    const logger = mock()
+    const middleware = createAuthMiddleware(new Set([12345]), logger)
+    const ctx = makeCtx(12345, 99)
+    await middleware(ctx as never, mock().mockResolvedValue(undefined))
+    const unauthorizedCalls = logger.mock.calls.filter(
+      (call: unknown[]) => (call[0] as Record<string, unknown>)?.event === "unauthorized"
+    )
+    expect(unauthorizedCalls).toHaveLength(0)
+  })
+})
