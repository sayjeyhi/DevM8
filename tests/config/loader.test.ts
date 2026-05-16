import { describe, it, expect, beforeEach, afterEach } from "bun:test"
import { join } from "path"
import { mkdtemp, rm, stat } from "node:fs/promises"
import { tmpdir } from "os"
import { loadConfig, configExists, writeConfig } from "../../src/config/loader"
import { FriendlyError } from "../../src/shared/errors"
import type { AppConfig } from "../../src/config/schema"

const VALID_TOML = `
[telegram]
bot_token = "123456789:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh"

[jira]
base_url = "https://mycompany.atlassian.net"
api_token = "my-api-token"
email = "user@example.com"
project_key = "MYPROJECT"

[claude]
binary_path = "/usr/local/bin/claude"

[app]
log_level = "info"
`

const VALID_CONFIG: AppConfig = {
  telegram: { bot_token: "123456789:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh" },
  jira: {
    base_url: "https://mycompany.atlassian.net",
    api_token: "my-api-token",
    email: "user@example.com",
    project_key: "MYPROJECT",
  },
  claude: { binary_path: "/usr/local/bin/claude" },
  app: { log_level: "info" },
}

let tmpDir: string

beforeEach(async () => {
  tmpDir = await mkdtemp(join(tmpdir(), "jira-assistant-test-"))
})

afterEach(async () => {
  await rm(tmpDir, { recursive: true, force: true })
})

describe("loadConfig", () => {
  it("parses valid TOML and returns AppConfig shape", async () => {
    const configPath = join(tmpDir, "config.toml")
    await Bun.write(configPath, VALID_TOML)
    const config = await loadConfig(configPath)
    expect(config.telegram.bot_token).toBe("123456789:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh")
    expect(config.jira.base_url).toBe("https://mycompany.atlassian.net")
    expect(config.app.log_level).toBe("info")
  })

  it("defaults app.log_level to 'info' when omitted", async () => {
    const configPath = join(tmpDir, "config.toml")
    const toml = VALID_TOML.replace(/log_level = "info"\n/, "")
    await Bun.write(configPath, toml)
    const config = await loadConfig(configPath)
    expect(config.app.log_level).toBe("info")
  })

  it("throws FriendlyError listing all invalid fields when required field missing", async () => {
    const configPath = join(tmpDir, "config.toml")
    const toml = `
[jira]
base_url = "https://mycompany.atlassian.net"
api_token = "token"
email = "user@example.com"
project_key = "PROJ"

[claude]
binary_path = "/usr/bin/claude"
`
    await Bun.write(configPath, toml)
    let err: unknown
    try { await loadConfig(configPath) } catch (e) { err = e }
    expect(err).toBeInstanceOf(FriendlyError)
    const msg = (err as FriendlyError).message
    expect(msg).toContain("telegram")
  })

  it("throws FriendlyError on malformed TOML", async () => {
    const configPath = join(tmpDir, "config.toml")
    await Bun.write(configPath, "key = ")
    let err: unknown
    try { await loadConfig(configPath) } catch (e) { err = e }
    expect(err).toBeInstanceOf(FriendlyError)
  })

  it("throws FriendlyError with jira-assistant config hint when file not found", async () => {
    let err: unknown
    try { await loadConfig("/nonexistent/path/config.toml") } catch (e) { err = e }
    expect(err).toBeInstanceOf(FriendlyError)
    const friendly = err as FriendlyError
    expect(friendly.message).toContain("jira-assistant config")
  })
})

describe("configExists", () => {
  it("returns false when file does not exist", async () => {
    const result = await configExists(join(tmpDir, "nonexistent.toml"))
    expect(result).toBe(false)
  })

  it("returns true when file exists", async () => {
    const configPath = join(tmpDir, "config.toml")
    await Bun.write(configPath, VALID_TOML)
    const result = await configExists(configPath)
    expect(result).toBe(true)
  })
})

describe("writeConfig", () => {
  it("creates missing directory and writes file", async () => {
    const configPath = join(tmpDir, "nested", "dir", "config.toml")
    await writeConfig(VALID_CONFIG, configPath)
    const exists = await Bun.file(configPath).exists()
    expect(exists).toBe(true)
  })

  it("round-trips config (writeConfig then loadConfig returns equal object)", async () => {
    const configPath = join(tmpDir, "config.toml")
    await writeConfig(VALID_CONFIG, configPath)
    const loaded = await loadConfig(configPath)
    expect(loaded).toEqual(VALID_CONFIG)
  })

  it("sets file permissions to 0o600", async () => {
    const configPath = join(tmpDir, "config.toml")
    await writeConfig(VALID_CONFIG, configPath)
    const s = await stat(configPath)
    expect(s.mode & 0o777).toBe(0o600)
  })

  it("uses atomic write (no leftover .tmp files after success)", async () => {
    const configPath = join(tmpDir, "config.toml")
    await writeConfig(VALID_CONFIG, configPath)
    const tmpFile = await Bun.file(configPath + ".tmp").exists()
    expect(tmpFile).toBe(false)
  })
})
