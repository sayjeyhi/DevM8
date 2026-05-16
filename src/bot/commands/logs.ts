import type { Context } from "grammy"
import { existsSync, readFileSync } from "node:fs"
import { PATHS } from "../../shared/paths"
import { splitMessage } from "../utils/splitMessage"

const DEFAULT_LINES = 50
const MAX_LINES = 200

function formatLine(raw: string): string {
  try {
    const { level, ts, msg, ...meta } = JSON.parse(raw) as {
      level: string; ts: string; msg: string; [k: string]: unknown
    }
    const time = new Date(ts).toLocaleTimeString("en-GB", { hour12: false })
    const label = level.toUpperCase().padEnd(5)
    const metaStr = Object.keys(meta).length > 0 ? " " + JSON.stringify(meta) : ""
    return `${time} [${label}] ${msg}${metaStr}`
  } catch {
    return raw
  }
}

export async function handleLogs(ctx: Context): Promise<void> {
  const arg = ctx.message?.text?.split(/\s+/)[1]
  const n = Math.min(MAX_LINES, Math.max(1, parseInt(arg ?? "", 10) || DEFAULT_LINES))

  if (!existsSync(PATHS.logFile)) {
    await ctx.reply("No log file found. Start the daemon first.")
    return
  }

  const lines = readFileSync(PATHS.logFile, "utf8").split("\n").filter(Boolean)
  const recent = lines.slice(-n)

  if (recent.length === 0) {
    await ctx.reply("Log file is empty.")
    return
  }

  const body = recent.map(formatLine).join("\n")
  const header = `Last ${recent.length} log lines:\n\n`

  for (const chunk of splitMessage(header + body)) {
    await ctx.reply(chunk)
  }
}
