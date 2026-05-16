import type { Context } from "grammy"
import { existsSync, readFileSync } from "node:fs"
import { PATHS } from "../../shared/paths"

const DEFAULT_LINES = 50
const MAX_LINES = 200
const CHUNK_LIMIT = 3900 // leave room for <pre> tags within Telegram's 4096 char limit

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
}

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

function chunkLines(lines: string[]): string[] {
  const chunks: string[] = []
  let current = ""
  for (const line of lines) {
    const candidate = current ? `${current}\n${line}` : line
    if (candidate.length > CHUNK_LIMIT) {
      if (current) chunks.push(current)
      current = line
    } else {
      current = candidate
    }
  }
  if (current) chunks.push(current)
  return chunks
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

  const formatted = recent.map(formatLine)
  const chunks = chunkLines(formatted)
  const total = chunks.length

  for (let i = 0; i < chunks.length; i++) {
    const prefix = total > 1 ? `[${i + 1}/${total}] Last ${recent.length} log lines:\n` : `Last ${recent.length} log lines:\n`
    await ctx.reply(`${prefix}<pre>${escapeHtml(chunks[i])}</pre>`, { parse_mode: "HTML" })
  }
}
