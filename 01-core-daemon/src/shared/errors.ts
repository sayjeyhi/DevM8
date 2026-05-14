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
  return "Run `jira-assistant status` for more info"
}
