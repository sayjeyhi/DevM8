import { describe, it, expect, spyOn, beforeEach, afterEach, mock } from "bun:test"
import { join } from "path"
import { tmpdir } from "os"
import { unlinkSync } from "fs"
import { generatePlist, writePlist, loadAgent, unloadAgent, agentStatus } from "../../src/daemon/launchd"
import { PATHS } from "../../src/shared/paths"
import { LaunchctlError } from "../../src/shared/errors"

const BINARY = "/usr/local/bin/jira-assistant"

describe("generatePlist", () => {
  it("contains the correct Label key", () => {
    const xml = generatePlist(BINARY)
    expect(xml).toContain("<key>Label</key>")
    expect(xml).toContain("<string>net.jira-assistant</string>")
  })

  it("contains KeepAlive as a dictionary with SuccessfulExit=false and Crashed=true (not simple boolean true)", () => {
    const xml = generatePlist(BINARY)
    expect(xml).toContain("<key>KeepAlive</key>")
    expect(xml).toContain("<key>SuccessfulExit</key>")
    expect(xml).toContain("<false/>")
    expect(xml).toContain("<key>Crashed</key>")
    expect(xml).toContain("<true/>")
    // Must NOT be the simple form
    expect(xml).not.toMatch(/<key>KeepAlive<\/key>\s*<true\/>/)
  })

  it("contains ThrottleInterval of 10", () => {
    const xml = generatePlist(BINARY)
    expect(xml).toContain("<key>ThrottleInterval</key>")
    expect(xml).toContain("<integer>10</integer>")
  })

  it("contains ProgramArguments with binary path and 'daemon' subcommand", () => {
    const xml = generatePlist(BINARY)
    expect(xml).toContain("<key>ProgramArguments</key>")
    expect(xml).toContain(`<string>${BINARY}</string>`)
    expect(xml).toContain("<string>daemon</string>")
  })

  it("does NOT contain StandardOutPath key", () => {
    const xml = generatePlist(BINARY)
    expect(xml).not.toContain("StandardOutPath")
  })

  it("does NOT contain StandardErrorPath key", () => {
    const xml = generatePlist(BINARY)
    expect(xml).not.toContain("StandardErrorPath")
  })
})

describe("writePlist", () => {
  let testPlistPath: string

  beforeEach(() => {
    testPlistPath = join(tmpdir(), `test-${Date.now()}.plist`)
  })

  afterEach(() => {
    try { unlinkSync(testPlistPath) } catch {}
  })

  it("creates the plist file at PATHS.plistFile", async () => {
    await writePlist(BINARY, testPlistPath)
    const content = await Bun.file(testPlistPath).text()
    expect(content).toContain("net.jira-assistant")
    expect(content).toContain(BINARY)
  })
})

// Helper: create a mock Bun.spawn result
function makeSpawnResult(exitCode: number, stdout = "", stderr = "") {
  return {
    exited: Promise.resolve(exitCode),
    stdout: new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode(stdout))
        controller.close()
      }
    }),
    stderr: new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode(stderr))
        controller.close()
      }
    }),
  }
}

describe("loadAgent", () => {
  it("calls Bun.spawn with ['launchctl', 'load', '-w', PATHS.plistFile]", async () => {
    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(makeSpawnResult(0) as any)
    await loadAgent()
    expect(spawnSpy).toHaveBeenCalledWith(
      expect.arrayContaining(["launchctl", "load", "-w", PATHS.plistFile]),
      expect.anything()
    )
    spawnSpy.mockRestore()
  })

  it("throws LaunchctlError containing raw stderr when launchctl exits non-zero", async () => {
    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(
      makeSpawnResult(1, "", "No such file or directory") as any
    )
    await expect(loadAgent()).rejects.toBeInstanceOf(LaunchctlError)
    spawnSpy.mockRestore()
  })
})

describe("unloadAgent", () => {
  it("calls Bun.spawn with ['launchctl', 'unload', '-w', PATHS.plistFile]", async () => {
    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(makeSpawnResult(0) as any)
    await unloadAgent()
    expect(spawnSpy).toHaveBeenCalledWith(
      expect.arrayContaining(["launchctl", "unload", "-w", PATHS.plistFile]),
      expect.anything()
    )
    spawnSpy.mockRestore()
  })

  it("throws LaunchctlError on non-zero exit", async () => {
    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(
      makeSpawnResult(1, "", "Permission denied") as any
    )
    await expect(unloadAgent()).rejects.toBeInstanceOf(LaunchctlError)
    spawnSpy.mockRestore()
  })
})

describe("agentStatus", () => {
  it("parses running process from launchctl print output (macOS 12+ format)", async () => {
    const printOutput = `{
      pid = 12345
      state = running
      last exit code = 0
    }`
    const spawnSpy = spyOn(Bun, "spawn").mockReturnValue(
      makeSpawnResult(0, printOutput, "") as any
    )
    const status = await agentStatus()
    expect(status.running).toBe(true)
    expect(status.pid).toBe(12345)
    spawnSpy.mockRestore()
  })

  it("falls back to launchctl list when print fails", async () => {
    const listOutput = "12345\t0\tnet.jira-assistant\n"
    let callCount = 0
    const spawnSpy = spyOn(Bun, "spawn").mockImplementation((() => {
      callCount++
      if (callCount === 1) return makeSpawnResult(1, "", "Unknown service") as any
      return makeSpawnResult(0, listOutput, "") as any
    }) as any)
    const status = await agentStatus()
    expect(status.running).toBe(true)
    expect(status.pid).toBe(12345)
    expect(status.exitCode).toBeUndefined()
    spawnSpy.mockRestore()
  })

  it("populates exitCode from launchctl list when process stopped", async () => {
    const listOutput = "-\t1\tnet.jira-assistant\n"
    let callCount = 0
    const spawnSpy = spyOn(Bun, "spawn").mockImplementation((() => {
      callCount++
      if (callCount === 1) return makeSpawnResult(1, "", "Unknown service") as any
      return makeSpawnResult(0, listOutput, "") as any
    }) as any)
    const status = await agentStatus()
    expect(status.running).toBe(false)
    expect(status.exitCode).toBe(1)
    spawnSpy.mockRestore()
  })

  it("returns { running: false } when agent is not loaded", async () => {
    const spawnSpy = spyOn(Bun, "spawn").mockImplementation(
      (() => makeSpawnResult(1, "", "Could not find service")) as any
    )
    const status = await agentStatus()
    expect(status.running).toBe(false)
    spawnSpy.mockRestore()
  })
})
