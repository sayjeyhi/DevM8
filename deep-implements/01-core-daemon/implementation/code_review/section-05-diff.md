diff --git a/01-core-daemon/src/commands/config.ts b/01-core-daemon/src/commands/config.ts
new file mode 100644
index 0000000..de1ec8d
--- /dev/null
+++ b/01-core-daemon/src/commands/config.ts
@@ -0,0 +1,24 @@
+import { defineCommand } from "citty"
+import { loadConfig, writeConfig } from "../config/loader"
+import { runWizard } from "../config/wizard"
+import { PATHS } from "../shared/paths"
+
+export async function configCommand(): Promise<void> {
+  let existing
+  try {
+    existing = await loadConfig()
+  } catch {
+    existing = undefined
+  }
+
+  const result = await runWizard(existing)
+  await writeConfig(result)
+  process.stdout.write(`Config written to ${PATHS.configFile}\n`)
+}
+
+export default defineCommand({
+  meta: { name: "config", description: "Configure jira-assistant" },
+  async run() {
+    await configCommand()
+  },
+})
diff --git a/01-core-daemon/src/commands/daemon.ts b/01-core-daemon/src/commands/daemon.ts
new file mode 100644
index 0000000..9297e69
--- /dev/null
+++ b/01-core-daemon/src/commands/daemon.ts
@@ -0,0 +1,62 @@
+import { defineCommand } from "citty"
+import { createLogger } from "../logger/index"
+import { rotateIfNeeded } from "../logger/rotate"
+import { loadConfig } from "../config/loader"
+import { writePid, removePid } from "../daemon/pid"
+import { RestartTracker } from "../daemon/restart-tracker"
+import { PATHS } from "../shared/paths"
+import { FriendlyError } from "../shared/errors"
+import { startPolling } from "../../../02-integration-clients/src/index"
+
+export async function daemonCommand(): Promise<void> {
+  let config
+  try {
+    config = await loadConfig()
+  } catch (err) {
+    if (err instanceof FriendlyError) {
+      process.stderr.write(`${err.message}\n`)
+      process.exit(1)
+    }
+    throw err
+  }
+
+  const logger = createLogger(config.app.log_level)
+  const restartTracker = new RestartTracker(PATHS.restartsFile, 10, 60_000)
+
+  await writePid(process.pid)
+
+  const shutdownController = new AbortController()
+  let pollingPromise: Promise<void> | undefined
+
+  process.on("SIGTERM", async () => {
+    shutdownController.abort()
+    if (pollingPromise) {
+      try { await pollingPromise } catch {}
+    }
+    await removePid()
+    logger.info("shutdown complete")
+    process.exit(0)
+  })
+
+  await rotateIfNeeded(PATHS.logFile)
+  setInterval(() => rotateIfNeeded(PATHS.logFile), 60 * 60 * 1000)
+
+  try {
+    pollingPromise = startPolling(shutdownController.signal)
+    await pollingPromise
+  } catch (err) {
+    const limitExceeded = await restartTracker.recordRestart()
+    if (limitExceeded) {
+      logger.warn("restart limit exceeded, shutting down")
+      process.exit(0)
+    }
+    throw err
+  }
+}
+
+export default defineCommand({
+  meta: { name: "daemon", description: "Run the daemon process (used by launchd)" },
+  async run() {
+    await daemonCommand()
+  },
+})
diff --git a/01-core-daemon/src/commands/start.ts b/01-core-daemon/src/commands/start.ts
new file mode 100644
index 0000000..b8d49f1
--- /dev/null
+++ b/01-core-daemon/src/commands/start.ts
@@ -0,0 +1,78 @@
+import { realpathSync } from "node:fs"
+import { mkdir, access, constants } from "node:fs/promises"
+import { defineCommand } from "citty"
+import { PATHS } from "../shared/paths"
+import { FriendlyError } from "../shared/errors"
+import { loadConfig, configExists, writeConfig } from "../config/loader"
+import { runWizard } from "../config/wizard"
+import { agentStatus, writePlist, loadAgent } from "../daemon/launchd"
+import { stopCommand } from "./stop"
+
+async function preflight(): Promise<void> {
+  if (process.platform !== "darwin") {
+    throw new FriendlyError(
+      "jira-assistant requires macOS",
+      "This tool uses launchd, which is only available on macOS."
+    )
+  }
+
+  await mkdir(PATHS.launchAgentsDir, { recursive: true })
+
+  let config
+  try {
+    config = await loadConfig()
+  } catch {
+    return
+  }
+
+  try {
+    await access(config.claude.binary_path, constants.X_OK)
+  } catch {
+    throw new FriendlyError(
+      `Claude binary not executable at ${config.claude.binary_path}`,
+      "Run `which claude` to find the correct path, then update with `jira-assistant config`."
+    )
+  }
+}
+
+export async function startCommand(): Promise<void> {
+  await preflight()
+
+  if (!(await configExists())) {
+    const result = await runWizard()
+    await writeConfig(result)
+  }
+
+  const status = await agentStatus()
+  if (status.running) {
+    process.stdout.write("Daemon already running; stopping first...\n")
+    await stopCommand()
+  }
+
+  await writePlist(realpathSync(Bun.argv[0]))
+  await loadAgent()
+
+  const deadline = Date.now() + 5000
+  while (Date.now() < deadline) {
+    const s = await agentStatus()
+    if (s.running) {
+      process.stdout.write(`jira-assistant started (PID ${s.pid})\n`)
+      return
+    }
+    await Bun.sleep(200)
+  }
+
+  const finalStatus = await agentStatus()
+  process.stderr.write(
+    `jira-assistant failed to start. Last exit code: ${finalStatus.exitCode ?? "unknown"}\n` +
+    `Hint: check \`jira-assistant status\` or ${PATHS.logFile}\n`
+  )
+  process.exit(1)
+}
+
+export default defineCommand({
+  meta: { name: "start", description: "Start the jira-assistant daemon" },
+  async run() {
+    await startCommand()
+  },
+})
diff --git a/01-core-daemon/src/commands/status.ts b/01-core-daemon/src/commands/status.ts
new file mode 100644
index 0000000..79a390b
--- /dev/null
+++ b/01-core-daemon/src/commands/status.ts
@@ -0,0 +1,54 @@
+import { homedir } from "os"
+import { defineCommand } from "citty"
+import { agentStatus } from "../daemon/launchd"
+import { readPid } from "../daemon/pid"
+import { loadConfig } from "../config/loader"
+import { PATHS } from "../shared/paths"
+
+function fmtPath(p: string): string {
+  return p.replace(homedir(), "~")
+}
+
+function fmtUptime(ms: number): string {
+  const totalSecs = Math.floor(ms / 1000)
+  const h = Math.floor(totalSecs / 3600)
+  const m = Math.floor((totalSecs % 3600) / 60)
+  return h > 0 ? `${h}h ${m}m` : `${m}m`
+}
+
+export async function statusCommand(): Promise<void> {
+  const [status, pid] = await Promise.all([agentStatus(), readPid()])
+
+  let config = null
+  try {
+    config = await loadConfig()
+  } catch {}
+
+  let uptime: string | undefined
+  if (status.running) {
+    try {
+      const stat = await Bun.file(PATHS.pidFile).stat()
+      if (stat) uptime = fmtUptime(Date.now() - stat.mtimeMs)
+    } catch {}
+  }
+
+  const lines: string[] = ["jira-assistant status"]
+  lines.push(`  State:       ${status.running ? "running" : "stopped"}`)
+  if (status.running && pid !== null) lines.push(`  PID:         ${pid}`)
+  if (status.running && uptime) lines.push(`  Uptime:      ${uptime}`)
+  lines.push(`  Config:      ${fmtPath(PATHS.configFile)}`)
+  if (config) {
+    lines.push(`  Jira URL:    ${config.jira.base_url}`)
+    lines.push(`  Project:     ${config.jira.project_key}`)
+  }
+  lines.push(`  Log:         ${fmtPath(PATHS.logFile)}`)
+
+  process.stdout.write(lines.join("\n") + "\n")
+}
+
+export default defineCommand({
+  meta: { name: "status", description: "Show jira-assistant daemon status" },
+  async run() {
+    await statusCommand()
+  },
+})
diff --git a/01-core-daemon/src/commands/stop.ts b/01-core-daemon/src/commands/stop.ts
new file mode 100644
index 0000000..76f3f47
--- /dev/null
+++ b/01-core-daemon/src/commands/stop.ts
@@ -0,0 +1,27 @@
+import { defineCommand } from "citty"
+import { unloadAgent } from "../daemon/launchd"
+import { removePid } from "../daemon/pid"
+import { FriendlyError, LaunchctlError } from "../shared/errors"
+
+export async function stopCommand(): Promise<void> {
+  try {
+    await unloadAgent()
+  } catch (err) {
+    if (err instanceof LaunchctlError) {
+      throw new FriendlyError(
+        `Failed to stop daemon: ${err.hint ?? err.message}`,
+        err.hint
+      )
+    }
+    throw err
+  }
+  await removePid()
+  process.stdout.write("jira-assistant stopped\n")
+}
+
+export default defineCommand({
+  meta: { name: "stop", description: "Stop the jira-assistant daemon" },
+  async run() {
+    await stopCommand()
+  },
+})
diff --git a/01-core-daemon/src/index.ts b/01-core-daemon/src/index.ts
new file mode 100644
index 0000000..12608fd
--- /dev/null
+++ b/01-core-daemon/src/index.ts
@@ -0,0 +1,28 @@
+import { defineCommand, runMain } from "citty"
+import { FriendlyError } from "./shared/errors"
+
+declare const __VERSION__: string
+
+const main = defineCommand({
+  meta: {
+    name: "jira-assistant",
+    version: __VERSION__,
+    description: "Manage your Jira assistant Telegram bot daemon",
+  },
+  subCommands: {
+    start:  () => import("./commands/start").then(m => m.default),
+    stop:   () => import("./commands/stop").then(m => m.default),
+    status: () => import("./commands/status").then(m => m.default),
+    config: () => import("./commands/config").then(m => m.default),
+    daemon: () => import("./commands/daemon").then(m => m.default),
+  },
+})
+
+runMain(main).catch(err => {
+  if (err instanceof FriendlyError) {
+    process.stderr.write(`Error: ${err.message}\n`)
+    if (err.hint) process.stderr.write(`Hint: ${err.hint}\n`)
+    process.exit(1)
+  }
+  throw err
+})
diff --git a/01-core-daemon/tests/commands/config.test.ts b/01-core-daemon/tests/commands/config.test.ts
new file mode 100644
index 0000000..85c293f
--- /dev/null
+++ b/01-core-daemon/tests/commands/config.test.ts
@@ -0,0 +1,66 @@
+import { describe, it, expect, mock, spyOn } from "bun:test"
+
+const wizardResultMock = {
+  telegram: { bot_token: "123456:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef" },
+  jira: { base_url: "https://test.atlassian.net", api_token: "token", email: "user@test.com", project_key: "TEST" },
+  claude: { binary_path: "/usr/bin/claude" },
+  app: { log_level: "info" as const },
+}
+
+const existingConfig = {
+  ...wizardResultMock,
+  jira: { ...wizardResultMock.jira, project_key: "EXISTING" },
+}
+
+const runWizardMock = mock((_existing?: typeof existingConfig) => Promise.resolve(wizardResultMock))
+const loadConfigMock = mock(() => Promise.resolve(existingConfig))
+const writeConfigMock = mock(() => Promise.resolve())
+const configExistsMock = mock(() => Promise.resolve(true))
+
+mock.module("../../src/config/wizard", () => ({ runWizard: runWizardMock }))
+mock.module("../../src/config/loader", () => ({
+  loadConfig: loadConfigMock,
+  configExists: configExistsMock,
+  writeConfig: writeConfigMock,
+}))
+
+import { configCommand } from "../../src/commands/config"
+
+describe("configCommand()", () => {
+  it("runs wizard with no existing argument when config does not exist", async () => {
+    loadConfigMock.mockImplementation(() => Promise.reject(new Error("ENOENT")))
+    runWizardMock.mockClear()
+
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+    await configCommand()
+
+    expect(runWizardMock).toHaveBeenCalledWith(undefined)
+
+    runWizardMock.mockImplementation((_existing?: typeof existingConfig) => Promise.resolve(wizardResultMock))
+    loadConfigMock.mockImplementation(() => Promise.resolve(existingConfig))
+    stdoutSpy.mockRestore()
+  })
+
+  it("runs wizard pre-filled with existing values when config exists (mocks loadConfig)", async () => {
+    loadConfigMock.mockImplementation(() => Promise.resolve(existingConfig))
+    runWizardMock.mockClear()
+
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+    await configCommand()
+
+    expect(runWizardMock).toHaveBeenCalledWith(existingConfig)
+
+    stdoutSpy.mockRestore()
+  })
+
+  it("calls writeConfig with the wizard result on completion", async () => {
+    writeConfigMock.mockClear()
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+
+    await configCommand()
+
+    expect(writeConfigMock).toHaveBeenCalledWith(wizardResultMock)
+
+    stdoutSpy.mockRestore()
+  })
+})
diff --git a/01-core-daemon/tests/commands/daemon.test.ts b/01-core-daemon/tests/commands/daemon.test.ts
new file mode 100644
index 0000000..c7dd2e8
--- /dev/null
+++ b/01-core-daemon/tests/commands/daemon.test.ts
@@ -0,0 +1,122 @@
+import { describe, it, expect, mock, spyOn, beforeEach } from "bun:test"
+
+const startPollingMock = mock((_signal: AbortSignal): Promise<void> => Promise.resolve())
+const rotateIfNeededMock = mock((_path: string): Promise<void> => Promise.resolve())
+const writePidMock = mock((_pid: number): Promise<void> => Promise.resolve())
+const removePidMock = mock((): Promise<void> => Promise.resolve())
+
+mock.module("../../../02-integration-clients/src/index", () => ({
+  startPolling: startPollingMock,
+}))
+mock.module("../../src/logger/rotate", () => ({
+  rotateIfNeeded: rotateIfNeededMock,
+}))
+mock.module("../../src/daemon/pid", () => ({
+  writePid: writePidMock,
+  removePid: removePidMock,
+  readPid: mock(() => Promise.resolve(null)),
+  isProcessRunning: mock(() => Promise.resolve(false)),
+}))
+mock.module("../../src/config/loader", () => ({
+  loadConfig: mock(() => Promise.resolve({
+    app: { log_level: "info" as const },
+    telegram: { bot_token: "123:abc" },
+    jira: { base_url: "https://test.atlassian.net", api_token: "tok", email: "e@e.com", project_key: "T" },
+    claude: { binary_path: "/usr/bin/claude" },
+  })),
+  configExists: mock(() => Promise.resolve(true)),
+  writeConfig: mock(() => Promise.resolve()),
+}))
+
+import { daemonCommand } from "../../src/commands/daemon"
+import { RestartTracker } from "../../src/daemon/restart-tracker"
+
+describe("daemonCommand()", () => {
+  beforeEach(() => {
+    startPollingMock.mockImplementation((_signal: AbortSignal): Promise<void> => Promise.resolve())
+    rotateIfNeededMock.mockClear()
+    writePidMock.mockClear()
+    removePidMock.mockClear()
+    process.removeAllListeners("SIGTERM")
+  })
+
+  it("calls shutdownController.abort() on SIGTERM (mocks polling loop entry)", async () => {
+    let abortedBeforeResolve = false
+
+    startPollingMock.mockImplementation((signal: AbortSignal): Promise<void> =>
+      new Promise(resolve => {
+        signal.addEventListener("abort", () => {
+          abortedBeforeResolve = true
+          resolve()
+        }, { once: true })
+      })
+    )
+
+    const exitSpy = spyOn(process, "exit").mockImplementation((_code?: number) => undefined as never)
+
+    const runPromise = daemonCommand()
+
+    // Tick to allow SIGTERM handler to register
+    await new Promise(r => setTimeout(r, 10))
+
+    process.emit("SIGTERM" as any)
+
+    await runPromise
+
+    // Allow async SIGTERM handler to complete
+    await new Promise(r => setTimeout(r, 20))
+
+    expect(abortedBeforeResolve).toBe(true)
+    expect(removePidMock).toHaveBeenCalled()
+    expect(exitSpy).toHaveBeenCalledWith(0)
+
+    exitSpy.mockRestore()
+  })
+
+  it("calls restartTracker.recordRestart() and re-throws when unhandled crash is under limit", async () => {
+    const crashError = new Error("telegram API crash")
+    startPollingMock.mockImplementation((): Promise<void> => Promise.reject(crashError))
+
+    const recordRestartSpy = spyOn(RestartTracker.prototype, "recordRestart").mockResolvedValue(false)
+
+    await expect(daemonCommand()).rejects.toThrow("telegram API crash")
+
+    expect(recordRestartSpy).toHaveBeenCalled()
+
+    recordRestartSpy.mockRestore()
+  })
+
+  it("calls process.exit(0) when restartTracker.recordRestart() returns true (limit exceeded)", async () => {
+    startPollingMock.mockImplementation((): Promise<void> => Promise.reject(new Error("crash")))
+
+    const recordRestartSpy = spyOn(RestartTracker.prototype, "recordRestart").mockResolvedValue(true)
+    const exitSpy = spyOn(process, "exit").mockImplementation((_code?: number) => undefined as never)
+
+    await daemonCommand().catch(() => {})
+
+    expect(exitSpy).toHaveBeenCalledWith(0)
+
+    recordRestartSpy.mockRestore()
+    exitSpy.mockRestore()
+  })
+
+  it("calls rotateIfNeeded() before starting the polling loop", async () => {
+    const callOrder: string[] = []
+
+    rotateIfNeededMock.mockImplementation((_path: string): Promise<void> => {
+      callOrder.push("rotate")
+      return Promise.resolve()
+    })
+    startPollingMock.mockImplementation((_signal: AbortSignal): Promise<void> => {
+      callOrder.push("poll")
+      return Promise.resolve()
+    })
+
+    await daemonCommand()
+
+    const rotateIdx = callOrder.indexOf("rotate")
+    const pollIdx = callOrder.indexOf("poll")
+    expect(rotateIdx).toBeGreaterThanOrEqual(0)
+    expect(pollIdx).toBeGreaterThan(rotateIdx)
+  })
+})
diff --git a/01-core-daemon/tests/commands/start.test.ts b/01-core-daemon/tests/commands/start.test.ts
new file mode 100644
index 0000000..1717a86
--- /dev/null
+++ b/01-core-daemon/tests/commands/start.test.ts
@@ -0,0 +1,228 @@
+import { describe, it, expect, mock, spyOn, beforeEach } from "bun:test"
+import { realpathSync } from "node:fs"
+import { FriendlyError } from "../../src/shared/errors"
+
+const validConfig = {
+  telegram: { bot_token: "123:abc" },
+  jira: { base_url: "https://test.atlassian.net", api_token: "token", email: "e@e.com", project_key: "TEST" },
+  claude: { binary_path: "/usr/bin/true" },
+  app: { log_level: "info" as const },
+}
+
+const loadConfigMock = mock(() => Promise.resolve(validConfig))
+const configExistsMock = mock(() => Promise.resolve(true))
+const writeConfigMock = mock(() => Promise.resolve())
+const runWizardMock = mock(() => Promise.resolve(validConfig))
+const agentStatusMock = mock(() => Promise.resolve({ running: false }))
+const writePlistMock = mock((_path: string) => Promise.resolve())
+const loadAgentMock = mock(() => Promise.resolve())
+const unloadAgentMock = mock(() => Promise.resolve())
+const removePidMock = mock(() => Promise.resolve())
+const mkdirMock = mock((_path: string, _opts?: any) => Promise.resolve(undefined as any))
+const accessMock = mock((_path: string, _mode?: number) => Promise.resolve())
+
+mock.module("../../src/config/loader", () => ({
+  loadConfig: loadConfigMock,
+  configExists: configExistsMock,
+  writeConfig: writeConfigMock,
+}))
+mock.module("../../src/config/wizard", () => ({ runWizard: runWizardMock }))
+mock.module("../../src/daemon/launchd", () => ({
+  agentStatus: agentStatusMock,
+  writePlist: writePlistMock,
+  loadAgent: loadAgentMock,
+  unloadAgent: unloadAgentMock,
+  generatePlist: mock(() => ""),
+}))
+mock.module("../../src/daemon/pid", () => ({
+  writePid: mock(() => Promise.resolve()),
+  readPid: mock(() => Promise.resolve(null)),
+  removePid: removePidMock,
+  isProcessRunning: mock(() => Promise.resolve(false)),
+}))
+mock.module("node:fs/promises", () => ({
+  mkdir: mkdirMock,
+  access: accessMock,
+  writeFile: mock(() => Promise.resolve()),
+  rename: mock(() => Promise.resolve()),
+  chmod: mock(() => Promise.resolve()),
+  unlink: mock(() => Promise.resolve()),
+}))
+
+import { startCommand } from "../../src/commands/start"
+
+describe("preflight()", () => {
+  beforeEach(() => {
+    loadConfigMock.mockImplementation(() => Promise.resolve(validConfig))
+    configExistsMock.mockImplementation(() => Promise.resolve(true))
+    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false }))
+    writePlistMock.mockClear()
+    loadAgentMock.mockClear()
+    mkdirMock.mockClear()
+    accessMock.mockImplementation(() => Promise.resolve())
+  })
+
+  it("throws FriendlyError mentioning macOS when running on Linux", async () => {
+    const origDescriptor = Object.getOwnPropertyDescriptor(process, "platform")
+    Object.defineProperty(process, "platform", { value: "linux", configurable: true })
+
+    let caughtErr: unknown
+    try {
+      await startCommand()
+    } catch (e) {
+      caughtErr = e
+    } finally {
+      if (origDescriptor) Object.defineProperty(process, "platform", origDescriptor)
+    }
+
+    expect(caughtErr).toBeInstanceOf(FriendlyError)
+    expect((caughtErr as FriendlyError).message).toContain("macOS")
+  })
+
+  it("creates ~/Library/LaunchAgents dir when missing", async () => {
+    mkdirMock.mockClear()
+
+    let statusCalls = 0
+    agentStatusMock.mockImplementation(() => {
+      statusCalls++
+      return Promise.resolve({ running: statusCalls > 0, pid: 42 })
+    })
+
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+    await startCommand()
+
+    expect(mkdirMock).toHaveBeenCalledWith(
+      expect.stringContaining("LaunchAgents"),
+      expect.objectContaining({ recursive: true })
+    )
+
+    stdoutSpy.mockRestore()
+  })
+
+  it("throws FriendlyError when claude binary path is not executable", async () => {
+    accessMock.mockImplementation(() => Promise.reject(new Error("EACCES")))
+
+    let caughtErr: unknown
+    try {
+      await startCommand()
+    } catch (e) {
+      caughtErr = e
+    }
+
+    expect(caughtErr).toBeInstanceOf(FriendlyError)
+  })
+})
+
+describe("startCommand()", () => {
+  beforeEach(() => {
+    loadConfigMock.mockImplementation(() => Promise.resolve(validConfig))
+    configExistsMock.mockImplementation(() => Promise.resolve(true))
+    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false }))
+    writePlistMock.mockClear()
+    loadAgentMock.mockClear()
+    unloadAgentMock.mockClear()
+    removePidMock.mockClear()
+    accessMock.mockImplementation(() => Promise.resolve())
+  })
+
+  it("triggers wizard flow when no config exists (mocks runWizard and writeConfig)", async () => {
+    configExistsMock.mockImplementation(() => Promise.resolve(false))
+    runWizardMock.mockClear()
+    writeConfigMock.mockClear()
+
+    let statusCalls = 0
+    agentStatusMock.mockImplementation(() => {
+      statusCalls++
+      return Promise.resolve({ running: statusCalls > 1, pid: 42 })
+    })
+
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+    await startCommand()
+
+    expect(runWizardMock).toHaveBeenCalled()
+    expect(writeConfigMock).toHaveBeenCalled()
+
+    stdoutSpy.mockRestore()
+  })
+
+  it("calls stopCommand first when daemon is already running", async () => {
+    unloadAgentMock.mockClear()
+
+    let statusCalls = 0
+    agentStatusMock.mockImplementation(() => {
+      statusCalls++
+      if (statusCalls === 1) return Promise.resolve({ running: true, pid: 100 })
+      return Promise.resolve({ running: true, pid: 200 })
+    })
+
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+    await startCommand()
+
+    // stopCommand calls unloadAgent — verifies stopCommand was invoked
+    expect(unloadAgentMock).toHaveBeenCalled()
+
+    stdoutSpy.mockRestore()
+  })
+
+  it("calls writePlist with realpathSync(Bun.argv[0]), not process.execPath directly", async () => {
+    writePlistMock.mockClear()
+    let statusCalls = 0
+    agentStatusMock.mockImplementation(() => {
+      statusCalls++
+      return Promise.resolve({ running: statusCalls > 0, pid: 42 })
+    })
+
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+    await startCommand()
+
+    expect(writePlistMock).toHaveBeenCalledWith(realpathSync(Bun.argv[0]))
+
+    stdoutSpy.mockRestore()
+  })
+
+  it("polls agentStatus until running (not-running for first 2 polls, then running)", async () => {
+    const sleepSpy = spyOn(Bun, "sleep").mockResolvedValue(undefined)
+
+    let pollCalls = 0
+    agentStatusMock.mockImplementation(() => {
+      pollCalls++
+      if (pollCalls <= 2) return Promise.resolve({ running: false })
+      return Promise.resolve({ running: true, pid: 999 })
+    })
+
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+    await startCommand()
+
+    expect(pollCalls).toBeGreaterThanOrEqual(3)
+
+    sleepSpy.mockRestore()
+    stdoutSpy.mockRestore()
+  })
+
+  it("exits with failure message after 5s timeout if never reaches running state", async () => {
+    const sleepSpy = spyOn(Bun, "sleep").mockResolvedValue(undefined)
+    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false, exitCode: 127 }))
+
+    const stderrSpy = spyOn(process.stderr, "write").mockImplementation(() => true)
+    const exitSpy = spyOn(process, "exit").mockImplementation((_code?: number) => undefined as never)
+
+    let nowCalls = 0
+    const origNow = Date.now
+    Date.now = () => {
+      nowCalls++
+      return nowCalls <= 2 ? origNow() : origNow() + 6000
+    }
+
+    try {
+      await startCommand()
+    } catch {}
+
+    expect(exitSpy).toHaveBeenCalledWith(1)
+    expect(stderrSpy).toHaveBeenCalledWith(expect.stringContaining("failed to start"))
+
+    Date.now = origNow
+    sleepSpy.mockRestore()
+    stderrSpy.mockRestore()
+    exitSpy.mockRestore()
+  })
+})
diff --git a/01-core-daemon/tests/commands/status.test.ts b/01-core-daemon/tests/commands/status.test.ts
new file mode 100644
index 0000000..ed5aec2
--- /dev/null
+++ b/01-core-daemon/tests/commands/status.test.ts
@@ -0,0 +1,97 @@
+import { describe, it, expect, mock, spyOn } from "bun:test"
+
+const agentStatusMock = mock(() => Promise.resolve({ running: true, pid: 12345 }))
+const readPidMock = mock(() => Promise.resolve(12345))
+const loadConfigMock = mock(() => Promise.resolve({
+  telegram: { bot_token: "123:abc" },
+  jira: { base_url: "https://myorg.atlassian.net", api_token: "token", email: "e@e.com", project_key: "ENG" },
+  claude: { binary_path: "/usr/bin/claude" },
+  app: { log_level: "info" as const },
+}))
+
+mock.module("../../src/daemon/launchd", () => ({
+  agentStatus: agentStatusMock,
+  writePlist: mock(() => Promise.resolve()),
+  loadAgent: mock(() => Promise.resolve()),
+  unloadAgent: mock(() => Promise.resolve()),
+  generatePlist: mock(() => ""),
+}))
+mock.module("../../src/daemon/pid", () => ({
+  readPid: readPidMock,
+  writePid: mock(() => Promise.resolve()),
+  removePid: mock(() => Promise.resolve()),
+  isProcessRunning: mock(() => Promise.resolve(false)),
+}))
+mock.module("../../src/config/loader", () => ({
+  loadConfig: loadConfigMock,
+  configExists: mock(() => Promise.resolve(true)),
+  writeConfig: mock(() => Promise.resolve()),
+}))
+
+import { statusCommand } from "../../src/commands/status"
+
+describe("statusCommand()", () => {
+  it("output contains 'running' and PID when daemon is running", async () => {
+    agentStatusMock.mockImplementation(() => Promise.resolve({ running: true, pid: 12345 }))
+    readPidMock.mockImplementation(() => Promise.resolve(12345))
+
+    const chunks: string[] = []
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation((chunk: any) => {
+      chunks.push(String(chunk))
+      return true
+    })
+
+    await statusCommand()
+
+    const output = chunks.join("")
+    expect(output).toContain("running")
+    expect(output).toContain("12345")
+
+    stdoutSpy.mockRestore()
+  })
+
+  it("output contains 'stopped' when daemon is not running", async () => {
+    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false }))
+    readPidMock.mockImplementation(() => Promise.resolve(null))
+
+    const chunks: string[] = []
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation((chunk: any) => {
+      chunks.push(String(chunk))
+      return true
+    })
+
+    await statusCommand()
+
+    const output = chunks.join("")
+    expect(output).toContain("stopped")
+
+    stdoutSpy.mockRestore()
+  })
+
+  it("skips config section but still shows launchd state when no config file exists", async () => {
+    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false }))
+    readPidMock.mockImplementation(() => Promise.resolve(null))
+    loadConfigMock.mockImplementation(() => Promise.reject(new Error("ENOENT")))
+
+    const chunks: string[] = []
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation((chunk: any) => {
+      chunks.push(String(chunk))
+      return true
+    })
+
+    await statusCommand()
+
+    const output = chunks.join("")
+    expect(output).toContain("stopped")
+    expect(output).not.toContain("Jira URL")
+
+    loadConfigMock.mockImplementation(() => Promise.resolve({
+      telegram: { bot_token: "123:abc" },
+      jira: { base_url: "https://myorg.atlassian.net", api_token: "token", email: "e@e.com", project_key: "ENG" },
+      claude: { binary_path: "/usr/bin/claude" },
+      app: { log_level: "info" as const },
+    }))
+
+    stdoutSpy.mockRestore()
+  })
+})
diff --git a/01-core-daemon/tests/commands/stop.test.ts b/01-core-daemon/tests/commands/stop.test.ts
new file mode 100644
index 0000000..53882d7
--- /dev/null
+++ b/01-core-daemon/tests/commands/stop.test.ts
@@ -0,0 +1,56 @@
+import { describe, it, expect, mock, spyOn } from "bun:test"
+import { LaunchctlError } from "../../src/shared/errors"
+
+const unloadAgentMock = mock((): Promise<void> => Promise.resolve())
+const removePidMock = mock((): Promise<void> => Promise.resolve())
+
+mock.module("../../src/daemon/launchd", () => ({
+  unloadAgent: unloadAgentMock,
+  loadAgent: mock(() => Promise.resolve()),
+  agentStatus: mock(() => Promise.resolve({ running: false })),
+  writePlist: mock(() => Promise.resolve()),
+  generatePlist: mock(() => ""),
+}))
+mock.module("../../src/daemon/pid", () => ({
+  removePid: removePidMock,
+  writePid: mock(() => Promise.resolve()),
+  readPid: mock(() => Promise.resolve(null)),
+  isProcessRunning: mock(() => Promise.resolve(false)),
+}))
+
+import { stopCommand } from "../../src/commands/stop"
+import { FriendlyError } from "../../src/shared/errors"
+
+describe("stopCommand()", () => {
+  it("calls unloadAgent() then removePid() in order", async () => {
+    const callOrder: string[] = []
+    unloadAgentMock.mockImplementation(async () => { callOrder.push("unload") })
+    removePidMock.mockImplementation(async () => { callOrder.push("removePid") })
+
+    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
+    await stopCommand()
+
+    expect(callOrder).toEqual(["unload", "removePid"])
+
+    stdoutSpy.mockRestore()
+    unloadAgentMock.mockImplementation(() => Promise.resolve())
+    removePidMock.mockImplementation(() => Promise.resolve())
+  })
+
+  it("surfaces friendly error message when unloadAgent throws LaunchctlError", async () => {
+    unloadAgentMock.mockImplementation(() => {
+      throw new LaunchctlError("permission denied", "Check file permissions on the plist")
+    })
+
+    let caughtErr: unknown
+    try {
+      await stopCommand()
+    } catch (e) {
+      caughtErr = e
+    }
+
+    expect(caughtErr).toBeInstanceOf(FriendlyError)
+
+    unloadAgentMock.mockImplementation(() => Promise.resolve())
+  })
+})
diff --git a/02-integration-clients/src/index.ts b/02-integration-clients/src/index.ts
new file mode 100644
index 0000000..5f8fe81
--- /dev/null
+++ b/02-integration-clients/src/index.ts
@@ -0,0 +1,4 @@
+export async function startPolling(_signal: AbortSignal): Promise<void> {
+  // placeholder — implemented in 02-integration-clients section
+  await new Promise<void>(() => {})
+}
