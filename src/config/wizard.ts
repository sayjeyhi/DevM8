import { intro, outro, group, text, isCancel } from "@clack/prompts"
import type { AppConfig } from "./schema"
import { FriendlyError } from "../shared/errors"
import {
  validateBotToken,
  validateAllowedUserIds,
  validateJiraBaseUrl,
  validateApiToken,
  validateEmail,
  validateProjectKey,
  validateBinaryPath,
} from "./validators"

export async function runWizard(existing?: AppConfig): Promise<AppConfig> {
  if (!process.stdin.isTTY) {
    throw new FriendlyError(
      "Cannot run wizard in non-interactive mode.",
      "Attach a TTY or provide config manually."
    )
  }

  intro("DevM8 setup")

  const result = await group(
    {
      bot_token: () => text({
        message: "Telegram bot token",
        initialValue: existing?.telegram.bot_token,
        validate: validateBotToken,
      }),
      allowed_user_ids: () => text({
        message: "Your Telegram user ID(s), comma-separated (send /start to @userinfobot to get yours)",
        initialValue: existing?.telegram.allowed_user_ids?.join(", ") ?? "",
        validate: validateAllowedUserIds,
      }),
      base_url: () => text({
        message: "Jira base URL (e.g. https://mycompany.atlassian.net)",
        initialValue: existing?.jira.base_url,
        validate: validateJiraBaseUrl,
      }),
      api_token: () => text({
        message: "Jira API token",
        initialValue: existing?.jira.api_token,
        validate: validateApiToken,
      }),
      email: () => text({
        message: "Jira account email",
        initialValue: existing?.jira.email,
        validate: validateEmail,
      }),
      project_key: () => text({
        message: "Jira project key (e.g. MYPROJECT)",
        initialValue: existing?.jira.project_key,
        validate: validateProjectKey,
      }),
      binary_path: () => text({
        message: "Path to claude binary",
        initialValue: existing?.claude.binary_path ?? (Bun.which("claude") ?? ""),
        validate: validateBinaryPath,
      }),
      api_key: () => text({
        message: "Anthropic API key (leave blank if claude is already logged in via `claude login`)",
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
    email: string; project_key: string; binary_path: string; api_key: string
  }

  outro("Setup complete!")

  const allowedUserIds = r.allowed_user_ids
    .split(",")
    .map(s => parseInt(s.trim(), 10))
    .filter(n => !isNaN(n) && n > 0)

  return {
    telegram: { bot_token: r.bot_token, allowed_user_ids: allowedUserIds },
    jira: { base_url: r.base_url, api_token: r.api_token, email: r.email, project_key: r.project_key },
    claude: { binary_path: r.binary_path, ...(r.api_key.trim() ? { api_key: r.api_key.trim() } : {}) },
    app: { log_level: existing?.app.log_level ?? "info" },
  }
}
