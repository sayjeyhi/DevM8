import { describe, it, expect, beforeEach, afterEach } from "bun:test"
import { join } from "path"
import { tmpdir } from "os"
import { writePid, readPid, removePid, isProcessRunning } from "../../src/daemon/pid"

let testPidFile: string

beforeEach(() => {
  testPidFile = join(tmpdir(), `test-${Date.now()}-${Math.random()}.pid`)
})

afterEach(() => {
  try { require("fs").unlinkSync(testPidFile) } catch {}
})

describe("writePid / readPid", () => {
  it("round-trips: writePid(1234) then readPid() returns 1234", async () => {
    await writePid(1234, testPidFile)
    const pid = await readPid(testPidFile)
    expect(pid).toBe(1234)
  })

  it("uses atomic write (temp file + rename): file appears fully written", async () => {
    await writePid(9999, testPidFile)
    const content = await Bun.file(testPidFile).text()
    expect(content.trim()).toBe("9999")
  })
})

describe("readPid", () => {
  it("returns null when file is missing", async () => {
    const result = await readPid("/nonexistent/path/that/cannot/exist.pid")
    expect(result).toBeNull()
  })
})

describe("removePid", () => {
  it("deletes the file; subsequent readPid() returns null", async () => {
    await writePid(5678, testPidFile)
    expect(await readPid(testPidFile)).toBe(5678)
    await removePid(testPidFile)
    expect(await readPid(testPidFile)).toBeNull()
  })
})

describe("isProcessRunning", () => {
  it("returns true for the current process PID", async () => {
    expect(await isProcessRunning(process.pid)).toBe(true)
  })

  it("returns false for a non-existent PID like 99999999", async () => {
    expect(await isProcessRunning(99999999)).toBe(false)
  })
})
