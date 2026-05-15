import { describe, it, expect, beforeEach, afterEach } from "bun:test"
import { loadConfig } from "../../src/bot/config"

const ENV_KEYS = [
  "TELEGRAM_BOT_TOKEN",
  "JIRA_BASE_URL",
  "JIRA_PROJECT_KEY",
  "JIRA_USER_EMAIL",
  "JIRA_API_TOKEN",
  "CLAUDE_API_KEY",
  "ALLOWED_USER_IDS",
] as const

const VALID_ENV: Record<string, string> = {
  TELEGRAM_BOT_TOKEN: "bot123:token",
  JIRA_BASE_URL: "https://test.atlassian.net",
  JIRA_PROJECT_KEY: "PROJ",
  JIRA_USER_EMAIL: "user@example.com",
  JIRA_API_TOKEN: "jira-secret",
  CLAUDE_API_KEY: "claude-secret",
  ALLOWED_USER_IDS: "123,456",
}

let savedEnv: Record<string, string | undefined> = {}

beforeEach(() => {
  savedEnv = {}
  for (const key of ENV_KEYS) {
    savedEnv[key] = process.env[key]
  }
  Object.assign(process.env, VALID_ENV)
})

afterEach(() => {
  for (const [key, val] of Object.entries(savedEnv)) {
    if (val === undefined) delete process.env[key]
    else process.env[key] = val
  }
})

describe("loadConfig()", () => {
  it("returns valid Config with all required fields present", () => {
    const config = loadConfig()
    expect(config.telegramBotToken).toBe("bot123:token")
    expect(config.jiraBaseUrl).toBe("https://test.atlassian.net")
    expect(config.jiraProjectKey).toBe("PROJ")
    expect(config.jiraUserEmail).toBe("user@example.com")
    expect(config.jiraApiToken).toBe("jira-secret")
    expect(config.claudeApiKey).toBe("claude-secret")
    expect(config.allowedUserIds).toEqual(new Set([123, 456]))
  })

  it("throws with message mentioning TELEGRAM_BOT_TOKEN when it is missing", () => {
    delete process.env.TELEGRAM_BOT_TOKEN
    expect(() => loadConfig()).toThrow("TELEGRAM_BOT_TOKEN")
  })

  it("throws when ALLOWED_USER_IDS is missing", () => {
    delete process.env.ALLOWED_USER_IDS
    expect(() => loadConfig()).toThrow("ALLOWED_USER_IDS")
  })

  it("parses ALLOWED_USER_IDS='123,456' into Set containing 123 and 456", () => {
    process.env.ALLOWED_USER_IDS = "123,456"
    const config = loadConfig()
    expect(config.allowedUserIds.has(123)).toBe(true)
    expect(config.allowedUserIds.has(456)).toBe(true)
    expect(config.allowedUserIds.size).toBe(2)
  })

  it("trims whitespace from ALLOWED_USER_IDS entries ('123, 456')", () => {
    process.env.ALLOWED_USER_IDS = "123, 456"
    const config = loadConfig()
    expect(config.allowedUserIds.has(123)).toBe(true)
    expect(config.allowedUserIds.has(456)).toBe(true)
    expect(config.allowedUserIds.size).toBe(2)
  })

  it("filters NaN entries from ALLOWED_USER_IDS without throwing ('abc,456')", () => {
    process.env.ALLOWED_USER_IDS = "abc,456"
    const config = loadConfig()
    expect(config.allowedUserIds.has(456)).toBe(true)
    expect(config.allowedUserIds.size).toBe(1)
  })

  it("returns empty Set when ALLOWED_USER_IDS is empty string", () => {
    process.env.ALLOWED_USER_IDS = ""
    const config = loadConfig()
    expect(config.allowedUserIds.size).toBe(0)
  })

  it("returns empty Set when ALLOWED_USER_IDS is whitespace only", () => {
    process.env.ALLOWED_USER_IDS = "  "
    const config = loadConfig()
    expect(config.allowedUserIds.size).toBe(0)
  })
})
