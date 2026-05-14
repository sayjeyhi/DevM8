import { describe, it, expect, beforeEach, afterEach } from "bun:test"
import { join } from "path"
import { tmpdir } from "os"
import { unlinkSync } from "fs"
import { RestartTracker } from "../../src/daemon/restart-tracker"

let testFile: string

beforeEach(() => {
  testFile = join(tmpdir(), `restarts-${Date.now()}-${Math.random()}.json`)
})

afterEach(() => {
  try { unlinkSync(testFile) } catch {}
})

describe("RestartTracker", () => {
  it("first recordRestart() returns false (under limit)", async () => {
    const tracker = new RestartTracker(testFile, 3, 60_000)
    expect(await tracker.recordRestart()).toBe(false)
  })

  it("reaching maxRestarts within windowMs returns true on final call", async () => {
    const tracker = new RestartTracker(testFile, 3, 60_000)
    expect(await tracker.recordRestart()).toBe(false) // 1 < 3
    expect(await tracker.recordRestart()).toBe(false) // 2 < 3
    expect(await tracker.recordRestart()).toBe(true)  // 3 >= 3
  })

  it("timestamps outside windowMs are pruned; pruned count goes back under limit", async () => {
    const tracker = new RestartTracker(testFile, 2, 1000)
    await tracker.recordRestart()
    await tracker.recordRestart()

    // Write old timestamps directly to file to simulate aged entries
    const old = Date.now() - 2000
    await Bun.write(testFile, JSON.stringify([old, old]))

    // Now recordRestart should prune the old ones and return false (1 entry, under limit of 2)
    expect(await tracker.recordRestart()).toBe(false)
  })

  it("recreating tracker pointing to same file reads persisted timestamps", async () => {
    const tracker1 = new RestartTracker(testFile, 3, 60_000)
    await tracker1.recordRestart() // 1
    await tracker1.recordRestart() // 2

    const tracker2 = new RestartTracker(testFile, 3, 60_000)
    // 3rd restart: 3 >= 3 → true
    expect(await tracker2.recordRestart()).toBe(true)
  })

  it("starts with empty array when state file is missing", async () => {
    const missingFile = join(tmpdir(), `restarts-missing-${Date.now()}.json`)
    const tracker = new RestartTracker(missingFile, 5, 60_000)
    expect(await tracker.recordRestart()).toBe(false)
  })

  it("reset() clears the persisted file", async () => {
    const tracker = new RestartTracker(testFile, 2, 60_000)
    await tracker.recordRestart()
    await tracker.recordRestart()
    await tracker.reset()

    const tracker2 = new RestartTracker(testFile, 2, 60_000)
    expect(await tracker2.recordRestart()).toBe(false)
  })
})
