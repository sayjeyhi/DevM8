import { describe, it, expect, mock, spyOn } from "bun:test"
import { LaunchctlError } from "../../src/shared/errors"

const unloadAgentMock = mock((): Promise<void> => Promise.resolve())
const removePidMock = mock((): Promise<void> => Promise.resolve())

mock.module("../../src/daemon/launchd", () => ({
  unloadAgent: unloadAgentMock,
  loadAgent: mock(() => Promise.resolve()),
  agentStatus: mock(() => Promise.resolve({ running: false })),
  writePlist: mock(() => Promise.resolve()),
  generatePlist: mock(() => ""),
}))
mock.module("../../src/daemon/pid", () => ({
  removePid: removePidMock,
  writePid: mock(() => Promise.resolve()),
  readPid: mock(() => Promise.resolve(null)),
  isProcessRunning: mock(() => Promise.resolve(false)),
}))

import { stopCommand } from "../../src/commands/stop"
import { FriendlyError } from "../../src/shared/errors"

describe("stopCommand()", () => {
  it("calls unloadAgent() then removePid() in order", async () => {
    const callOrder: string[] = []
    unloadAgentMock.mockImplementation(async () => { callOrder.push("unload") })
    removePidMock.mockImplementation(async () => { callOrder.push("removePid") })

    const stdoutSpy = spyOn(process.stdout, "write").mockImplementation(() => true)
    await stopCommand()

    expect(callOrder).toEqual(["unload", "removePid"])

    stdoutSpy.mockRestore()
    unloadAgentMock.mockImplementation(() => Promise.resolve())
    removePidMock.mockImplementation(() => Promise.resolve())
  })

  it("surfaces friendly error message when unloadAgent throws LaunchctlError", async () => {
    unloadAgentMock.mockImplementation(() => {
      throw new LaunchctlError("permission denied", "Check file permissions on the plist")
    })

    let caughtErr: unknown
    try {
      await stopCommand()
    } catch (e) {
      caughtErr = e
    }

    expect(caughtErr).toBeInstanceOf(FriendlyError)

    unloadAgentMock.mockImplementation(() => Promise.resolve())
  })
})
