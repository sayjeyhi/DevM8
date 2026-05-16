import { describe, it, expect, mock, spyOn, beforeEach } from "bun:test"

const startPollingMock = mock((_signal: AbortSignal): Promise<void> => Promise.resolve())
const rotateIfNeededMock = mock((_path: string): Promise<void> => Promise.resolve())
const writePidMock = mock((_pid: number): Promise<void> => Promise.resolve())
const removePidMock = mock((): Promise<void> => Promise.resolve())

mock.module("../../src/index", () => ({
  startPolling: startPollingMock,
}))
mock.module("../../src/logger/rotate", () => ({
  rotateIfNeeded: rotateIfNeededMock,
}))
mock.module("../../src/daemon/pid", () => ({
  writePid: writePidMock,
  removePid: removePidMock,
  readPid: mock(() => Promise.resolve(null)),
  isProcessRunning: mock(() => Promise.resolve(false)),
}))
mock.module("../../src/config/loader", () => ({
  loadConfig: mock(() => Promise.resolve({
    app: { log_level: "info" as const },
    telegram: { bot_token: "123:abc" },
    jira: { base_url: "https://test.atlassian.net", api_token: "tok", email: "e@e.com", project_key: "T" },
    claude: { binary_path: "/usr/bin/claude" },
  })),
  configExists: mock(() => Promise.resolve(true)),
  writeConfig: mock(() => Promise.resolve()),
}))

import { daemonCommand } from "../../src/commands/daemon"
import { RestartTracker } from "../../src/daemon/restart-tracker"

describe("daemonCommand()", () => {
  beforeEach(() => {
    startPollingMock.mockImplementation((_signal: AbortSignal): Promise<void> => Promise.resolve())
    rotateIfNeededMock.mockClear()
    writePidMock.mockClear()
    removePidMock.mockClear()
    process.removeAllListeners("SIGTERM")
  })

  it("calls shutdownController.abort() on SIGTERM (mocks polling loop entry)", async () => {
    let abortedBeforeResolve = false

    startPollingMock.mockImplementation((signal: AbortSignal): Promise<void> =>
      new Promise(resolve => {
        signal.addEventListener("abort", () => {
          abortedBeforeResolve = true
          resolve()
        }, { once: true })
      })
    )

    const exitSpy = spyOn(process, "exit").mockImplementation((_code?: number) => undefined as never)

    const runPromise = daemonCommand()

    // Tick to allow SIGTERM handler to register
    await new Promise(r => setTimeout(r, 10))

    process.emit("SIGTERM" as any)

    await runPromise

    // Allow async SIGTERM handler to complete
    await new Promise(r => setTimeout(r, 20))

    expect(abortedBeforeResolve).toBe(true)
    expect(removePidMock).toHaveBeenCalled()
    expect(exitSpy).toHaveBeenCalledWith(0)

    exitSpy.mockRestore()
  })

  it("calls restartTracker.recordRestart() and calls process.exit(1) when unhandled crash is under limit", async () => {
    startPollingMock.mockImplementation((): Promise<void> => Promise.reject(new Error("telegram API crash")))

    const recordRestartSpy = spyOn(RestartTracker.prototype, "recordRestart").mockResolvedValue(false)
    const exitSpy = spyOn(process, "exit").mockImplementation((_code?: number) => undefined as never)

    await daemonCommand().catch(() => {})

    expect(recordRestartSpy).toHaveBeenCalled()
    expect(exitSpy).toHaveBeenCalledWith(1)

    recordRestartSpy.mockRestore()
    exitSpy.mockRestore()
  })

  it("calls process.exit(0) when restartTracker.recordRestart() returns true (limit exceeded)", async () => {
    startPollingMock.mockImplementation((): Promise<void> => Promise.reject(new Error("crash")))

    const recordRestartSpy = spyOn(RestartTracker.prototype, "recordRestart").mockResolvedValue(true)
    const exitSpy = spyOn(process, "exit").mockImplementation((_code?: number) => undefined as never)

    await daemonCommand().catch(() => {})

    expect(exitSpy).toHaveBeenCalledWith(0)

    recordRestartSpy.mockRestore()
    exitSpy.mockRestore()
  })

  it("calls rotateIfNeeded() before starting the polling loop", async () => {
    const callOrder: string[] = []

    rotateIfNeededMock.mockImplementation((_path: string): Promise<void> => {
      callOrder.push("rotate")
      return Promise.resolve()
    })
    startPollingMock.mockImplementation((_signal: AbortSignal): Promise<void> => {
      callOrder.push("poll")
      return Promise.resolve()
    })

    await daemonCommand()

    const rotateIdx = callOrder.indexOf("rotate")
    const pollIdx = callOrder.indexOf("poll")
    expect(rotateIdx).toBeGreaterThanOrEqual(0)
    expect(pollIdx).toBeGreaterThan(rotateIdx)
  })
})
