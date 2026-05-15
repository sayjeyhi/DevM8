export interface Config {
  telegramBotToken: string
  jiraBaseUrl: string
  jiraProjectKey: string
  jiraUserEmail: string
  jiraApiToken: string
  claudeApiKey: string
  allowedUserIds: Set<number>
}

export function loadConfig(): Config {
  function required(key: string): string {
    const val = process.env[key]
    if (val === undefined || val === "") {
      throw new Error(`Missing required environment variable: ${key}`)
    }
    return val
  }

  const allowedUserIdsEnv = process.env["ALLOWED_USER_IDS"]
  if (allowedUserIdsEnv === undefined) {
    throw new Error("Missing required environment variable: ALLOWED_USER_IDS")
  }

  // ALLOWED_USER_IDS may be empty string — this produces an empty Set (bot starts but
  // no user is authorized). This is intentional per spec: misconfigured = unusable, not crashed.
  const allowedUserIds = new Set(
    allowedUserIdsEnv
      .split(",")
      .map(s => s.trim())
      // Must filter empty strings BEFORE Number(): Number("") === 0, not NaN,
      // which would silently grant user ID 0 authorization.
      .filter(s => s !== "")
      .map(Number)
      .filter(n => !isNaN(n)),
  )

  return {
    telegramBotToken: required("TELEGRAM_BOT_TOKEN"),
    jiraBaseUrl: required("JIRA_BASE_URL"),
    jiraProjectKey: required("JIRA_PROJECT_KEY"),
    jiraUserEmail: required("JIRA_USER_EMAIL"),
    jiraApiToken: required("JIRA_API_TOKEN"),
    claudeApiKey: required("CLAUDE_API_KEY"),
    allowedUserIds,
  }
}
