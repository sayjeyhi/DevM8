import { describe, it, expect, mock, spyOn } from "bun:test"

const agentStatusMock = mock(() => Promise.resolve({ running: true, pid: 12345 }))
const readPidMock = mock(() => Promise.resolve(12345))
const loadConfigMock = mock(() => Promise.resolve({
  telegram: { bot_token: "123:abc" },
  jira: { base_url: "https://myorg.atlassian.net", api_token: "token", email: "e@e.com", project_key: "ENG" },
  claude: { binary_path: "/usr/bin/claude" },
  app: { log_level: "info" as const },
}))

mock.module("../../src/daemon/launchd", () => ({
  agentStatus: agentStatusMock,
  writePlist: mock(() => Promise.resolve()),
  loadAgent: mock(() => Promise.resolve()),
  unloadAgent: mock(() => Promise.resolve()),
  generatePlist: mock(() => ""),
}))
mock.module("../../src/daemon/pid", () => ({
  readPid: readPidMock,
  writePid: mock(() => Promise.resolve()),
  removePid: mock(() => Promise.resolve()),
  isProcessRunning: mock(() => Promise.resolve(false)),
}))
mock.module("../../src/config/loader", () => ({
  loadConfig: loadConfigMock,
  configExists: mock(() => Promise.resolve(true)),
  writeConfig: mock(() => Promise.resolve()),
}))

import { statusCommand } from "../../src/commands/status"

describe("statusCommand()", () => {
  it("output contains 'running' and PID when daemon is running", async () => {
    agentStatusMock.mockImplementation(() => Promise.resolve({ running: true, pid: 12345 }))
    readPidMock.mockImplementation(() => Promise.resolve(12345))

    const chunks: string[] = []
    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation((chunk: any) => {
      chunks.push(String(chunk))
      return true
    })

    await statusCommand()

    const output = chunks.join("")
    expect(output).toContain("running")
    expect(output).toContain("12345")

    stdoutSpy.mockRestore()
  })

  it("output contains 'stopped' when daemon is not running", async () => {
    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false }))
    readPidMock.mockImplementation(() => Promise.resolve(null))

    const chunks: string[] = []
    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation((chunk: any) => {
      chunks.push(String(chunk))
      return true
    })

    await statusCommand()

    const output = chunks.join("")
    expect(output).toContain("stopped")

    stdoutSpy.mockRestore()
  })

  it("skips config section but still shows launchd state when no config file exists", async () => {
    agentStatusMock.mockImplementation(() => Promise.resolve({ running: false }))
    readPidMock.mockImplementation(() => Promise.resolve(null))
    loadConfigMock.mockImplementation(() => Promise.reject(new Error("ENOENT")))

    const chunks: string[] = []
    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation((chunk: any) => {
      chunks.push(String(chunk))
      return true
    })

    await statusCommand()

    const output = chunks.join("")
    expect(output).toContain("stopped")
    expect(output).not.toContain("Jira URL")

    loadConfigMock.mockImplementation(() => Promise.resolve({
      telegram: { bot_token: "123:abc" },
      jira: { base_url: "https://myorg.atlassian.net", api_token: "token", email: "e@e.com", project_key: "ENG" },
      claude: { binary_path: "/usr/bin/claude" },
      app: { log_level: "info" as const },
    }))

    stdoutSpy.mockRestore()
  })
})
