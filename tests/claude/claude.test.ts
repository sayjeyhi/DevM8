import { describe, it, expect, mock, spyOn, beforeEach } from "bun:test"
import { ClaudeClient } from "../../src/claude/ClaudeClient"
import { ClaudeTimeoutError, ClaudeExitError } from "../../src/shared/errors"

const mockLogger = { info: mock(), error: mock() }

const baseConfig = {
  binaryPath: "/usr/local/bin/claude",
  timeoutMs: 5000,
}

function makeStream(text: string): ReadableStream<Uint8Array> {
  return new ReadableStream({
    start(ctrl) {
      ctrl.enqueue(new TextEncoder().encode(text))
      ctrl.close()
    },
  })
}

function makeMockProc(opts: { exitCode?: number; stdout?: string; stderr?: string } = {}) {
  const { exitCode = 0, stdout = "", stderr = "" } = opts
  return {
    stdin: { write: mock(), end: mock() },
    stdout: makeStream(stdout),
    stderr: makeStream(stderr),
    exitCode,
    exited: Promise.resolve(exitCode),
    kill: mock(),
  }
}

function makeHungProc(opts: { stdout?: string; stderr?: string } = {}) {
  const { stdout = "", stderr = "" } = opts
  let resolveExited!: (code: number) => void
  const exited = new Promise<number>(resolve => {
    resolveExited = resolve
  })
  const proc = {
    stdin: { write: mock(), end: mock() },
    stdout: makeStream(stdout),
    stderr: makeStream(stderr),
    exitCode: null as number | null,
    exited,
    kill: mock((signal: string) => {
      if (signal === "SIGKILL") {
        proc.exitCode = 137
        resolveExited(137)
      }
    }),
  }
  return proc
}

beforeEach(() => {
  mockLogger.info.mockClear()
  mockLogger.error.mockClear()
})

// Helper: spy on Bun.spawn + Bun.sleep, return restore function
function stubBun(proc: ReturnType<typeof makeMockProc> | ReturnType<typeof makeHungProc>) {
  const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(proc as never)
  const sleepSpy = spyOn(Bun, "sleep").mockResolvedValue(undefined as never)
  return {
    spawnSpy,
    sleepSpy,
    restore() {
      spawnSpy.mockRestore()
      sleepSpy.mockRestore()
    },
  }
}

// ─── Subprocess invocation ────────────────────────────────────────────────────

describe("subprocess invocation", () => {
  it("writes prompt to stdin and closes it; prompt not in argv", async () => {
    const proc = makeMockProc({ stdout: '{"result":"ok"}' })
    const { spawnSpy, restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      await client.ask("my secret prompt")

      expect(proc.stdin.write).toHaveBeenCalledWith("my secret prompt")
      expect(proc.stdin.end).toHaveBeenCalled()
      const args = (spawnSpy.mock.calls[0] as [string[]])[0]
      expect(args.join(" ")).not.toContain("my secret prompt")
    } finally { restore() }
  })

  it("deletes CLAUDECODE from env (not set to undefined)", async () => {
    process.env.CLAUDECODE = "1"
    const proc = makeMockProc({ stdout: '{"result":"ok"}' })
    const { spawnSpy, restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      await client.ask("prompt")

      const spawnOpts = (spawnSpy.mock.calls[0] as [string[], { env: Record<string, unknown> }])[1]
      expect("CLAUDECODE" in spawnOpts.env).toBe(false)
    } finally {
      delete process.env.CLAUDECODE
      restore()
    }
  })

  it("includes required fixed flags in args", async () => {
    const proc = makeMockProc({ stdout: '{"result":"ok"}' })
    const { spawnSpy, restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      await client.ask("prompt")

      const args = (spawnSpy.mock.calls[0] as [string[]])[0]
      expect(args).toContain("--print")
      expect(args).toContain("--bare")
      expect(args).toContain("--no-session-persistence")
      expect(args).toContain("--output-format")
      expect(args).toContain("json")
    } finally { restore() }
  })

  it("omits --model when no model configured", async () => {
    const proc = makeMockProc({ stdout: '{"result":"ok"}' })
    const { spawnSpy, restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      await client.ask("prompt")

      const args = (spawnSpy.mock.calls[0] as [string[]])[0]
      expect(args).not.toContain("--model")
    } finally { restore() }
  })

  it("includes --model from ClaudeConfig.model", async () => {
    const proc = makeMockProc({ stdout: '{"result":"ok"}' })
    const { spawnSpy, restore } = stubBun(proc)
    try {
      const client = new ClaudeClient({ ...baseConfig, model: "claude-3-opus" }, mockLogger)
      await client.ask("prompt")

      const args = (spawnSpy.mock.calls[0] as [string[]])[0]
      expect(args).toContain("--model")
      expect(args).toContain("claude-3-opus")
    } finally { restore() }
  })

  it("includes --model from AskOptions.model (overrides config)", async () => {
    const proc = makeMockProc({ stdout: '{"result":"ok"}' })
    const { spawnSpy, restore } = stubBun(proc)
    try {
      const client = new ClaudeClient({ ...baseConfig, model: "claude-3-opus" }, mockLogger)
      await client.ask("prompt", { model: "claude-3-sonnet" })

      const args = (spawnSpy.mock.calls[0] as [string[]])[0]
      expect(args).toContain("--model")
      expect(args).toContain("claude-3-sonnet")
      expect(args).not.toContain("claude-3-opus")
    } finally { restore() }
  })
})

// ─── Happy path ───────────────────────────────────────────────────────────────

describe("happy path", () => {
  it("returns result string from JSON stdout", async () => {
    const proc = makeMockProc({ exitCode: 0, stdout: '{"result":"some response"}' })
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      const result = await client.ask("test prompt")
      expect(result).toBe("some response")
    } finally { restore() }
  })

  it("returns only the result field, not the whole parsed object", async () => {
    const proc = makeMockProc({ exitCode: 0, stdout: '{"result":"hello","extra":"ignored"}' })
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      const result = await client.ask("prompt")
      expect(result).toBe("hello")
      expect(typeof result).toBe("string")
    } finally { restore() }
  })
})

// ─── Error paths ──────────────────────────────────────────────────────────────

describe("error paths", () => {
  it("non-zero exit → throws ClaudeExitError with exitCode and stderr", async () => {
    const proc = makeMockProc({ exitCode: 1, stderr: "command failed" })
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      const err = await client.ask("prompt").catch(e => e)
      expect(err).toBeInstanceOf(ClaudeExitError)
      expect((err as ClaudeExitError).exitCode).toBe(1)
      expect((err as ClaudeExitError).stderr).toBe("command failed")
    } finally { restore() }
  })

  it("exit 0 but invalid JSON stdout → throws Error containing raw stdout", async () => {
    const proc = makeMockProc({ exitCode: 0, stdout: "not valid json" })
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      const err = await client.ask("prompt").catch(e => e)
      expect(err).toBeInstanceOf(Error)
      expect((err as Error).message).toContain("not valid json")
    } finally { restore() }
  })

  it("exit 0 but result is not a string → throws Error", async () => {
    const proc = makeMockProc({ exitCode: 0, stdout: '{"result":null}' })
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      const err = await client.ask("prompt").catch(e => e)
      expect(err).toBeInstanceOf(Error)
      expect((err as Error).message).toContain("unexpected result type")
    } finally { restore() }
  })
})

// ─── Timeout behavior ─────────────────────────────────────────────────────────

describe("timeout behavior", () => {
  it("sends SIGTERM then SIGKILL and throws ClaudeTimeoutError", async () => {
    const proc = makeHungProc()
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient({ ...baseConfig, timeoutMs: 1 }, mockLogger)
      const err = await client.ask("prompt").catch(e => e)
      expect(err).toBeInstanceOf(ClaudeTimeoutError)
      expect(proc.kill).toHaveBeenCalledWith("SIGTERM")
      expect(proc.kill).toHaveBeenCalledWith("SIGKILL")
    } finally { restore() }
  })

  it("ClaudeTimeoutError carries the timeoutMs value", async () => {
    const proc = makeHungProc()
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient({ ...baseConfig, timeoutMs: 1 }, mockLogger)
      const err = await client.ask("prompt").catch(e => e)
      expect(err).toBeInstanceOf(ClaudeTimeoutError)
      expect((err as ClaudeTimeoutError).timeoutMs).toBe(1)
    } finally { restore() }
  })

  it("AskOptions.timeoutMs overrides config timeout", async () => {
    const proc = makeHungProc()
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient({ ...baseConfig, timeoutMs: 60000 }, mockLogger)
      const err = await client.ask("prompt", { timeoutMs: 1 }).catch(e => e)
      expect(err).toBeInstanceOf(ClaudeTimeoutError)
      expect((err as ClaudeTimeoutError).timeoutMs).toBe(1)
    } finally { restore() }
  })

  it("throws ClaudeTimeoutError (not ClaudeExitError) when kill causes non-zero exit", async () => {
    const proc = makeHungProc()
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient({ ...baseConfig, timeoutMs: 1 }, mockLogger)
      const err = await client.ask("prompt").catch(e => e)
      expect(err).toBeInstanceOf(ClaudeTimeoutError)
      expect(err).not.toBeInstanceOf(ClaudeExitError)
    } finally { restore() }
  })

  it("does not send SIGKILL if process exits during grace period", async () => {
    let resolveExited!: (code: number) => void
    const exited = new Promise<number>(resolve => { resolveExited = resolve })
    const proc = {
      stdin: { write: mock(), end: mock() },
      stdout: makeStream(""),
      stderr: makeStream(""),
      exitCode: null as number | null,
      exited,
      kill: mock((signal: string) => {
        if (signal === "SIGTERM") {
          proc.exitCode = 15
          resolveExited(15)
        }
      }),
    }

    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient({ ...baseConfig, timeoutMs: 1 }, mockLogger)
      await client.ask("prompt").catch(() => {})
      expect(proc.kill).toHaveBeenCalledWith("SIGTERM")
      expect(proc.kill).not.toHaveBeenCalledWith("SIGKILL")
    } finally { restore() }
  })

  it("clears timer on successful call (no timer leak)", async () => {
    const clearTimeoutSpy = spyOn(globalThis, "clearTimeout")
    const proc = makeMockProc({ stdout: '{"result":"done"}' })
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      await client.ask("prompt")
      expect(clearTimeoutSpy).toHaveBeenCalled()
    } finally {
      restore()
      clearTimeoutSpy.mockRestore()
    }
  })
})

// ─── Concurrent stdout/stderr drain ──────────────────────────────────────────

describe("stdout drain", () => {
  it("reads stdout and stderr concurrently via Promise.all (no deadlock)", async () => {
    const proc = makeMockProc({ exitCode: 0, stdout: '{"result":"data"}', stderr: "some warning" })
    const { restore } = stubBun(proc)
    try {
      const client = new ClaudeClient(baseConfig, mockLogger)
      const result = await client.ask("prompt")
      expect(result).toBe("data")
    } finally { restore() }
  })
})
