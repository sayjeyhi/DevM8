diff --git a/01-core-daemon/src/logger/index.ts b/01-core-daemon/src/logger/index.ts
new file mode 100644
index 0000000..07dd13c
--- /dev/null
+++ b/01-core-daemon/src/logger/index.ts
@@ -0,0 +1,61 @@
+export interface Logger {
+  info(msg: string, meta?: object): void
+  error(msg: string, meta?: object): void
+  warn(msg: string, meta?: object): void
+  debug(msg: string, meta?: object): void
+}
+
+const LEVEL_PRIORITY = { debug: 0, info: 1, warn: 2, error: 3 } as const
+
+type Level = keyof typeof LEVEL_PRIORITY
+
+const ANSI = {
+  reset: "\x1b[0m",
+  red: "\x1b[31m",
+  yellow: "\x1b[33m",
+  dim: "\x1b[2m",
+} as const
+
+const LEVEL_COLOR: Record<Level, string> = {
+  debug: ANSI.dim,
+  info: "",
+  warn: ANSI.yellow,
+  error: ANSI.red,
+}
+
+export function createLogger(
+  level: "info" | "debug" | "error",
+  mode?: "tty" | "json"
+): Logger {
+  const useColor =
+    process.env.NO_COLOR === undefined &&
+    process.env.CLICOLOR !== "0" &&
+    process.env.TERM !== "dumb" &&
+    Boolean(process.stdout.isTTY)
+
+  const effectiveMode = mode ?? (process.stdout.isTTY ? "tty" : "json")
+
+  function emit(msgLevel: Level, msg: string, meta?: object): void {
+    if (LEVEL_PRIORITY[msgLevel] < LEVEL_PRIORITY[level as Level]) return
+
+    if (effectiveMode === "json") {
+      const line = JSON.stringify({ level: msgLevel, ts: new Date().toISOString(), msg, ...meta })
+      process.stdout.write(line + "\n")
+    } else {
+      const label = msgLevel.toUpperCase().padEnd(5)
+      const colored =
+        useColor && LEVEL_COLOR[msgLevel]
+          ? `${LEVEL_COLOR[msgLevel]}[${label}]${ANSI.reset}`
+          : `[${label}]`
+      const metaPart = meta && Object.keys(meta).length > 0 ? `  ${JSON.stringify(meta)}` : ""
+      process.stdout.write(`${colored} ${msg}${metaPart}\n`)
+    }
+  }
+
+  return {
+    info: (msg, meta) => emit("info", msg, meta),
+    error: (msg, meta) => emit("error", msg, meta),
+    warn: (msg, meta) => emit("warn", msg, meta),
+    debug: (msg, meta) => emit("debug", msg, meta),
+  }
+}
diff --git a/01-core-daemon/src/logger/rotate.ts b/01-core-daemon/src/logger/rotate.ts
new file mode 100644
index 0000000..3c5bbf0
--- /dev/null
+++ b/01-core-daemon/src/logger/rotate.ts
@@ -0,0 +1,32 @@
+import { rename } from "fs/promises"
+
+export async function rotateIfNeeded(
+  logFile: string,
+  maxBytes: number = 10 * 1024 * 1024,
+  keepCount: number = 5
+): Promise<void> {
+  const f = Bun.file(logFile)
+  if (!(await f.exists())) return
+  if (f.size < maxBytes) return
+
+  // Shift existing rotated files: app.log.(keepCount-1) → app.log.keepCount, etc.
+  for (let i = keepCount - 1; i >= 1; i--) {
+    const src = `${logFile}.${i}`
+    const dst = `${logFile}.${i + 1}`
+    if (await Bun.file(src).exists()) {
+      await rename(src, dst)
+    }
+  }
+
+  // Remove any file beyond keepCount (shouldn't normally exist, but be safe)
+  const overflow = `${logFile}.${keepCount + 1}`
+  if (await Bun.file(overflow).exists()) {
+    await Bun.file(overflow).delete?.()
+  }
+
+  // Rotate active log → app.log.1
+  await rename(logFile, `${logFile}.1`)
+
+  // Create fresh empty log file
+  await Bun.write(logFile, "")
+}
diff --git a/01-core-daemon/tests/logger/index.test.ts b/01-core-daemon/tests/logger/index.test.ts
new file mode 100644
index 0000000..7a3a9b2
--- /dev/null
+++ b/01-core-daemon/tests/logger/index.test.ts
@@ -0,0 +1,126 @@
+import { describe, it, expect, beforeEach, afterEach, spyOn } from "bun:test"
+import { createLogger } from "../../src/logger/index"
+
+describe("createLogger — JSON mode", () => {
+  let writeSpy: ReturnType<typeof spyOn>
+
+  beforeEach(() => {
+    writeSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+  })
+
+  afterEach(() => {
+    writeSpy.mockRestore()
+  })
+
+  it("each log call writes one valid JSON line with level, ts, msg fields", () => {
+    const logger = createLogger("info", "json")
+    logger.info("hello world")
+    expect(writeSpy).toHaveBeenCalledTimes(1)
+    const line = (writeSpy.mock.calls[0][0] as string).trim()
+    const parsed = JSON.parse(line)
+    expect(parsed).toHaveProperty("level", "info")
+    expect(parsed).toHaveProperty("ts")
+    expect(parsed).toHaveProperty("msg", "hello world")
+  })
+
+  it("meta object fields are merged into root of log line", () => {
+    const logger = createLogger("info", "json")
+    logger.info("hello", { reqId: "42" })
+    const line = (writeSpy.mock.calls[0][0] as string).trim()
+    const parsed = JSON.parse(line)
+    expect(parsed).toHaveProperty("reqId", "42")
+    expect(parsed).toHaveProperty("msg", "hello")
+  })
+})
+
+describe("createLogger — TTY mode ANSI suppression", () => {
+  let writeSpy: ReturnType<typeof spyOn>
+  const originalEnv = { ...process.env }
+
+  beforeEach(() => {
+    writeSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+  })
+
+  afterEach(() => {
+    writeSpy.mockRestore()
+    // restore env
+    for (const key of ["NO_COLOR", "CLICOLOR", "TERM"]) {
+      if (originalEnv[key] === undefined) {
+        delete process.env[key]
+      } else {
+        process.env[key] = originalEnv[key]
+      }
+    }
+  })
+
+  function hasAnsi(output: string): boolean {
+    return /\x1b\[/.test(output)
+  }
+
+  it("NO_COLOR set → output contains no ANSI escape codes", () => {
+    process.env.NO_COLOR = ""
+    const logger = createLogger("info", "tty")
+    logger.info("test")
+    const out = writeSpy.mock.calls[0][0] as string
+    expect(hasAnsi(out)).toBe(false)
+  })
+
+  it("CLICOLOR=0 → no ANSI codes", () => {
+    delete process.env.NO_COLOR
+    process.env.CLICOLOR = "0"
+    const logger = createLogger("info", "tty")
+    logger.info("test")
+    const out = writeSpy.mock.calls[0][0] as string
+    expect(hasAnsi(out)).toBe(false)
+  })
+
+  it("TERM=dumb → no ANSI codes", () => {
+    delete process.env.NO_COLOR
+    delete process.env.CLICOLOR
+    process.env.TERM = "dumb"
+    const logger = createLogger("info", "tty")
+    logger.info("test")
+    const out = writeSpy.mock.calls[0][0] as string
+    expect(hasAnsi(out)).toBe(false)
+  })
+
+  it("process.stdout.isTTY falsy → no ANSI codes", () => {
+    delete process.env.NO_COLOR
+    delete process.env.CLICOLOR
+    delete process.env.TERM
+    const origIsTTY = process.stdout.isTTY
+    Object.defineProperty(process.stdout, "isTTY", { value: false, configurable: true })
+    const logger = createLogger("info", "tty")
+    logger.info("test")
+    Object.defineProperty(process.stdout, "isTTY", { value: origIsTTY, configurable: true })
+    const out = writeSpy.mock.calls[0][0] as string
+    expect(hasAnsi(out)).toBe(false)
+  })
+})
+
+describe("createLogger — level gating", () => {
+  let writeSpy: ReturnType<typeof spyOn>
+
+  beforeEach(() => {
+    writeSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+  })
+
+  afterEach(() => {
+    writeSpy.mockRestore()
+  })
+
+  it("debug messages suppressed when level = 'info'", () => {
+    const logger = createLogger("info", "json")
+    logger.debug("secret")
+    expect(writeSpy).not.toHaveBeenCalled()
+  })
+
+  it("debug messages emitted when level = 'debug'", () => {
+    const logger = createLogger("debug", "json")
+    logger.debug("visible")
+    expect(writeSpy).toHaveBeenCalledTimes(1)
+    const line = (writeSpy.mock.calls[0][0] as string).trim()
+    const parsed = JSON.parse(line)
+    expect(parsed.msg).toBe("visible")
+  })
+})
diff --git a/01-core-daemon/tests/logger/rotate.test.ts b/01-core-daemon/tests/logger/rotate.test.ts
new file mode 100644
index 0000000..bd1800c
--- /dev/null
+++ b/01-core-daemon/tests/logger/rotate.test.ts
@@ -0,0 +1,89 @@
+import { describe, it, expect, beforeEach, afterEach } from "bun:test"
+import { rotateIfNeeded } from "../../src/logger/rotate"
+import { writeFile, readFile, stat, rm, rename } from "fs/promises"
+import { join } from "path"
+import { tmpdir } from "os"
+import { mkdtemp } from "fs/promises"
+
+let tmpDir: string
+let logFile: string
+
+beforeEach(async () => {
+  tmpDir = await mkdtemp(join(tmpdir(), "ja-test-"))
+  logFile = join(tmpDir, "app.log")
+})
+
+afterEach(async () => {
+  await rm(tmpDir, { recursive: true, force: true })
+})
+
+describe("rotateIfNeeded", () => {
+  it("file size below maxBytes → no rotation, original file unchanged", async () => {
+    await writeFile(logFile, "small content")
+    await rotateIfNeeded(logFile, 1024 * 1024)
+    const rotated = join(tmpDir, "app.log.1")
+    let exists = false
+    try {
+      await stat(rotated)
+      exists = true
+    } catch {}
+    expect(exists).toBe(false)
+    const content = await readFile(logFile, "utf8")
+    expect(content).toBe("small content")
+  })
+
+  it("file size at/above maxBytes → app.log.1 created with original content", async () => {
+    const content = "x".repeat(100)
+    await writeFile(logFile, content)
+    await rotateIfNeeded(logFile, 50)
+    const rotated = join(tmpDir, "app.log.1")
+    const rotatedContent = await readFile(rotated, "utf8")
+    expect(rotatedContent).toBe(content)
+    const fresh = await readFile(logFile, "utf8")
+    expect(fresh).toBe("")
+  })
+
+  it("second rotation → app.log.1 becomes app.log.2, new app.log.1 has previous app.log content", async () => {
+    // First rotation
+    const first = "first content"
+    await writeFile(logFile, first)
+    await rotateIfNeeded(logFile, 1)
+
+    // Second rotation
+    const second = "second content"
+    await writeFile(logFile, second)
+    await rotateIfNeeded(logFile, 1)
+
+    const log1 = await readFile(join(tmpDir, "app.log.1"), "utf8")
+    const log2 = await readFile(join(tmpDir, "app.log.2"), "utf8")
+    expect(log1).toBe(second)
+    expect(log2).toBe(first)
+  })
+
+  it("when keepCount files exist → oldest file deleted, others shifted", async () => {
+    const keepCount = 3
+    // Pre-create app.log.1 through app.log.keepCount
+    for (let i = 1; i <= keepCount; i++) {
+      await writeFile(join(tmpDir, `app.log.${i}`), `rotated-${i}`)
+    }
+    await writeFile(logFile, "x".repeat(100))
+    await rotateIfNeeded(logFile, 50, keepCount)
+
+    // app.log.<keepCount+1> must NOT exist
+    let tooManyExist = false
+    try {
+      await stat(join(tmpDir, `app.log.${keepCount + 1}`))
+      tooManyExist = true
+    } catch {}
+    expect(tooManyExist).toBe(false)
+
+    // app.log.1 must exist
+    const log1 = await stat(join(tmpDir, "app.log.1"))
+    expect(log1.isFile()).toBe(true)
+  })
+
+  it("non-existent log file → no-op, no error thrown", async () => {
+    const missing = join(tmpDir, "missing.log")
+    await expect(rotateIfNeeded(missing)).resolves.toBeUndefined()
+  })
+})
