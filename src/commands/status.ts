import { homedir } from "os"
import { defineCommand } from "citty"
import { agentStatus } from "../daemon/launchd"
import { readPid } from "../daemon/pid"
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
  const [status, pid] = await Promise.all([agentStatus(), readPid()])

  let config = null
  try {
    config = await loadConfig()
  } catch {}

  let uptime: string | undefined
  if (status.running) {
    try {
      const stat = await Bun.file(PATHS.pidFile).stat()
      if (stat) uptime = fmtUptime(Date.now() - stat.mtimeMs)
    } catch {}
  }

  const lines: string[] = ["jira-assistant status"]
  lines.push(`  State:       ${status.running ? "running" : "stopped"}`)
  if (status.running && pid !== null) lines.push(`  PID:         ${pid}`)
  if (status.running && uptime) lines.push(`  Uptime:      ${uptime}`)
  lines.push(`  Config:      ${fmtPath(PATHS.configFile)}`)
  if (config) {
    lines.push(`  Jira URL:    ${config.jira.base_url}`)
    lines.push(`  Project:     ${config.jira.project_key}`)
  }
  lines.push(`  Log:         ${fmtPath(PATHS.logFile)}`)

  process.stdout.write(lines.join("\n") + "\n")
}

export default defineCommand({
  meta: { name: "status", description: "Show jira-assistant daemon status" },
  async run() {
    await statusCommand()
  },
})
