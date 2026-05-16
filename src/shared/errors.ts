export class FriendlyError extends Error {
  readonly hint?: string
  constructor(message: string, hint?: string) {
    super(message)
    this.name = "FriendlyError"
    this.hint = hint
  }
}

export class LaunchctlError extends FriendlyError {
  readonly rawOutput: string
  constructor(stderr: string, hint: string) {
    super("launchctl invocation failed", hint)
    this.name = "LaunchctlError"
    this.rawOutput = stderr
  }
}

export function launchctlHint(stderr: string): string {
  const s = stderr.toLowerCase()
  if (s.includes("no such file or directory")) {
    return "Make sure you ran `jira-assistant start` first"
  }
  if (s.includes("operation already in progress")) {
    return "Daemon may already be running; check `jira-assistant status`"
  }
  if (s.includes("permission denied")) {
    return "Check file permissions on the plist"
  }
  return "launchctl exited with a non-zero status"
}

// ─── Jira errors ─────────────────────────────────────────────────────────────

export class JiraAuthError extends Error {
  readonly type = 'JIRA_AUTH' as const
  constructor(message = 'Jira authentication failed') {
    super(message)
    this.name = 'JiraAuthError'
  }
}

export class JiraPermissionError extends Error {
  readonly type = 'JIRA_PERMISSION' as const
  constructor(message = 'Jira permission denied') {
    super(message)
    this.name = 'JiraPermissionError'
  }
}

export class JiraNotFoundError extends Error {
  readonly type = 'JIRA_NOT_FOUND' as const
  readonly issueKey: string
  constructor(issueKey: string, message?: string) {
    super(message ?? `Issue ${issueKey} not found`)
    this.name = 'JiraNotFoundError'
    this.issueKey = issueKey
  }
}

export class JiraRateLimitError extends Error {
  readonly type = 'JIRA_RATE_LIMIT' as const
  readonly retryAfter?: number
  constructor(retryAfter?: number, message = 'Jira rate limit exceeded') {
    super(message)
    this.name = 'JiraRateLimitError'
    this.retryAfter = retryAfter
  }
}

export class JiraServerError extends Error {
  readonly type = 'JIRA_SERVER' as const
  readonly status: number
  constructor(status: number, message?: string) {
    super(message ?? `Jira server error: ${status}`)
    this.name = 'JiraServerError'
    this.status = status
  }
}

export class JiraTimeoutError extends Error {
  readonly type = 'JIRA_TIMEOUT' as const
  constructor(message = 'Jira request timed out') {
    super(message)
    this.name = 'JiraTimeoutError'
  }
}

export class InvalidTransitionError extends Error {
  readonly type = 'INVALID_TRANSITION' as const
  readonly attempted: string
  readonly available: string[]
  constructor(attempted: string, available: string[], message?: string) {
    super(message ?? `Invalid transition '${attempted}'. Available: ${available.join(', ')}`)
    this.name = 'InvalidTransitionError'
    this.attempted = attempted
    this.available = available
  }
}

// ─── Claude errors ────────────────────────────────────────────────────────────

export class ClaudeTimeoutError extends Error {
  readonly type = 'CLAUDE_TIMEOUT' as const
  readonly timeoutMs: number
  constructor(timeoutMs: number, message?: string) {
    super(message ?? `Claude timed out after ${timeoutMs}ms`)
    this.name = 'ClaudeTimeoutError'
    this.timeoutMs = timeoutMs
  }
}

export class ClaudeExitError extends Error {
  readonly type = 'CLAUDE_EXIT' as const
  readonly exitCode: number
  readonly stderr: string
  constructor(exitCode: number, stderr: string, message?: string) {
    super(message ?? `Claude exited with code ${exitCode}`)
    this.name = 'ClaudeExitError'
    this.exitCode = exitCode
    this.stderr = stderr
  }
}
