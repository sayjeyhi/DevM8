import { existsSync } from "node:fs"
import { BOT_TOKEN_REGEX, PROJECT_KEY_REGEX, EMAIL_REGEX } from "./schema"

export function validateBotToken(v: string): string | undefined {
  return v && BOT_TOKEN_REGEX.test(v) ? undefined
    : "Invalid Telegram bot token format. Expected format: 123456:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef"
}

export function validateAllowedUserIds(v: string): string | undefined {
  if (!v || v.trim() === "") return "At least one Telegram user ID is required"
  const ids = v.split(",").map(s => parseInt(s.trim(), 10))
  if (ids.some(n => isNaN(n) || n <= 0)) return "All values must be positive integers"
}

export function validateJiraBaseUrl(v: string): string | undefined {
  if (!v || !v.startsWith("https://")) return "Must be a valid HTTPS URL"
  try { new URL(v) } catch { return "Must be a valid HTTPS URL" }
}

export function validateApiToken(v: string): string | undefined {
  return v && v.length > 0 ? undefined : "API token is required"
}

export function validateEmail(v: string): string | undefined {
  return v && EMAIL_REGEX.test(v) ? undefined : "Must be a valid email address"
}

export function validateProjectKey(v: string): string | undefined {
  return v && PROJECT_KEY_REGEX.test(v) ? undefined : "Must be uppercase letters only, e.g. MYPROJECT"
}

export function validateProjectKeys(v: string): string | undefined {
  if (!v || v.trim() === "") return "At least one project key is required"
  const keys = v.split(",").map(s => s.trim())
  for (const key of keys) {
    if (!PROJECT_KEY_REGEX.test(key)) {
      return `"${key}" is not a valid project key — use uppercase letters, e.g. MYPROJECT`
    }
  }
}

export function validateBinaryPath(v: string): string | undefined {
  return v && existsSync(v) ? undefined : "File not found at this path"
}
