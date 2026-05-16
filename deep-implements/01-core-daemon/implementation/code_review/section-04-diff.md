diff --git a/01-core-daemon/src/daemon/launchd.ts b/01-core-daemon/src/daemon/launchd.ts
new file mode 100644
index 0000000..3b26b3c
--- /dev/null
+++ b/01-core-daemon/src/daemon/launchd.ts
@@ -0,0 +1,100 @@
+import { PATHS } from "../shared/paths"
+import { LaunchctlError, launchctlHint } from "../shared/errors"
+
+export interface AgentStatus {
+  running: boolean
+  pid?: number
+  exitCode?: number
+}
+
+export function generatePlist(binaryPath: string): string {
+  return `<?xml version="1.0" encoding="UTF-8"?>
+<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
+<plist version="1.0">
+<dict>
+    <key>Label</key>
+    <string>net.jira-assistant</string>
+    <key>ProgramArguments</key>
+    <array>
+        <string>${binaryPath}</string>
+        <string>daemon</string>
+    </array>
+    <key>KeepAlive</key>
+    <dict>
+        <key>SuccessfulExit</key>
+        <false/>
+        <key>Crashed</key>
+        <true/>
+    </dict>
+    <key>ThrottleInterval</key>
+    <integer>10</integer>
+    <key>RunAtLoad</key>
+    <false/>
+</dict>
+</plist>
+`
+}
+
+export async function writePlist(binaryPath: string, filePath: string = PATHS.plistFile): Promise<void> {
+  await Bun.write(filePath, generatePlist(binaryPath))
+}
+
+async function runLaunchctl(args: string[]): Promise<{ exitCode: number; stdout: string; stderr: string }> {
+  const proc = Bun.spawn(["launchctl", ...args], {
+    stdout: "pipe",
+    stderr: "pipe",
+  })
+  const exitCode = await proc.exited
+  const stdout = await new Response(proc.stdout).text()
+  const stderr = await new Response(proc.stderr).text()
+  return { exitCode, stdout, stderr }
+}
+
+export async function loadAgent(): Promise<void> {
+  const { exitCode, stderr } = await runLaunchctl(["load", "-w", PATHS.plistFile])
+  if (exitCode !== 0) {
+    throw new LaunchctlError(stderr, launchctlHint(stderr))
+  }
+}
+
+export async function unloadAgent(): Promise<void> {
+  const { exitCode, stderr } = await runLaunchctl(["unload", "-w", PATHS.plistFile])
+  if (exitCode !== 0) {
+    throw new LaunchctlError(stderr, launchctlHint(stderr))
+  }
+}
+
+export async function agentStatus(): Promise<AgentStatus> {
+  const uid = process.getuid?.() ?? 0
+  const { exitCode: printExit, stdout: printOut } = await runLaunchctl([
+    "print",
+    `gui/${uid}/net.jira-assistant`,
+  ])
+
+  if (printExit === 0) {
+    const pidMatch = printOut.match(/pid\s*=\s*(\d+)/)
+    if (pidMatch) {
+      return { running: true, pid: parseInt(pidMatch[1], 10) }
+    }
+    return { running: false }
+  }
+
+  const { exitCode: listExit, stdout: listOut } = await runLaunchctl([
+    "list",
+    "net.jira-assistant",
+  ])
+
+  if (listExit !== 0) {
+    return { running: false }
+  }
+
+  const line = listOut.trim().split("\n").find(l => l.includes("net.jira-assistant"))
+  if (!line) return { running: false }
+
+  const [pidStr, exitCodeStr] = line.split("\t")
+  if (pidStr && pidStr !== "-") {
+    return { running: true, pid: parseInt(pidStr, 10) }
+  }
+  const exitCode = exitCodeStr ? parseInt(exitCodeStr, 10) : undefined
+  return { running: false, exitCode: isNaN(exitCode ?? NaN) ? undefined : exitCode }
+}
diff --git a/01-core-daemon/src/daemon/pid.ts b/01-core-daemon/src/daemon/pid.ts
new file mode 100644
index 0000000..8653b24
--- /dev/null
+++ b/01-core-daemon/src/daemon/pid.ts
@@ -0,0 +1,36 @@
+import { join, dirname } from "path"
+import { rename, unlink } from "fs/promises"
+import { PATHS } from "../shared/paths"
+
+export async function writePid(pid: number, filePath: string = PATHS.pidFile): Promise<void> {
+  const dir = dirname(filePath)
+  const tmp = join(dir, `.pid-tmp-${process.pid}-${Date.now()}`)
+  await Bun.write(tmp, `${pid}\n`)
+  await rename(tmp, filePath)
+}
+
+export async function readPid(filePath: string = PATHS.pidFile): Promise<number | null> {
+  try {
+    const text = await Bun.file(filePath).text()
+    const n = parseInt(text.trim(), 10)
+    return isNaN(n) ? null : n
+  } catch {
+    return null
+  }
+}
+
+export async function removePid(filePath: string = PATHS.pidFile): Promise<void> {
+  try {
+    await unlink(filePath)
+  } catch {}
+}
+
+export async function isProcessRunning(pid: number): Promise<boolean> {
+  try {
+    process.kill(pid, 0)
+    return true
+  } catch (err: any) {
+    if (err?.code === "ESRCH") return false
+    throw err
+  }
+}
diff --git a/01-core-daemon/src/daemon/restart-tracker.ts b/01-core-daemon/src/daemon/restart-tracker.ts
new file mode 100644
index 0000000..9d5f9c1
--- /dev/null
+++ b/01-core-daemon/src/daemon/restart-tracker.ts
@@ -0,0 +1,45 @@
+import { join, dirname } from "path"
+import { rename } from "fs/promises"
+
+export class RestartTracker {
+  private readonly filePath: string
+  private readonly maxRestarts: number
+  private readonly windowMs: number
+
+  constructor(filePath: string, maxRestarts: number = 10, windowMs: number = 60_000) {
+    this.filePath = filePath
+    this.maxRestarts = maxRestarts
+    this.windowMs = windowMs
+  }
+
+  async recordRestart(): Promise<boolean> {
+    const timestamps = await this.read()
+    const now = Date.now()
+    timestamps.push(now)
+    const pruned = timestamps.filter(t => now - t <= this.windowMs)
+    await this.write(pruned)
+    return pruned.length > this.maxRestarts
+  }
+
+  async reset(): Promise<void> {
+    await this.write([])
+  }
+
+  private async read(): Promise<number[]> {
+    try {
+      const text = await Bun.file(this.filePath).text()
+      const parsed = JSON.parse(text)
+      if (Array.isArray(parsed)) return parsed as number[]
+      return []
+    } catch {
+      return []
+    }
+  }
+
+  private async write(timestamps: number[]): Promise<void> {
+    const dir = dirname(this.filePath)
+    const tmp = join(dir, `.restarts-tmp-${process.pid}-${Date.now()}`)
+    await Bun.write(tmp, JSON.stringify(timestamps))
+    await rename(tmp, this.filePath)
+  }
+}
diff --git a/01-core-daemon/tests/daemon/launchd.test.ts b/01-core-daemon/tests/daemon/launchd.test.ts
new file mode 100644
index 0000000..995e764
--- /dev/null
+++ b/01-core-daemon/tests/daemon/launchd.test.ts
@@ -0,0 +1,168 @@
+import { describe, it, expect, spyOn, beforeEach, afterEach, mock } from "bun:test"
+import { join } from "path"
+import { tmpdir } from "os"
+import { unlinkSync } from "fs"
+import { generatePlist, writePlist, loadAgent, unloadAgent, agentStatus } from "../../src/daemon/launchd"
+import { PATHS } from "../../src/shared/paths"
+import { LaunchctlError } from "../../src/shared/errors"
+
+const BINARY = "/usr/local/bin/jira-assistant"
+
+describe("generatePlist", () => {
+  it("contains the correct Label key", () => {
+    const xml = generatePlist(BINARY)
+    expect(xml).toContain("<key>Label</key>")
+    expect(xml).toContain("<string>net.jira-assistant</string>")
+  })
+
+  it("contains KeepAlive as a dictionary with SuccessfulExit=false and Crashed=true (not simple boolean true)", () => {
+    const xml = generatePlist(BINARY)
+    expect(xml).toContain("<key>KeepAlive</key>")
+    expect(xml).toContain("<key>SuccessfulExit</key>")
+    expect(xml).toContain("<false/>")
+    expect(xml).toContain("<key>Crashed</key>")
+    expect(xml).toContain("<true/>")
+    // Must NOT be the simple form
+    expect(xml).not.toMatch(/<key>KeepAlive<\/key>\s*<true\/>/)
+  })
+
+  it("contains ThrottleInterval of 10", () => {
+    const xml = generatePlist(BINARY)
+    expect(xml).toContain("<key>ThrottleInterval</key>")
+    expect(xml).toContain("<integer>10</integer>")
+  })
+
+  it("contains ProgramArguments with binary path and 'daemon' subcommand", () => {
+    const xml = generatePlist(BINARY)
+    expect(xml).toContain("<key>ProgramArguments</key>")
+    expect(xml).toContain(`<string>${BINARY}</string>`)
+    expect(xml).toContain("<string>daemon</string>")
+  })
+
+  it("does NOT contain StandardOutPath key", () => {
+    const xml = generatePlist(BINARY)
+    expect(xml).not.toContain("StandardOutPath")
+  })
+
+  it("does NOT contain StandardErrorPath key", () => {
+    const xml = generatePlist(BINARY)
+    expect(xml).not.toContain("StandardErrorPath")
+  })
+})
+
+describe("writePlist", () => {
+  let testPlistPath: string
+
+  beforeEach(() => {
+    testPlistPath = join(tmpdir(), `test-${Date.now()}.plist`)
+  })
+
+  afterEach(() => {
+    try { unlinkSync(testPlistPath) } catch {}
+  })
+
+  it("creates the plist file at PATHS.plistFile", async () => {
+    await writePlist(BINARY, testPlistPath)
+    const content = await Bun.file(testPlistPath).text()
+    expect(content).toContain("net.jira-assistant")
+    expect(content).toContain(BINARY)
+  })
+})
+
+// Helper: create a mock Bun.spawn result
+function makeSpawnResult(exitCode: number, stdout = "", stderr = "") {
+  return {
+    exited: Promise.resolve(exitCode),
+    stdout: new ReadableStream({
+      start(controller) {
+        controller.enqueue(new TextEncoder().encode(stdout))
+        controller.close()
+      }
+    }),
+    stderr: new ReadableStream({
+      start(controller) {
+        controller.enqueue(new TextEncoder().encode(stderr))
+        controller.close()
+      }
+    }),
+  }
+}
+
+describe("loadAgent", () => {
+  it("calls Bun.spawn with ['launchctl', 'load', '-w', PATHS.plistFile]", async () => {
+    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(makeSpawnResult(0) as any)
+    await loadAgent()
+    expect(spawnSpy).toHaveBeenCalledWith(
+      expect.arrayContaining(["launchctl", "load", "-w", PATHS.plistFile]),
+      expect.anything()
+    )
+    spawnSpy.mockRestore()
+  })
+
+  it("throws LaunchctlError containing raw stderr when launchctl exits non-zero", async () => {
+    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(
+      makeSpawnResult(1, "", "No such file or directory") as any
+    )
+    await expect(loadAgent()).rejects.toBeInstanceOf(LaunchctlError)
+    spawnSpy.mockRestore()
+  })
+})
+
+describe("unloadAgent", () => {
+  it("calls Bun.spawn with ['launchctl', 'unload', '-w', PATHS.plistFile]", async () => {
+    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(makeSpawnResult(0) as any)
+    await unloadAgent()
+    expect(spawnSpy).toHaveBeenCalledWith(
+      expect.arrayContaining(["launchctl", "unload", "-w", PATHS.plistFile]),
+      expect.anything()
+    )
+    spawnSpy.mockRestore()
+  })
+
+  it("throws LaunchctlError on non-zero exit", async () => {
+    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(
+      makeSpawnResult(1, "", "Permission denied") as any
+    )
+    await expect(unloadAgent()).rejects.toBeInstanceOf(LaunchctlError)
+    spawnSpy.mockRestore()
+  })
+})
+
+describe("agentStatus", () => {
+  it("parses running process from launchctl print output (macOS 12+ format)", async () => {
+    const printOutput = `{
+      pid = 12345
+      state = running
+    }`
+    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(
+      makeSpawnResult(0, printOutput, "") as any
+    )
+    const status = await agentStatus()
+    expect(status.running).toBe(true)
+    expect(status.pid).toBe(12345)
+    spawnSpy.mockRestore()
+  })
+
+  it("falls back to launchctl list when print fails", async () => {
+    const listOutput = "12345\t0\tnet.jira-assistant\n"
+    let callCount = 0
+    const spawnSpy = spyOn(Bun, "spawn").mockImplementation((() => {
+      callCount++
+      if (callCount === 1) return makeSpawnResult(1, "", "Unknown service") as any
+      return makeSpawnResult(0, listOutput, "") as any
+    }) as any)
+    const status = await agentStatus()
+    expect(status.running).toBe(true)
+    expect(status.pid).toBe(12345)
+    spawnSpy.mockRestore()
+  })
+
+  it("returns { running: false } when agent is not loaded", async () => {
+    const spawnSpy = spyOn(Bun, "spawn").mockImplementation(
+      (() => makeSpawnResult(1, "", "Could not find service")) as any
+    )
+    const status = await agentStatus()
+    expect(status.running).toBe(false)
+    spawnSpy.mockRestore()
+  })
+})
diff --git a/01-core-daemon/tests/daemon/pid.test.ts b/01-core-daemon/tests/daemon/pid.test.ts
new file mode 100644
index 0000000..657cf01
--- /dev/null
+++ b/01-core-daemon/tests/daemon/pid.test.ts
@@ -0,0 +1,55 @@
+import { describe, it, expect, beforeEach, afterEach } from "bun:test"
+import { join } from "path"
+import { tmpdir } from "os"
+import { writePid, readPid, removePid, isProcessRunning } from "../../src/daemon/pid"
+
+let testPidFile: string
+
+beforeEach(() => {
+  testPidFile = join(tmpdir(), `test-${Date.now()}-${Math.random()}.pid`)
+})
+
+afterEach(async () => {
+  try { await Bun.file(testPidFile).exists() && Bun.file(testPidFile).text() } catch {}
+  try { require("fs").unlinkSync(testPidFile) } catch {}
+})
+
+describe("writePid / readPid", () => {
+  it("round-trips: writePid(1234) then readPid() returns 1234", async () => {
+    await writePid(1234, testPidFile)
+    const pid = await readPid(testPidFile)
+    expect(pid).toBe(1234)
+  })
+
+  it("uses atomic write (temp file + rename): file appears fully written", async () => {
+    await writePid(9999, testPidFile)
+    const content = await Bun.file(testPidFile).text()
+    expect(content.trim()).toBe("9999")
+  })
+})
+
+describe("readPid", () => {
+  it("returns null when file is missing", async () => {
+    const result = await readPid("/nonexistent/path/that/cannot/exist.pid")
+    expect(result).toBeNull()
+  })
+})
+
+describe("removePid", () => {
+  it("deletes the file; subsequent readPid() returns null", async () => {
+    await writePid(5678, testPidFile)
+    expect(await readPid(testPidFile)).toBe(5678)
+    await removePid(testPidFile)
+    expect(await readPid(testPidFile)).toBeNull()
+  })
+})
+
+describe("isProcessRunning", () => {
+  it("returns true for the current process PID", async () => {
+    expect(await isProcessRunning(process.pid)).toBe(true)
+  })
+
+  it("returns false for a non-existent PID like 99999999", async () => {
+    expect(await isProcessRunning(99999999)).toBe(false)
+  })
+})
diff --git a/01-core-daemon/tests/daemon/restart-tracker.test.ts b/01-core-daemon/tests/daemon/restart-tracker.test.ts
new file mode 100644
index 0000000..d68bd1e
--- /dev/null
+++ b/01-core-daemon/tests/daemon/restart-tracker.test.ts
@@ -0,0 +1,71 @@
+import { describe, it, expect, beforeEach, afterEach } from "bun:test"
+import { join } from "path"
+import { tmpdir } from "os"
+import { unlinkSync } from "fs"
+import { RestartTracker } from "../../src/daemon/restart-tracker"
+
+let testFile: string
+
+beforeEach(() => {
+  testFile = join(tmpdir(), `restarts-${Date.now()}-${Math.random()}.json`)
+})
+
+afterEach(() => {
+  try { unlinkSync(testFile) } catch {}
+})
+
+describe("RestartTracker", () => {
+  it("first recordRestart() returns false (under limit)", async () => {
+    const tracker = new RestartTracker(testFile, 3, 60_000)
+    expect(await tracker.recordRestart()).toBe(false)
+  })
+
+  it("reaching maxRestarts within windowMs returns true on final call", async () => {
+    const tracker = new RestartTracker(testFile, 3, 60_000)
+    expect(await tracker.recordRestart()).toBe(false)
+    expect(await tracker.recordRestart()).toBe(false)
+    expect(await tracker.recordRestart()).toBe(false)
+    expect(await tracker.recordRestart()).toBe(true)
+  })
+
+  it("timestamps outside windowMs are pruned; pruned count goes back under limit", async () => {
+    const tracker = new RestartTracker(testFile, 2, 1000)
+    await tracker.recordRestart()
+    await tracker.recordRestart()
+
+    // Write old timestamps directly to file to simulate aged entries
+    const old = Date.now() - 2000
+    await Bun.write(testFile, JSON.stringify([old, old]))
+
+    // Now recordRestart should prune the old ones and return false (1 entry, under limit of 2)
+    expect(await tracker.recordRestart()).toBe(false)
+  })
+
+  it("recreating tracker pointing to same file reads persisted timestamps", async () => {
+    const tracker1 = new RestartTracker(testFile, 3, 60_000)
+    await tracker1.recordRestart()
+    await tracker1.recordRestart()
+
+    const tracker2 = new RestartTracker(testFile, 3, 60_000)
+    // Already has 2, adding 1 more = 3, still not over limit (> 3 means over)
+    expect(await tracker2.recordRestart()).toBe(false)
+    // 4th push = 4 > 3, returns true
+    expect(await tracker2.recordRestart()).toBe(true)
+  })
+
+  it("starts with empty array when state file is missing", async () => {
+    const missingFile = join(tmpdir(), `restarts-missing-${Date.now()}.json`)
+    const tracker = new RestartTracker(missingFile, 5, 60_000)
+    expect(await tracker.recordRestart()).toBe(false)
+  })
+
+  it("reset() clears the persisted file", async () => {
+    const tracker = new RestartTracker(testFile, 2, 60_000)
+    await tracker.recordRestart()
+    await tracker.recordRestart()
+    await tracker.reset()
+
+    const tracker2 = new RestartTracker(testFile, 2, 60_000)
+    expect(await tracker2.recordRestart()).toBe(false)
+  })
+})
