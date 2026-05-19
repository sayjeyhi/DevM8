import { intro, outro, text, multiselect, spinner, isCancel } from "@clack/prompts"
import type { AppConfig } from "./schema"
import { FriendlyError } from "../shared/errors"
import { JiraClient } from "../jira/JiraClient"
import {
  validateBotToken,
  validateAllowedUserIds,
  validateJiraBaseUrl,
  validateApiToken,
  validateEmail,
  validateProjectKeys,
  validateBinaryPath,
  validateRepoPath,
} from "./validators"

function cancel<T>(value: T): T {
  if (isCancel(value)) throw new FriendlyError("Setup cancelled.")
  return value
}

async function checkGhCli(): Promise<void> {
  const s = spinner()
  s.start("Checking GitHub CLI (gh)...")

  if (!Bun.which("gh")) {
    s.stop("gh not found — install from https://cli.github.com (optional)", 1)
    return
  }

  try {
    const proc = Bun.spawn(["gh", "auth", "status"], { stdout: "pipe", stderr: "pipe" })
    if (await proc.exited !== 0) {
      s.stop("gh found but not authenticated — run `gh auth login`", 1)
    } else {
      s.stop("gh authenticated", 0)
    }
  } catch {
    s.stop("gh check failed — skipping", 1)
  }
}

async function tryFetchProjects(
  baseUrl: string,
  apiToken: string,
  email: string,
): Promise<Array<{ key: string; name: string }> | null> {
  try {
    const jira = new JiraClient(
      { host: new URL(baseUrl).host, email, apiToken, projectKeys: [] },
      { info: () => {}, error: () => {} },
    )
    return await jira.getProjects()
  } catch {
    return null
  }
}

export async function runWizard(
  existing?: AppConfig,
  fetchProjectsFn: (baseUrl: string, apiToken: string, email: string) => Promise<Array<{ key: string; name: string }> | null> = tryFetchProjects,
): Promise<AppConfig> {
  if (!process.stdin.isTTY) {
    throw new FriendlyError(
      "Cannot run wizard in non-interactive mode.",
      "Attach a TTY or provide config manually."
    )
  }

  intro("DevM8 setup")

  await checkGhCli()

  const bot_token = cancel(await text({
    message: "Telegram bot token",
    initialValue: existing?.telegram.bot_token,
    validate: validateBotToken,
  }))

  const allowed_user_ids = cancel(await text({
    message: "Your Telegram user ID(s), comma-separated (send /start to @userinfobot to get yours)",
    initialValue: existing?.telegram.allowed_user_ids?.join(", ") ?? "",
    validate: validateAllowedUserIds,
  }))

  const base_url = cancel(await text({
    message: "Jira base URL (e.g. https://mycompany.atlassian.net)",
    initialValue: existing?.jira.base_url,
    validate: validateJiraBaseUrl,
  }))

  const api_token = cancel(await text({
    message: "Jira API token",
    initialValue: existing?.jira.api_token,
    validate: validateApiToken,
  }))

  const email = cancel(await text({
    message: "Jira account email",
    initialValue: existing?.jira.email,
    validate: validateEmail,
  }))

  let project_keys: string[]
  const fetchResult = await fetchProjectsFn(base_url as string, api_token as string, email as string)
  if (fetchResult && fetchResult.length > 0) {
    const initialKeys = existing?.jira.project_keys ?? []
    const selected = cancel(await multiselect({
      message: "Select Jira projects to track",
      options: fetchResult.map(p => ({ value: p.key, label: `${p.key} — ${p.name}` })),
      initialValues: initialKeys.filter(k => fetchResult.some(p => p.key === k)),
      required: true,
    }))
    project_keys = selected as string[]
  } else {
    const raw = cancel(await text({
      message: "Jira project keys, comma-separated (e.g. MP,BZ)",
      initialValue: existing?.jira.project_keys?.join(", ") ?? "",
      validate: validateProjectKeys,
    }))
    project_keys = (raw as string).split(",").map(s => s.trim().toUpperCase()).filter(Boolean)
  }

  const binary_path = cancel(await text({
    message: "Path to claude binary",
    initialValue: existing?.claude.binary_path ?? (Bun.which("claude") ?? ""),
    validate: validateBinaryPath,
  }))

  const api_key = cancel(await text({
    message: "Anthropic API key (leave blank if claude is already logged in via `claude login`)",
    initialValue: existing?.claude.api_key ?? "",
  }))

  const repo_path = cancel(await text({
    message: "Local git repository path for ticket implementation (leave blank to skip)",
    initialValue: existing?.repo?.path ?? "",
    validate: validateRepoPath,
  }))

  outro("Setup complete!")

  const allowedUserIds = (allowed_user_ids as string)
    .split(",")
    .map(s => parseInt(s.trim(), 10))
    .filter(n => !isNaN(n) && n > 0)

  const repoPathTrimmed = (repo_path as string).trim()

  return {
    telegram: { bot_token: bot_token as string, allowed_user_ids: allowedUserIds },
    jira: {
      base_url: base_url as string,
      api_token: api_token as string,
      email: email as string,
      project_keys,
    },
    claude: {
      binary_path: binary_path as string,
      ...((api_key as string).trim() ? { api_key: (api_key as string).trim() } : {}),
    },
    ...(repoPathTrimmed ? { repo: { path: repoPathTrimmed } } : {}),
    app: { log_level: existing?.app.log_level ?? "info" },
  }
}
