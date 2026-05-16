import { describe, it, expect, mock, spyOn } from "bun:test"

const wizardResultMock = {
  telegram: { bot_token: "123456:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef" },
  jira: { base_url: "https://test.atlassian.net", api_token: "token", email: "user@test.com", project_key: "TEST" },
  claude: { binary_path: "/usr/bin/claude" },
  app: { log_level: "info" as const },
}

const existingConfig = {
  ...wizardResultMock,
  jira: { ...wizardResultMock.jira, project_key: "EXISTING" },
}

const runWizardMock = mock((_existing?: typeof existingConfig) => Promise.resolve(wizardResultMock))
const loadConfigMock = mock(() => Promise.resolve(existingConfig))
const writeConfigMock = mock(() => Promise.resolve())
const configExistsMock = mock(() => Promise.resolve(true))

mock.module("../../src/config/wizard", () => ({ runWizard: runWizardMock }))
mock.module("../../src/config/loader", () => ({
  loadConfig: loadConfigMock,
  configExists: configExistsMock,
  writeConfig: writeConfigMock,
}))

import { configCommand } from "../../src/commands/config"

describe("configCommand()", () => {
  it("runs wizard with no existing argument when config does not exist", async () => {
    loadConfigMock.mockImplementation(() => Promise.reject(new Error("ENOENT")))
    runWizardMock.mockClear()

    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
    await configCommand()

    expect(runWizardMock).toHaveBeenCalledWith(undefined)

    runWizardMock.mockImplementation((_existing?: typeof existingConfig) => Promise.resolve(wizardResultMock))
    loadConfigMock.mockImplementation(() => Promise.resolve(existingConfig))
    stdoutSpy.mockRestore()
  })

  it("runs wizard pre-filled with existing values when config exists (mocks loadConfig)", async () => {
    loadConfigMock.mockImplementation(() => Promise.resolve(existingConfig))
    runWizardMock.mockClear()

    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
    await configCommand()

    expect(runWizardMock).toHaveBeenCalledWith(existingConfig)

    stdoutSpy.mockRestore()
  })

  it("calls writeConfig with the wizard result on completion", async () => {
    writeConfigMock.mockClear()
    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)

    await configCommand()

    expect(writeConfigMock).toHaveBeenCalledWith(wizardResultMock)

    stdoutSpy.mockRestore()
  })
})
