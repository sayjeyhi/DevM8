import { describe, it, expect } from "bun:test"
import { homedir } from "os"
import { PATHS } from "../../src/shared/paths"

describe("PATHS", () => {
  it("all values are absolute paths (no ~ literals)", () => {
    for (const value of Object.values(PATHS)) {
      expect(value).not.toContain("~")
      expect(value.startsWith("/")).toBe(true)
    }
  })

  it("configDir starts with homedir()", () => {
    expect(PATHS.configDir.startsWith(homedir())).toBe(true)
  })

  it("plistFile contains Library/LaunchAgents and ends with .plist", () => {
    expect(PATHS.plistFile).toContain("Library/LaunchAgents")
    expect(PATHS.plistFile.endsWith(".plist")).toBe(true)
  })

  it("restartsFile ends with restarts.json", () => {
    expect(PATHS.restartsFile.endsWith("restarts.json")).toBe(true)
  })

  it("logFile ends with app.log", () => {
    expect(PATHS.logFile.endsWith("app.log")).toBe(true)
  })

  it("pidFile ends with daemon.pid", () => {
    expect(PATHS.pidFile.endsWith("daemon.pid")).toBe(true)
  })

  it("configFile ends with config.toml", () => {
    expect(PATHS.configFile.endsWith("config.toml")).toBe(true)
  })

  it("logsDir is a prefix of logFile", () => {
    expect(PATHS.logFile.startsWith(PATHS.logsDir)).toBe(true)
  })
})
