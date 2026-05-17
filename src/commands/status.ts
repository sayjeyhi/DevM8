import { homedir } from "os"
import { defineCommand } from "citty"
import { agentStatus } from "../daemon/launchd"
import { readPid, isProcessRunning } from "../daemon/pid"
import { loadConfig } from "../config/loader"
import { PATHS } from "../shared/paths"

function fmtPath(p: string): string {
  return p.replace(homedir(), "~")
}

function fmtUptime(ms: number): string {
  const totalSecs = Math.floor(ms / 1000)
  const h = Math.floor(totalSecs / 3600)
  const m = Math.floor((totalSecs % 3600) / 60)
  return h > 0 ? `${h}h ${m}m` : `${m}m`
}

export async function statusCommand(): Promise<void> {
  const [launchd, pid] = await Promise.all([agentStatus(), readPid()])

  const devMode = !launchd.running && pid !== null && await isProcessRunning(pid)
  const running = launchd.running || devMode
  const activePid = launchd.pid ?? (devMode ? pid : null)

  let config = null
  try {
    config = await loadConfig()
  } catch {}

  let uptime: string | undefined
  if (running) {
    try {
      const stat = await Bun.file(PATHS.pidFile).stat()
      if (stat) uptime = fmtUptime(Date.now() - stat.mtimeMs)
    } catch {}
  }

  const state = running ? (devMode ? "running (dev)" : "running") : "stopped"
  const lines: string[] = ["devm8 status"]
  lines.push(`  State:       ${state}`)
  if (running && activePid !== null) lines.push(`  PID:         ${activePid}`)
  if (running && uptime) lines.push(`  Uptime:      ${uptime}`)
  lines.push(`  Config:      ${fmtPath(PATHS.configFile)}`)
  if (config) {
    lines.push(`  Jira URL:    ${config.jira.base_url}`)
    lines.push(`  Project:     ${config.jira.project_key}`)
  }
  lines.push(`  Log:         ${fmtPath(PATHS.logFile)}`)

  process.stdout.write(lines.join("\n") + "\n")
}

export default defineCommand({
  meta: { name: "status", description: "Show DevM8 daemon status" },
  async run() {
    await statusCommand()
  },
})
