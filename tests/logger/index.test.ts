import { describe, it, expect, beforeEach, afterEach, spyOn } from "bun:test"
import { createLogger } from "../../src/logger/index"

describe("createLogger — JSON mode", () => {
  let writeSpy: ReturnType<typeof spyOn>

  beforeEach(() => {
    writeSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
  })

  afterEach(() => {
    writeSpy.mockRestore()
  })

  it("each log call writes one valid JSON line with level, ts, msg fields", () => {
    const logger = createLogger("info", "json")
    logger.info("hello world")
    expect(writeSpy).toHaveBeenCalledTimes(1)
    const line = (writeSpy.mock.calls[0][0] as string).trim()
    const parsed = JSON.parse(line)
    expect(parsed).toHaveProperty("level", "info")
    expect(parsed).toHaveProperty("ts")
    expect(parsed).toHaveProperty("msg", "hello world")
  })

  it("meta object fields are merged into root of log line", () => {
    const logger = createLogger("info", "json")
    logger.info("hello", { reqId: "42" })
    const line = (writeSpy.mock.calls[0][0] as string).trim()
    const parsed = JSON.parse(line)
    expect(parsed).toHaveProperty("reqId", "42")
    expect(parsed).toHaveProperty("msg", "hello")
  })
})

describe("createLogger — TTY mode ANSI suppression", () => {
  let writeSpy: ReturnType<typeof spyOn>
  const originalEnv = { ...process.env }

  beforeEach(() => {
    writeSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
  })

  afterEach(() => {
    writeSpy.mockRestore()
    // restore env
    for (const key of ["NO_COLOR", "CLICOLOR", "TERM"]) {
      if (originalEnv[key] === undefined) {
        delete process.env[key]
      } else {
        process.env[key] = originalEnv[key]
      }
    }
  })

  function hasAnsi(output: string): boolean {
    return /\x1b\[/.test(output)
  }

  it("NO_COLOR set → output contains no ANSI escape codes", () => {
    process.env.NO_COLOR = ""
    const logger = createLogger("info", "tty")
    logger.info("test")
    const out = writeSpy.mock.calls[0][0] as string
    expect(hasAnsi(out)).toBe(false)
  })

  it("CLICOLOR=0 → no ANSI codes", () => {
    delete process.env.NO_COLOR
    process.env.CLICOLOR = "0"
    const logger = createLogger("info", "tty")
    logger.info("test")
    const out = writeSpy.mock.calls[0][0] as string
    expect(hasAnsi(out)).toBe(false)
  })

  it("TERM=dumb → no ANSI codes", () => {
    delete process.env.NO_COLOR
    delete process.env.CLICOLOR
    process.env.TERM = "dumb"
    const logger = createLogger("info", "tty")
    logger.info("test")
    const out = writeSpy.mock.calls[0][0] as string
    expect(hasAnsi(out)).toBe(false)
  })

  it("process.stdout.isTTY falsy → no ANSI codes", () => {
    delete process.env.NO_COLOR
    delete process.env.CLICOLOR
    delete process.env.TERM
    const origDescriptor = Object.getOwnPropertyDescriptor(process.stdout, "isTTY")
    Object.defineProperty(process.stdout, "isTTY", { value: false, configurable: true, writable: true })
    const logger = createLogger("info", "tty")
    logger.info("test")
    if (origDescriptor) {
      Object.defineProperty(process.stdout, "isTTY", origDescriptor)
    }
    const out = writeSpy.mock.calls[0][0] as string
    expect(hasAnsi(out)).toBe(false)
  })
})

describe("createLogger — level gating", () => {
  let writeSpy: ReturnType<typeof spyOn>

  beforeEach(() => {
    writeSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
  })

  afterEach(() => {
    writeSpy.mockRestore()
  })

  it("debug messages suppressed when level = 'info'", () => {
    const logger = createLogger("info", "json")
    logger.debug("secret")
    expect(writeSpy).not.toHaveBeenCalled()
  })

  it("debug messages emitted when level = 'debug'", () => {
    const logger = createLogger("debug", "json")
    logger.debug("visible")
    expect(writeSpy).toHaveBeenCalledTimes(1)
    const line = (writeSpy.mock.calls[0][0] as string).trim()
    const parsed = JSON.parse(line)
    expect(parsed.msg).toBe("visible")
  })
})
