import { describe, it, expect, beforeEach, afterEach } from "bun:test"
import { rotateIfNeeded } from "../../src/logger/rotate"
import { writeFile, readFile, stat, rm, rename } from "fs/promises"
import { join } from "path"
import { tmpdir } from "os"
import { mkdtemp } from "fs/promises"

let tmpDir: string
let logFile: string

beforeEach(async () => {
  tmpDir = await mkdtemp(join(tmpdir(), "ja-test-"))
  logFile = join(tmpDir, "app.log")
})

afterEach(async () => {
  await rm(tmpDir, { recursive: true, force: true })
})

describe("rotateIfNeeded", () => {
  it("file size below maxBytes → no rotation, original file unchanged", async () => {
    await writeFile(logFile, "small content")
    await rotateIfNeeded(logFile, 1024 * 1024)
    const rotated = join(tmpDir, "app.log.1")
    let exists = false
    try {
      await stat(rotated)
      exists = true
    } catch {}
    expect(exists).toBe(false)
    const content = await readFile(logFile, "utf8")
    expect(content).toBe("small content")
  })

  it("file size at/above maxBytes → app.log.1 created with original content", async () => {
    const content = "x".repeat(100)
    await writeFile(logFile, content)
    await rotateIfNeeded(logFile, 50)
    const rotated = join(tmpDir, "app.log.1")
    const rotatedContent = await readFile(rotated, "utf8")
    expect(rotatedContent).toBe(content)
    const fresh = await readFile(logFile, "utf8")
    expect(fresh).toBe("")
  })

  it("second rotation → app.log.1 becomes app.log.2, new app.log.1 has previous app.log content", async () => {
    // First rotation
    const first = "first content"
    await writeFile(logFile, first)
    await rotateIfNeeded(logFile, 1)

    // Second rotation
    const second = "second content"
    await writeFile(logFile, second)
    await rotateIfNeeded(logFile, 1)

    const log1 = await readFile(join(tmpDir, "app.log.1"), "utf8")
    const log2 = await readFile(join(tmpDir, "app.log.2"), "utf8")
    expect(log1).toBe(second)
    expect(log2).toBe(first)
  })

  it("when keepCount files exist → oldest file deleted, others shifted", async () => {
    const keepCount = 3
    // Pre-create app.log.1 through app.log.keepCount
    for (let i = 1; i <= keepCount; i++) {
      await writeFile(join(tmpDir, `app.log.${i}`), `rotated-${i}`)
    }
    await writeFile(logFile, "x".repeat(100))
    await rotateIfNeeded(logFile, 50, keepCount)

    // app.log.<keepCount+1> must NOT exist
    let tooManyExist = false
    try {
      await stat(join(tmpDir, `app.log.${keepCount + 1}`))
      tooManyExist = true
    } catch {}
    expect(tooManyExist).toBe(false)

    // app.log.1 must exist
    const log1 = await stat(join(tmpDir, "app.log.1"))
    expect(log1.isFile()).toBe(true)
  })

  it("non-existent log file → no-op, no error thrown", async () => {
    const missing = join(tmpDir, "missing.log")
    await expect(rotateIfNeeded(missing)).resolves.toBeUndefined()
  })
})
