import { realpathSync } from "node:fs"
import { mkdir, access, constants } from "node:fs/promises"
import { defineCommand } from "citty"
import { PATHS } from "../shared/paths"
import { FriendlyError } from "../shared/errors"
import { loadConfig, configExists, writeConfig } from "../config/loader"
import { runWizard } from "../config/wizard"
import { agentStatus, writePlist, loadAgent } from "../daemon/launchd"
import { stopCommand } from "./stop"

async function preflight(): Promise<void> {
  if (process.platform !== "darwin") {
    throw new FriendlyError(
      "jira-assistant requires macOS",
      "This tool uses launchd, which is only available on macOS."
    )
  }

  await mkdir(PATHS.launchAgentsDir, { recursive: true })

  let config
  try {
    config = await loadConfig()
  } catch (err) {
    if (err instanceof FriendlyError) return  // config not yet created
    throw err
  }

  try {
    await access(config.claude.binary_path, constants.X_OK)
  } catch {
    throw new FriendlyError(
      `Claude binary not executable at ${config.claude.binary_path}`,
      "Run `which claude` to find the correct path, then update with `jira-assistant config`."
    )
  }
}

export async function startCommand(): Promise<void> {
  await preflight()

  if (!(await configExists())) {
    const result = await runWizard()
    await writeConfig(result)
  }

  const status = await agentStatus()
  if (status.running) {
    process.stdout.write("Daemon already running; stopping first...\n")
    await stopCommand()
  }

  await writePlist(realpathSync(Bun.argv[0]))
  await loadAgent()

  const deadline = Date.now() + 5000
  while (Date.now() < deadline) {
    const s = await agentStatus()
    if (s.running) {
      process.stdout.write(`jira-assistant started (PID ${s.pid})\n`)
      return
    }
    await Bun.sleep(200)
  }

  const finalStatus = await agentStatus()
  process.stderr.write(
    `jira-assistant failed to start. Last exit code: ${finalStatus.exitCode ?? "unknown"}\n` +
    `Hint: check \`jira-assistant status\` or ${PATHS.logFile}\n`
  )
  process.exit(1)
}

export default defineCommand({
  meta: { name: "start", description: "Start the jira-assistant daemon" },
  async run() {
    await startCommand()
  },
})
