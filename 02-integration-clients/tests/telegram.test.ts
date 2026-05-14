import { vi, describe, it, expect, beforeEach } from 'vitest'
import { splitMessage } from '../src/telegram/splitMessage'
import {
  JiraAuthError,
  JiraNotFoundError,
  InvalidTransitionError,
  ClaudeTimeoutError,
  ClaudeExitError,
} from '../src/errors'

const { mockSendMessage, mockCommand, mockOn, mockCatch, mockStart, mockStop } = vi.hoisted(() => ({
  mockSendMessage: vi.fn().mockResolvedValue({}),
  mockCommand: vi.fn(),
  mockOn: vi.fn(),
  mockCatch: vi.fn(),
  mockStart: vi.fn().mockResolvedValue(undefined),
  mockStop: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('grammy', () => ({
  Bot: vi.fn(() => ({
    command: mockCommand,
    on: mockOn,
    catch: mockCatch,
    start: mockStart,
    stop: mockStop,
    api: { sendMessage: mockSendMessage },
  })),
}))

// Import after mock setup
const { TelegramClient } = await import('../src/telegram/TelegramClient')

const mockLogger = { info: vi.fn(), error: vi.fn() }
const config = { token: 'test-token', allowedUserIds: [123, 456] }

function makeClient() {
  return new TelegramClient(config, mockLogger)
}

// Helper: extract the callback registered with a mock vi.fn call
function getCapturedHandler(mockFn: ReturnType<typeof vi.fn>, callIndex = 0, argIndex = 1) {
  return mockFn.mock.calls[callIndex][argIndex] as (...args: unknown[]) => Promise<unknown>
}

// Helper: build a minimal grammY ctx object
function makeCtx(userId: number, chatId: number, text: string) {
  return {
    from: { id: userId },
    chat: { id: chatId },
    message: { text },
    reply: vi.fn().mockResolvedValue({}),
  }
}

// Helper: build a grammY BotError-like object
function makeBotError(error: Error, chatId: number) {
  return {
    error,
    ctx: { chat: { id: chatId }, reply: vi.fn().mockResolvedValue({}) },
  }
}

// ─── splitMessage ──────────────────────────────────────────────────────────

describe('splitMessage', () => {
  it('short text returns single-element array', () => {
    expect(splitMessage('hello')).toEqual(['hello'])
  })

  it('text exactly at limit returns single-element array', () => {
    const text = 'a'.repeat(4096)
    const result = splitMessage(text)
    expect(result).toHaveLength(1)
    expect(result[0]).toHaveLength(4096)
  })

  it('splits on paragraph boundaries', () => {
    const text = 'para1\n\npara2\n\npara3'
    // limit=10: 'para1' (5) + '\n\n' (2) + 'para2' (5) = 12 > 10, forces split
    const result = splitMessage(text, 10)
    expect(result.length).toBeGreaterThan(1)
    const joined = result.join('\n\n')
    expect(joined).toContain('para1')
    expect(joined).toContain('para2')
  })

  it('splits long paragraph at word boundaries', () => {
    const words = Array.from({ length: 100 }, (_, i) => `word${i}`)
    const text = words.join(' ')
    const result = splitMessage(text, 50)
    for (const chunk of result) {
      expect(chunk.length).toBeLessThanOrEqual(50)
    }
    expect(result.join(' ')).toBe(text)
  })

  it('hard-splits single word exceeding limit', () => {
    const bigWord = 'x'.repeat(200)
    const result = splitMessage(bigWord, 100)
    expect(result).toHaveLength(2)
    for (const chunk of result) {
      expect(chunk.length).toBeLessThanOrEqual(100)
    }
  })

  it('never returns empty array for non-empty input', () => {
    expect(splitMessage('a', 10).length).toBeGreaterThan(0)
  })
})

// ─── Construction ──────────────────────────────────────────────────────────

describe('TelegramClient construction', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('registers error middleware via bot.catch at construction time', () => {
    makeClient()
    expect(mockCatch).toHaveBeenCalledOnce()
  })

  it('registers non-command fallback via bot.on at construction time', () => {
    makeClient()
    expect(mockOn).toHaveBeenCalledWith('message:text', expect.any(Function))
  })
})

// ─── onCommand ─────────────────────────────────────────────────────────────

describe('onCommand', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('calls bot.command with the given command name', () => {
    const client = makeClient()
    client.onCommand('start', vi.fn())
    expect(mockCommand).toHaveBeenCalledWith('start', expect.any(Function))
  })

  it('authorized userId — handler is invoked', async () => {
    const client = makeClient()
    const handler = vi.fn().mockResolvedValue(undefined)
    client.onCommand('test', handler)

    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
    const ctx = makeCtx(123, 99, '/test arg1 arg2')
    await grammyHandler(ctx)

    expect(handler).toHaveBeenCalledOnce()
  })

  it('unauthorized userId — handler is NOT invoked', async () => {
    const client = makeClient()
    const handler = vi.fn()
    client.onCommand('test', handler)

    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
    const ctx = makeCtx(999, 99, '/test')
    await grammyHandler(ctx)

    expect(handler).not.toHaveBeenCalled()
  })

  it('unauthorized userId — no reply is sent', async () => {
    const client = makeClient()
    client.onCommand('test', vi.fn())

    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
    const ctx = makeCtx(999, 99, '/test')
    await grammyHandler(ctx)

    expect(ctx.reply).not.toHaveBeenCalled()
  })

  it('empty allowedUserIds blocks all commands', async () => {
    const client = new TelegramClient({ token: 'tok', allowedUserIds: [] }, mockLogger)
    const handler = vi.fn()
    client.onCommand('test', handler)

    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
    await grammyHandler(makeCtx(123, 99, '/test'))

    expect(handler).not.toHaveBeenCalled()
  })

  it('CommandContext has correct chatId, userId, args, rawText', async () => {
    const client = makeClient()
    let capturedCtx: unknown
    client.onCommand('cmd', async (ctx) => { capturedCtx = ctx })

    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
    await grammyHandler(makeCtx(123, 42, '/cmd foo bar'))

    expect((capturedCtx as { chatId: number }).chatId).toBe(42)
    expect((capturedCtx as { userId: number }).userId).toBe(123)
    expect((capturedCtx as { args: string[] }).args).toEqual(['foo', 'bar'])
    expect((capturedCtx as { rawText: string }).rawText).toBe('/cmd foo bar')
  })
})

// ─── sendMessage ───────────────────────────────────────────────────────────

describe('sendMessage', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('short text — exactly one bot.api.sendMessage call', async () => {
    const client = makeClient()
    await client.sendMessage(1, 'hello')
    expect(mockSendMessage).toHaveBeenCalledOnce()
    expect(mockSendMessage).toHaveBeenCalledWith(1, 'hello')
  })

  it('text over 4096 chars — multiple sendMessage calls', async () => {
    const client = makeClient()
    const longText = 'word '.repeat(1000)
    await client.sendMessage(1, longText)
    expect(mockSendMessage.mock.calls.length).toBeGreaterThan(1)
  })

  it('CommandContext.reply routes through sendMessage (chunking applied)', async () => {
    const client = makeClient()
    client.onCommand('cmd', async (ctx) => {
      await ctx.reply('hello')
    })

    const grammyHandler = getCapturedHandler(mockCommand, 0, 1)
    await grammyHandler(makeCtx(123, 5, '/cmd'))

    expect(mockSendMessage).toHaveBeenCalledWith(5, 'hello')
  })
})

// ─── Lifecycle ─────────────────────────────────────────────────────────────

describe('lifecycle', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('startPolling calls bot.start with drop_pending_updates: true', () => {
    const client = makeClient()
    client.startPolling()
    expect(mockStart).toHaveBeenCalledWith({ drop_pending_updates: true })
  })

  it('stopPolling awaits bot.stop()', async () => {
    const client = makeClient()
    await client.stopPolling()
    expect(mockStop).toHaveBeenCalledOnce()
  })
})

// ─── Error middleware ───────────────────────────────────────────────────────

describe('error middleware', () => {
  beforeEach(() => { vi.clearAllMocks() })

  async function invokeErrorHandler(error: Error, chatId = 10) {
    makeClient()
    const handler = mockCatch.mock.calls[0][0] as (e: unknown) => Promise<void>
    const botErr = makeBotError(error, chatId)
    await handler(botErr)
    return botErr.ctx.reply
  }

  it('JiraAuthError → reply mentions authentication', async () => {
    const reply = await invokeErrorHandler(new JiraAuthError())
    expect(reply).toHaveBeenCalledOnce()
    const msg = (reply.mock.calls[0][0] as string).toLowerCase()
    expect(msg).toContain('authentication')
  })

  it('JiraNotFoundError → reply contains issue key', async () => {
    const reply = await invokeErrorHandler(new JiraNotFoundError('PROJ-42'))
    const msg = reply.mock.calls[0][0] as string
    expect(msg).toContain('PROJ-42')
  })

  it('InvalidTransitionError → reply lists available transitions', async () => {
    const reply = await invokeErrorHandler(new InvalidTransitionError('Close', ['Resolve', 'Reopen']))
    const msg = reply.mock.calls[0][0] as string
    expect(msg).toContain('Resolve')
    expect(msg).toContain('Reopen')
  })

  it('ClaudeTimeoutError → reply contains timeout duration', async () => {
    const reply = await invokeErrorHandler(new ClaudeTimeoutError(5000))
    const msg = reply.mock.calls[0][0] as string
    expect(msg).toContain('5000')
  })

  it('ClaudeExitError → reply contains exit code', async () => {
    const reply = await invokeErrorHandler(new ClaudeExitError(1, 'stderr output'))
    const msg = reply.mock.calls[0][0] as string
    expect(msg).toContain('exit 1')
  })

  it('unknown error → generic reply', async () => {
    const reply = await invokeErrorHandler(new Error('unexpected'))
    const msg = (reply.mock.calls[0][0] as string).toLowerCase()
    expect(msg).toContain('wrong')
  })
})
