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

  intro("jira-assistant setup")

  const result = await group(
    {
      bot_token: () => text({
        message: "Telegram bot token",
        initialValue: existing?.telegram.bot_token,
        validate: (v) => (v && BOT_TOKEN_REGEX.test(v)) ? undefined : "Invalid Telegram bot token format. Expected format: 123456:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef",
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
    },
    {
      onCancel: () => { throw new FriendlyError("Setup cancelled.") },
    }
  )

  if (isCancel(result)) {
    throw new FriendlyError("Setup cancelled.")
  }

  const r = result as {
    bot_token: string; base_url: string; api_token: string
    email: string; project_key: string; binary_path: string
  }

  outro("Setup complete!")

  return {
    telegram: { bot_token: r.bot_token },
    jira: { base_url: r.base_url, api_token: r.api_token, email: r.email, project_key: r.project_key },
    claude: { binary_path: r.binary_path },
    app: { log_level: existing?.app.log_level ?? "info" },
  }
}
