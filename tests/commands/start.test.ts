import { describe, it, expect, mock, spyOn, beforeEach } from "bun:test"
import { realpathSync } from "node:fs"
import { FriendlyError } from "../../src/shared/errors"

const validConfig = {
  telegram: { bot_token: "123:abc" },
  jira: { base_url: "https://test.atlassian.net", api_token: "token", email: "e@e.com", project_key: "TEST" },
  claude: { binary_path: "/usr/bin/true" },
  app: { log_level: "info" as const },
}

const loadConfigMock = mock(() => Promise.resolve(validConfig))
const configExistsMock = mock(() => Promise.resolve(true))
const writeConfigMock = mock(() => Promise.resolve())
const runWizardMock = mock(() => Promise.resolve(validConfig))
const agentStatusMock = mock(() => Promise.resolve({ running: false }))
const writePlistMock = mock((_path: string) => Promise.resolve())
const loadAgentMock = mock(() => Promise.resolve())
const unloadAgentMock = mock(() => Promise.resolve())
const removePidMock = mock(() => Promise.resolve())
const mkdirMock = mock((_path: string, _opts?: any) => Promise.resolve(undefined as any))
const accessMock = mock((_path: string, _mode?: number) => Promise.resolve())

mock.module("../../src/config/loader", () => ({
  loadConfig: loadConfigMock,
  configExists: configExistsMock,
  writeConfig: writeConfigMock,
}))
mock.module("../../src/config/wizard", () => ({ runWizard: runWizardMock }))
mock.module("../../src/daemon/launchd", () => ({
  agentStatus: agentStatusMock,
  writePlist: writePlistMock,
  loadAgent: loadAgentMock,
  unloadAgent: unloadAgentMock,
  generatePlist: mock(() => ""),
}))
mock.module("../../src/daemon/pid", () => ({
  writePid: mock(() => Promise.resolve()),
  readPid: mock(() => Promise.resolve(null)),
  removePid: removePidMock,
  isProcessRunning: mock(() => Promise.resolve(false)),
}))
mock.module("node:fs/promises", () => ({
  mkdir: mkdirMock,
  access: accessMock,
  writeFile: mock(() => Promise.resolve()),
  rename: mock(() => Promise.resolve()),
  chmod: mock(() => Promise.resolve()),
  unlink: mock(() => Promise.resolve()),
}))

import { startCommand } from "../../src/commands/start"

describe("preflight()", () => {
  beforeEach(() => {
    loadConfigMock.mockImplementation(() => Promise.resolve(validConfig))
    configExistsMock.mockImplementation(() => Promise.resolve(true))
    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false }))
    writePlistMock.mockClear()
    loadAgentMock.mockClear()
    mkdirMock.mockClear()
    accessMock.mockImplementation(() => Promise.resolve())
  })

  it("throws FriendlyError mentioning macOS when running on Linux", async () => {
    const origDescriptor = Object.getOwnPropertyDescriptor(process, "platform")
    Object.defineProperty(process, "platform", { value: "linux", configurable: true })

    let caughtErr: unknown
    try {
      await startCommand()
    } catch (e) {
      caughtErr = e
    } finally {
      if (origDescriptor) Object.defineProperty(process, "platform", origDescriptor)
    }

    expect(caughtErr).toBeInstanceOf(FriendlyError)
    expect((caughtErr as FriendlyError).message).toContain("macOS")
  })

  it("creates ~/Library/LaunchAgents dir when missing", async () => {
    mkdirMock.mockClear()

    let statusCalls = 0
    agentStatusMock.mockImplementation(() => {
      statusCalls++
      return Promise.resolve({ running: statusCalls > 0, pid: 42 })
    })

    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
    await startCommand()

    expect(mkdirMock).toHaveBeenCalledWith(
      expect.stringContaining("LaunchAgents"),
      expect.objectContaining({ recursive: true })
    )

    stdoutSpy.mockRestore()
  })

  it("throws FriendlyError when claude binary path is not executable", async () => {
    accessMock.mockImplementation(() => Promise.reject(new Error("EACCES")))

    let caughtErr: unknown
    try {
      await startCommand()
    } catch (e) {
      caughtErr = e
    }

    expect(caughtErr).toBeInstanceOf(FriendlyError)
  })
})

describe("startCommand()", () => {
  beforeEach(() => {
    loadConfigMock.mockImplementation(() => Promise.resolve(validConfig))
    configExistsMock.mockImplementation(() => Promise.resolve(true))
    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false }))
    writePlistMock.mockClear()
    loadAgentMock.mockClear()
    unloadAgentMock.mockClear()
    removePidMock.mockClear()
    accessMock.mockImplementation(() => Promise.resolve())
  })

  it("triggers wizard flow when no config exists (mocks runWizard and writeConfig)", async () => {
    configExistsMock.mockImplementation(() => Promise.resolve(false))
    runWizardMock.mockClear()
    writeConfigMock.mockClear()

    let statusCalls = 0
    agentStatusMock.mockImplementation(() => {
      statusCalls++
      return Promise.resolve({ running: statusCalls > 1, pid: 42 })
    })

    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
    await startCommand()

    expect(runWizardMock).toHaveBeenCalled()
    expect(writeConfigMock).toHaveBeenCalled()

    stdoutSpy.mockRestore()
  })

  it("calls stopCommand first when daemon is already running", async () => {
    unloadAgentMock.mockClear()

    let statusCalls = 0
    agentStatusMock.mockImplementation(() => {
      statusCalls++
      if (statusCalls === 1) return Promise.resolve({ running: true, pid: 100 })
      return Promise.resolve({ running: true, pid: 200 })
    })

    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
    await startCommand()

    // stopCommand calls unloadAgent — verifies stopCommand was invoked
    expect(unloadAgentMock).toHaveBeenCalled()

    stdoutSpy.mockRestore()
  })

  it("calls writePlist with realpathSync(Bun.argv[0]), not process.execPath directly", async () => {
    writePlistMock.mockClear()
    let statusCalls = 0
    agentStatusMock.mockImplementation(() => {
      statusCalls++
      return Promise.resolve({ running: statusCalls > 0, pid: 42 })
    })

    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
    await startCommand()

    expect(writePlistMock).toHaveBeenCalledWith(realpathSync(Bun.argv[0]))

    stdoutSpy.mockRestore()
  })

  it("polls agentStatus until running (not-running for first 2 polls, then running)", async () => {
    const sleepSpy = spyOn(Bun, "sleep").mockResolvedValue(undefined)

    let pollCalls = 0
    agentStatusMock.mockImplementation(() => {
      pollCalls++
      if (pollCalls <= 2) return Promise.resolve({ running: false })
      return Promise.resolve({ running: true, pid: 999 })
    })

    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
    await startCommand()

    expect(pollCalls).toBeGreaterThanOrEqual(3)

    sleepSpy.mockRestore()
    stdoutSpy.mockRestore()
  })

  it("exits with failure message after 5s timeout if never reaches running state", async () => {
    const sleepSpy = spyOn(Bun, "sleep").mockResolvedValue(undefined)
    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false, exitCode: 127 }))

    const stderrSpy = spyOn(process.stderr, "write").mockImplementation(() => true)
    const exitSpy = spyOn(process, "exit").mockImplementation((_code?: number) => undefined as never)

    let nowCalls = 0
    const origNow = Date.now
    Date.now = () => {
      nowCalls++
      return nowCalls <= 2 ? origNow() : origNow() + 6000
    }

    try {
      await startCommand()
    } catch {}

    expect(exitSpy).toHaveBeenCalledWith(1)
    expect(stderrSpy).toHaveBeenCalledWith(expect.stringContaining("failed to start"))

    Date.now = origNow
    sleepSpy.mockRestore()
    stderrSpy.mockRestore()
    exitSpy.mockRestore()
  })
})
