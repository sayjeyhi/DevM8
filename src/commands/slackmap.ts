import { defineCommand } from "citty"
import { intro, outro, text, spinner, isCancel } from "@clack/prompts"
import { loadConfig, writeConfig } from "../config/loader"
import { ConfigMissingError, FriendlyError } from "../shared/errors"
import { SlackClient } from "../slack/SlackClient"

function cancel<T>(value: T): T {
  if (isCancel(value)) throw new FriendlyError("Setup cancelled.")
  return value
}

function toValidate(fn: (v: string) => string | undefined) {
  return (v: string | undefined) => fn(v ?? "")
}

async function runSlackWizard(existing?: { user_token: string; poll_interval_ms?: number }) {
  if (!process.stdin.isTTY) {
    throw new FriendlyError("Cannot run wizard in non-interactive mode.", "Attach a TTY.")
  }

  intro("Slack bridge setup")

  process.stdout.write(`
To get a Slack User Token:
  1. Go to https://api.slack.com/apps → Create New App (From scratch)
  2. OAuth & Permissions → User Token Scopes, add:
       im:history  im:read  chat:write
       channels:history  groups:history  mpim:history  users:read
  3. Install App to Workspace
  4. Copy the User OAuth Token (starts with xoxp-)

`)

  const user_token = cancel(await text({
    message: "Slack User OAuth Token (xoxp-...)",
    initialValue: existing?.user_token ?? "",
    validate: toValidate(v => {
      if (!v) return "Required"
      if (!v.startsWith("xoxp-")) return "Must start with xoxp-"
    }),
  }))

  const s = spinner()
  s.start("Validating token…")

  let workspace: string
  try {
    const client = new SlackClient(user_token as string)
    const result = await client.authTest()
    workspace = result.team
    s.stop(`✓ Connected to ${result.team} as ${result.user}`)
  } catch (err) {
    s.stop("✗ Token validation failed")
    throw new FriendlyError(`Invalid Slack token: ${(err as Error).message}`)
  }

  const pollRaw = cancel(await text({
    message: "Poll interval in seconds (default: 30, min: 5)",
    initialValue: String(Math.round((existing?.poll_interval_ms ?? 30000) / 1000)),
    validate: toValidate(v => {
      const n = parseInt(v, 10)
      if (isNaN(n) || n < 5) return "Must be at least 5 seconds"
    }),
  }))

  outro(`Slack bridge configured for workspace: ${workspace}`)

  return {
    user_token: user_token as string,
    poll_interval_ms: parseInt(pollRaw as string, 10) * 1000,
  }
}

export async function slackmapCommand(): Promise<void> {
  let config
  try {
    config = await loadConfig()
  } catch (err) {
    if (err instanceof ConfigMissingError) {
      process.stderr.write("No devm8 config found — run `devm8 config` first.\n")
      process.exit(1)
    }
    throw err
  }

  if (config.slack) {
    const s = spinner()
    s.start("Checking existing Slack connection…")
    try {
      const client = new SlackClient(config.slack.user_token)
      const result = await client.authTest()
      s.stop(`✓ Currently connected to ${result.team} as ${result.user}`)
    } catch {
      s.stop("✗ Existing Slack token invalid — reconfiguring")
    }
    process.stdout.write("\n")
  }

  const slackConfig = await runSlackWizard(config.slack)
  await writeConfig({ ...config, slack: slackConfig })

  process.stdout.write("\n✅ Slack bridge configured.\n")
  process.stdout.write("   Restart the daemon to apply: devm8 stop && devm8 start\n")
}

export default defineCommand({
  meta: { name: "slackmap", description: "Configure Slack → Telegram message bridge" },
  async run() {
    await slackmapCommand()
  },
})
