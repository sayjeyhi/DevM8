import { realpathSync } from "node:fs"
import { mkdir, access, constants } from "node:fs/promises"
import { defineCommand } from "citty"
import { PATHS } from "../shared/paths"
import { FriendlyError, ConfigMissingError } from "../shared/errors"
import { loadConfig, writeConfig } from "../config/loader"
import type { AppConfig } from "../config/schema"
import { runWizard } from "../config/wizard"
import { agentStatus, writePlist, loadAgent } from "../daemon/launchd"
import { stopCommand } from "./stop"
import { appendToLogFile } from "../logger/index"

async function preflight(): Promise<void> {
  if (process.platform !== "darwin") {
    throw new FriendlyError(
      "DevM8 requires macOS",
      "This tool uses launchd, which is only available on macOS."
    )
  }
  await mkdir(PATHS.launchAgentsDir, { recursive: true })
}

async function resolveConfig(): Promise<AppConfig> {
  try {
    return await loadConfig()
  } catch (err) {
    if (err instanceof ConfigMissingError) {
      process.stdout.write("No config found — starting setup...\n\n")
      const result = await runWizard()
      await writeConfig(result)
      process.stdout.write("\n")
      return result
    }
    throw err
  }
}

export async function startCommand(): Promise<void> {
  await preflight()

  const config = await resolveConfig()

  try {
    await access(config.claude.binary_path, constants.X_OK)
  } catch {
    throw new FriendlyError(
      `Claude binary not executable at ${config.claude.binary_path}`,
      "Run `which claude` to find the correct path, then update with `devm8 config`."
    )
  }

  const status = await agentStatus()
  if (status.running) {
    process.stdout.write("Daemon already running; stopping first...\n")
    await stopCommand()
  }

  await writePlist(realpathSync(process.execPath))
  await loadAgent()

  const deadline = Date.now() + 5000
  while (Date.now() < deadline) {
    const s = await agentStatus()
    if (s.running) {
      appendToLogFile(PATHS.logFile, "info", "service started", { pid: s.pid, via: "devm8 start" })
      process.stdout.write(`devm8 started (PID ${s.pid})\n`)
      return
    }
    await Bun.sleep(200)
  }

  const finalStatus = await agentStatus()
  process.stderr.write(
    `devm8 failed to start. Last exit code: ${finalStatus.exitCode ?? "unknown"}\n` +
    `Hint: check \`devm8 status\` or ${PATHS.logFile}\n`
  )
  process.exit(1)
}

export default defineCommand({
  meta: { name: "start", description: "Start the DevM8 daemon" },
  async run() {
    await startCommand()
  },
})
