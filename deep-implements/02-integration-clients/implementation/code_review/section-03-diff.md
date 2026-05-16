diff --git a/02-integration-clients/index.ts b/02-integration-clients/index.ts
index ae1a0b8..3808ca2 100644
--- a/02-integration-clients/index.ts
+++ b/02-integration-clients/index.ts
@@ -1,7 +1,8 @@
 export * from './src/errors'
+export * from './src/telegram/TelegramClient'
+export * from './src/telegram/types'
+export * from './src/telegram/splitMessage'
 
-// TODO (section-03): export * from './src/telegram/TelegramClient'
-// TODO (section-03): export * from './src/telegram/types'
 // TODO (section-04): export * from './src/jira/JiraClient'
 // TODO (section-04): export * from './src/jira/types'
 // TODO (section-05): export * from './src/claude/ClaudeClient'
diff --git a/02-integration-clients/src/telegram/TelegramClient.ts b/02-integration-clients/src/telegram/TelegramClient.ts
new file mode 100644
index 0000000..74a98c1
--- /dev/null
+++ b/02-integration-clients/src/telegram/TelegramClient.ts
@@ -0,0 +1,93 @@
+import { Bot } from 'grammy'
+import type { TelegramConfig, CommandHandler } from './types'
+import { splitMessage } from './splitMessage'
+import {
+  JiraAuthError,
+  JiraNotFoundError,
+  InvalidTransitionError,
+  ClaudeTimeoutError,
+  ClaudeExitError,
+} from '../errors'
+
+interface Logger {
+  info(obj: object): void
+  error(obj: object): void
+}
+
+export class TelegramClient {
+  private readonly bot: Bot
+  private readonly config: TelegramConfig
+  private readonly logger: Logger
+
+  constructor(config: TelegramConfig, logger: Logger) {
+    this.config = config
+    this.logger = logger
+    this.bot = new Bot(config.token)
+
+    this.bot.catch(async (err) => {
+      const chatId = err.ctx.chat?.id ?? 0
+      const error = err.error as Error & { type?: string; issueKey?: string; attempted?: string; available?: string[]; timeoutMs?: number; exitCode?: number }
+
+      let message: string
+
+      if (error instanceof JiraAuthError) {
+        message = 'Jira authentication failed. Check your API token.'
+      } else if (error instanceof JiraNotFoundError) {
+        message = `Issue ${error.issueKey} not found.`
+      } else if (error instanceof InvalidTransitionError) {
+        message = `Transition '${error.attempted}' not found. Valid: ${error.available.join(', ')}`
+      } else if (error instanceof ClaudeTimeoutError) {
+        message = `Claude timed out after ${error.timeoutMs}ms.`
+      } else if (error instanceof ClaudeExitError) {
+        message = `Claude subprocess failed (exit ${error.exitCode}).`
+      } else {
+        message = 'Something went wrong. Try again.'
+        this.logger.error({ event: 'error', errorType: 'unknown', chatId })
+      }
+
+      this.logger.info({ event: 'error', errorType: (error as { type?: string }).type ?? 'unknown', chatId })
+      await err.ctx.reply(message)
+    })
+
+    this.bot.on('message:text', (ctx) => ctx.reply('Use /help to see available commands.'))
+  }
+
+  onCommand(command: string, handler: CommandHandler): void {
+    this.bot.command(command, async (ctx) => {
+      const userId = ctx.from?.id
+      if (userId === undefined || !this.config.allowedUserIds.includes(userId)) {
+        this.logger.info({ event: 'unauthorized', chatId: ctx.chat.id, command })
+        return
+      }
+
+      const chatId = ctx.chat.id
+      const rawText = ctx.message?.text ?? ''
+      const args = rawText.split(' ').slice(1)
+
+      this.logger.info({ event: 'command', chatId, command, argCount: args.length })
+
+      await handler({
+        chatId,
+        userId,
+        command,
+        args,
+        rawText,
+        reply: (text: string) => this.sendMessage(chatId, text),
+      })
+    })
+  }
+
+  async sendMessage(chatId: number, text: string): Promise<void> {
+    for (const chunk of splitMessage(text)) {
+      await this.bot.api.sendMessage(chatId, chunk)
+    }
+  }
+
+  startPolling(): void {
+    this.bot.start({ drop_pending_updates: true })
+  }
+
+  async stopPolling(): Promise<void> {
+    await this.bot.stop()
+  }
+}
diff --git a/02-integration-clients/src/telegram/splitMessage.ts b/02-integration-clients/src/telegram/splitMessage.ts
new file mode 100644
index 0000000..6171a6b
--- /dev/null
+++ b/02-integration-clients/src/telegram/splitMessage.ts
@@ -0,0 +1,51 @@
+export function splitMessage(text: string, limit = 4096): string[] {
+  if (text.length <= limit) return [text]
+
+  const chunks: string[] = []
+  let current = ''
+
+  for (const para of text.split('\n\n')) {
+    for (const wordChunk of splitParagraph(para, limit)) {
+      if (current === '') {
+        current = wordChunk
+      } else if (current.length + 2 + wordChunk.length <= limit) {
+        current += '\n\n' + wordChunk
+      } else {
+        chunks.push(current)
+        current = wordChunk
+      }
+    }
+  }
+
+  if (current) chunks.push(current)
+  return chunks
+}
+
+function splitParagraph(text: string, limit: number): string[] {
+  if (text.length <= limit) return [text]
+
+  const chunks: string[] = []
+  let current = ''
+
+  for (const word of text.split(' ')) {
+    if (word.length > limit) {
+      if (current) { chunks.push(current); current = '' }
+      let remaining = word
+      while (remaining.length > limit) {
+        chunks.push(remaining.slice(0, limit))
+        remaining = remaining.slice(limit)
+      }
+      if (remaining) current = remaining
+    } else if (current === '') {
+      current = word
+    } else if (current.length + 1 + word.length <= limit) {
+      current += ' ' + word
+    } else {
+      chunks.push(current)
+      current = word
+    }
+  }
+
+  if (current) chunks.push(current)
+  return chunks
+}
diff --git a/02-integration-clients/src/telegram/types.ts b/02-integration-clients/src/telegram/types.ts
new file mode 100644
index 0000000..eef315d
--- /dev/null
+++ b/02-integration-clients/src/telegram/types.ts
@@ -0,0 +1,15 @@
+export interface TelegramConfig {
+  token: string
+  allowedUserIds: number[]
+}
+
+export type CommandHandler = (ctx: CommandContext) => Promise<void>
+
+export interface CommandContext {
+  chatId: number
+  userId: number
+  command: string
+  args: string[]
+  rawText: string
+  reply(text: string): Promise<void>
+}
diff --git a/02-integration-clients/tests/telegram.test.ts b/02-integration-clients/tests/telegram.test.ts
new file mode 100644
index 0000000..e7edde1
--- /dev/null
+++ b/02-integration-clients/tests/telegram.test.ts
@@ -0,0 +1,302 @@
+import { vi, describe, it, expect, beforeEach } from 'vitest'
+import { splitMessage } from '../src/telegram/splitMessage'
+import {
+  JiraAuthError,
+  JiraNotFoundError,
+  InvalidTransitionError,
+  ClaudeTimeoutError,
+  ClaudeExitError,
+} from '../src/errors'
+
+const { mockSendMessage, mockCommand, mockOn, mockCatch, mockStart, mockStop } = vi.hoisted(() => ({
+  mockSendMessage: vi.fn().mockResolvedValue({}),
+  mockCommand: vi.fn(),
+  mockOn: vi.fn(),
+  mockCatch: vi.fn(),
+  mockStart: vi.fn(),
+  mockStop: vi.fn().mockResolvedValue(undefined),
+}))
+
+vi.mock('grammy', () => ({
+  Bot: vi.fn(() => ({
+    command: mockCommand,
+    on: mockOn,
+    catch: mockCatch,
+    start: mockStart,
+    stop: mockStop,
+    api: { sendMessage: mockSendMessage },
+  })),
+}))
+
+// Import after mock setup
+const { TelegramClient } = await import('../src/telegram/TelegramClient')
+
+const mockLogger = { info: vi.fn(), error: vi.fn() }
+const config = { token: 'test-token', allowedUserIds: [123, 456] }
+
+function makeClient() {
+  return new TelegramClient(config, mockLogger)
+}
+
+// Helper: extract the callback registered with a mock vi.fn call
+function getCapturedHandler(mockFn: ReturnType<typeof vi.fn>, callIndex = 0, argIndex = 1) {
+  return mockFn.mock.calls[callIndex][argIndex] as (...args: unknown[]) => Promise<unknown>
+}
+
+// Helper: build a minimal grammY ctx object
+function makeCtx(userId: number, chatId: number, text: string) {
+  return {
+    from: { id: userId },
+    chat: { id: chatId },
+    message: { text },
+    reply: vi.fn().mockResolvedValue({}),
+  }
+}
+
+// Helper: build a grammY BotError-like object
+function makeBotError(error: Error, chatId: number) {
+  return {
+    error,
+    ctx: { chat: { id: chatId }, reply: vi.fn().mockResolvedValue({}) },
+  }
+}
+
+// ─── splitMessage ──────────────────────────────────────────────────────────
+
+describe('splitMessage', () => {
+  it('short text returns single-element array', () => {
+    expect(splitMessage('hello')).toEqual(['hello'])
+  })
+
+  it('text exactly at limit returns single-element array', () => {
+    const text = 'a'.repeat(4096)
+    const result = splitMessage(text)
+    expect(result).toHaveLength(1)
+    expect(result[0]).toHaveLength(4096)
+  })
+
+  it('splits on paragraph boundaries', () => {
+    const text = 'para1\n\npara2\n\npara3'
+    // limit=10: 'para1' (5) + '\n\n' (2) + 'para2' (5) = 12 > 10, forces split
+    const result = splitMessage(text, 10)
+    expect(result.length).toBeGreaterThan(1)
+    const joined = result.join('\n\n')
+    expect(joined).toContain('para1')
+    expect(joined).toContain('para2')
+  })
+
+  it('splits long paragraph at word boundaries', () => {
+    const words = Array.from({ length: 100 }, (_, i) => `word${i}`)
+    const text = words.join(' ')
+    const result = splitMessage(text, 50)
+    for (const chunk of result) {
+      expect(chunk.length).toBeLessThanOrEqual(50)
+    }
+    expect(result.join(' ')).toBe(text)
+  })
+
+  it('hard-splits single word exceeding limit', () => {
+    const bigWord = 'x'.repeat(200)
+    const result = splitMessage(bigWord, 100)
+    expect(result).toHaveLength(2)
+    for (const chunk of result) {
+      expect(chunk.length).toBeLessThanOrEqual(100)
+    }
+  })
+
+  it('never returns empty array for non-empty input', () => {
+    expect(splitMessage('a', 10).length).toBeGreaterThan(0)
+  })
+})
+
+// ─── Construction ──────────────────────────────────────────────────────────
+
+describe('TelegramClient construction', () => {
+  beforeEach(() => {
+    vi.clearAllMocks()
+  })
+
+  it('registers error middleware via bot.catch at construction time', () => {
+    makeClient()
+    expect(mockCatch).toHaveBeenCalledOnce()
+  })
+
+  it('registers non-command fallback via bot.on at construction time', () => {
+    makeClient()
+    expect(mockOn).toHaveBeenCalledWith('message:text', expect.any(Function))
+  })
+})
+
+// ─── onCommand ─────────────────────────────────────────────────────────────
+
+describe('onCommand', () => {
+  beforeEach(() => { vi.clearAllMocks() })
+
+  it('calls bot.command with the given command name', () => {
+    const client = makeClient()
+    client.onCommand('start', vi.fn())
+    expect(mockCommand).toHaveBeenCalledWith('start', expect.any(Function))
+  })
+
+  it('authorized userId — handler is invoked', async () => {
+    const client = makeClient()
+    const handler = vi.fn().mockResolvedValue(undefined)
+    client.onCommand('test', handler)
+
+    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
+    const ctx = makeCtx(123, 99, '/test arg1 arg2')
+    await grammyHandler(ctx)
+
+    expect(handler).toHaveBeenCalledOnce()
+  })
+
+  it('unauthorized userId — handler is NOT invoked', async () => {
+    const client = makeClient()
+    const handler = vi.fn()
+    client.onCommand('test', handler)
+
+    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
+    const ctx = makeCtx(999, 99, '/test')
+    await grammyHandler(ctx)
+
+    expect(handler).not.toHaveBeenCalled()
+  })
+
+  it('unauthorized userId — no reply is sent', async () => {
+    const client = makeClient()
+    client.onCommand('test', vi.fn())
+
+    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
+    const ctx = makeCtx(999, 99, '/test')
+    await grammyHandler(ctx)
+
+    expect(ctx.reply).not.toHaveBeenCalled()
+  })
+
+  it('empty allowedUserIds blocks all commands', async () => {
+    const client = new TelegramClient({ token: 'tok', allowedUserIds: [] }, mockLogger)
+    const handler = vi.fn()
+    client.onCommand('test', handler)
+
+    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
+    await grammyHandler(makeCtx(123, 99, '/test'))
+
+    expect(handler).not.toHaveBeenCalled()
+  })
+
+  it('CommandContext has correct chatId, userId, args, rawText', async () => {
+    const client = makeClient()
+    let capturedCtx: unknown
+    client.onCommand('cmd', async (ctx) => { capturedCtx = ctx })
+
+    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
+    await grammyHandler(makeCtx(123, 42, '/cmd foo bar'))
+
+    expect((capturedCtx as { chatId: number }).chatId).toBe(42)
+    expect((capturedCtx as { userId: number }).userId).toBe(123)
+    expect((capturedCtx as { args: string[] }).args).toEqual(['foo', 'bar'])
+    expect((capturedCtx as { rawText: string }).rawText).toBe('/cmd foo bar')
+  })
+})
+
+// ─── sendMessage ───────────────────────────────────────────────────────────
+
+describe('sendMessage', () => {
+  beforeEach(() => { vi.clearAllMocks() })
+
+  it('short text — exactly one bot.api.sendMessage call', async () => {
+    const client = makeClient()
+    await client.sendMessage(1, 'hello')
+    expect(mockSendMessage).toHaveBeenCalledOnce()
+    expect(mockSendMessage).toHaveBeenCalledWith(1, 'hello')
+  })
+
+  it('text over 4096 chars — multiple sendMessage calls', async () => {
+    const client = makeClient()
+    const longText = 'word '.repeat(1000)
+    await client.sendMessage(1, longText)
+    expect(mockSendMessage.mock.calls.length).toBeGreaterThan(1)
+  })
+
+  it('CommandContext.reply routes through sendMessage (chunking applied)', async () => {
+    const client = makeClient()
+    client.onCommand('cmd', async (ctx) => {
+      await ctx.reply('hello')
+    })
+
+    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
+    await grammyHandler(makeCtx(123, 5, '/cmd'))
+
+    expect(mockSendMessage).toHaveBeenCalledWith(5, 'hello')
+  })
+})
+
+// ─── Lifecycle ─────────────────────────────────────────────────────────────
+
+describe('lifecycle', () => {
+  beforeEach(() => { vi.clearAllMocks() })
+
+  it('startPolling calls bot.start with drop_pending_updates: true', () => {
+    const client = makeClient()
+    client.startPolling()
+    expect(mockStart).toHaveBeenCalledWith({ drop_pending_updates: true })
+  })
+
+  it('stopPolling awaits bot.stop()', async () => {
+    const client = makeClient()
+    await client.stopPolling()
+    expect(mockStop).toHaveBeenCalledOnce()
+  })
+})
+
+// ─── Error middleware ───────────────────────────────────────────────────────
+
+describe('error middleware', () => {
+  beforeEach(() => { vi.clearAllMocks() })
+
+  async function invokeErrorHandler(error: Error, chatId = 10) {
+    makeClient()
+    const handler = mockCatch.mock.calls[0][0] as (e: unknown) => Promise<void>
+    const botErr = makeBotError(error, chatId)
+    await handler(botErr)
+    return botErr.ctx.reply
+  }
+
+  it('JiraAuthError → reply mentions authentication', async () => {
+    const reply = await invokeErrorHandler(new JiraAuthError())
+    expect(reply).toHaveBeenCalledOnce()
+    const msg = (reply.mock.calls[0][0] as string).toLowerCase()
+    expect(msg).toContain('authentication')
+  })
+
+  it('JiraNotFoundError → reply contains issue key', async () => {
+    const reply = await invokeErrorHandler(new JiraNotFoundError('PROJ-42'))
+    const msg = reply.mock.calls[0][0] as string
+    expect(msg).toContain('PROJ-42')
+  })
+
+  it('InvalidTransitionError → reply lists available transitions', async () => {
+    const reply = await invokeErrorHandler(new InvalidTransitionError('Close', ['Resolve', 'Reopen']))
+    const msg = reply.mock.calls[0][0] as string
+    expect(msg).toContain('Resolve')
+    expect(msg).toContain('Reopen')
+  })
+
+  it('ClaudeTimeoutError → reply contains timeout duration', async () => {
+    const reply = await invokeErrorHandler(new ClaudeTimeoutError(5000))
+    const msg = reply.mock.calls[0][0] as string
+    expect(msg).toContain('5000')
+  })
+
+  it('ClaudeExitError → reply contains exit code', async () => {
+    const reply = await invokeErrorHandler(new ClaudeExitError(1, 'stderr output'))
+    const msg = reply.mock.calls[0][0] as string
+    expect(msg).toContain('1')
+  })
+
+  it('unknown error → generic reply', async () => {
+    const reply = await invokeErrorHandler(new Error('unexpected'))
+    const msg = (reply.mock.calls[0][0] as string).toLowerCase()
+    expect(msg).toContain('wrong')
+  })
+})
