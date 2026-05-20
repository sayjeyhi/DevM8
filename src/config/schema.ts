import { z } from "zod"

export const BOT_TOKEN_REGEX = /^\d+:[A-Za-z0-9_-]{20,}$/
export const PROJECT_KEY_REGEX = /^[A-Z][A-Z0-9_]+$/
export const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

export const AppConfigSchema = z.object({
  telegram: z.object({
    bot_token: z.string().regex(BOT_TOKEN_REGEX),
    allowed_user_ids: z.array(z.number().int().positive()).default([]),
  }),
  jira: z.object({
    base_url: z.string().refine((v) => {
      if (!v.startsWith("https://")) return false
      try { new URL(v); return true } catch { return false }
    }, { message: "Must be a valid HTTPS URL" }),
    api_token: z.string().min(1),
    email: z.string().email(),
    project_keys: z.array(z.string().regex(PROJECT_KEY_REGEX)).min(1),
  }),
  claude: z.object({
    binary_path: z.string().min(1),
    api_key: z.string().min(1).optional(),
  }),
  repos: z.record(
    z.string().regex(PROJECT_KEY_REGEX),
    z.array(z.string().min(1)).min(1),
  ).optional(),
  app: z.object({
    log_level: z.enum(["info", "debug", "error"]).default("info"),
  }).optional().default({ log_level: "info" }),
  slack: z.object({
    user_token: z.string().regex(/^xoxp-/, { message: "Must be a Slack User OAuth Token (xoxp-...)" }),
    poll_interval_ms: z.number().int().positive().default(30000),
  }).optional(),
})

export type AppConfig = z.infer<typeof AppConfigSchema>
