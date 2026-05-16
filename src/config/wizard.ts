import { existsSync } from "node:fs"
import { intro, outro, group, text, isCancel } from "@clack/prompts"
import { BOT_TOKEN_REGEX, PROJECT_KEY_REGEX, EMAIL_REGEX, type AppConfig } from "./schema"
import { FriendlyError } from "../shared/errors"

export async function runWizard(existing?: AppConfig): Promise<AppConfig> {
  if (!process.stdin.isTTY) {
    throw new FriendlyError(
      "Cannot run wizard in non-interactive mode.",
      "Attach a TTY or provide config manually."
    )
  }

  intro("DevMate setup")

  const result = await group(
    {
      bot_token: () => text({
        message: "Telegram bot token",
        initialValue: existing?.telegram.bot_token,
        validate: (v) => (v && BOT_TOKEN_REGEX.test(v)) ? undefined : "Invalid Telegram bot token format. Expected format: 123456:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef",
      }),
      allowed_user_ids: () => text({
        message: "Your Telegram user ID(s), comma-separated (send /start to @userinfobot to get yours)",
        initialValue: existing?.telegram.allowed_user_ids?.join(", ") ?? "",
        validate: (v) => {
          if (!v || v.trim() === "") return "At least one Telegram user ID is required"
          const ids = v.split(",").map(s => parseInt(s.trim(), 10))
          if (ids.some(n => isNaN(n) || n <= 0)) return "All values must be positive integers"
        },
      }),
      base_url: () => text({
        message: "Jira base URL (e.g. https://mycompany.atlassian.net)",
        initialValue: existing?.jira.base_url,
        validate: (v) => {
          if (!v || !v.startsWith("https://")) return "Must be a valid HTTPS URL"
          try { new URL(v) } catch { return "Must be a valid HTTPS URL" }
        },
      }),
      api_token: () => text({
        message: "Jira API token",
        initialValue: existing?.jira.api_token,
        validate: (v) => (v && v.length > 0) ? undefined : "API token is required",
      }),
      email: () => text({
        message: "Jira account email",
        initialValue: existing?.jira.email,
        validate: (v) => (v && EMAIL_REGEX.test(v)) ? undefined : "Must be a valid email address",
      }),
      project_key: () => text({
        message: "Jira project key (e.g. MYPROJECT)",
        initialValue: existing?.jira.project_key,
        validate: (v) => (v && PROJECT_KEY_REGEX.test(v)) ? undefined : "Must be uppercase letters only, e.g. MYPROJECT",
      }),
      binary_path: () => text({
        message: "Path to claude binary",
        initialValue: existing?.claude.binary_path ?? (Bun.which("claude") ?? ""),
        validate: (v) => (v && existsSync(v)) ? undefined : "File not found at this path",
      }),
      claude_api_key: () => text({
        message: "Anthropic API key (leave blank to use ANTHROPIC_API_KEY env var)",
        initialValue: existing?.claude.api_key ?? "",
      }),
    },
    {
      onCancel: () => { throw new FriendlyError("Setup cancelled.") },
    }
  )

  if (isCancel(result)) {
    throw new FriendlyError("Setup cancelled.")
  }

  const r = result as {
    bot_token: string; allowed_user_ids: string; base_url: string; api_token: string
    email: string; project_key: string; binary_path: string; claude_api_key: string
  }

  outro("Setup complete!")

  const allowedUserIds = r.allowed_user_ids
    .split(",")
    .map(s => parseInt(s.trim(), 10))
    .filter(n => !isNaN(n) && n > 0)

  return {
    telegram: { bot_token: r.bot_token, allowed_user_ids: allowedUserIds },
    jira: { base_url: r.base_url, api_token: r.api_token, email: r.email, project_key: r.project_key },
    claude: { binary_path: r.binary_path, api_key: r.claude_api_key || undefined },
    app: { log_level: existing?.app.log_level ?? "info" },
  }
}
