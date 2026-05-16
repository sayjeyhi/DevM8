import { defineCommand } from "citty"
import { PATHS } from "../shared/paths"
import {
  existsSync,
  readFileSync,
  watchFile,
  unwatchFile,
  openSync,
  readSync,
  closeSync,
  fstatSync,
} from "node:fs"
import { homedir } from "node:os"

const ANSI = {
  reset: "\x1b[0m",
  red: "\x1b[31m",
  yellow: "\x1b[33m",
  dim: "\x1b[2m",
  cyan: "\x1b[36m",
} as const

function formatLine(raw: string): string {
  try {
    const { level, ts, msg, ...meta } = JSON.parse(raw) as {
      level: string
      ts: string
      msg: string
      [k: string]: unknown
    }
    const time = new Date(ts).toLocaleTimeString("en-GB", { hour12: false })
    const label = level.toUpperCase().padEnd(5)
    const metaStr =
      Object.keys(meta).length > 0 ? "  " + JSON.stringify(meta) : ""

    if (process.stdout.isTTY) {
      const colors: Record<string, string> = {
        error: ANSI.red,
        warn: ANSI.yellow,
        debug: ANSI.dim,
        info: ANSI.cyan,
      }
      const color = colors[level] ?? ""
      const reset = color ? ANSI.reset : ""
      return `${ANSI.dim}${time}${ANSI.reset} ${color}[${label}]${reset} ${msg}${ANSI.dim}${metaStr}${ANSI.reset}`
    }
    return `${time} [${label}] ${msg}${metaStr}`
  } catch {
    return raw
  }
}

export default defineCommand({
  meta: { name: "logs", description: "Show DevMate daemon logs" },
  args: {
    tail: {
      type: "string",
      description: "Number of lines to show (default: 100)",
      default: "100",
      alias: "n",
    },
    follow: {
      type: "boolean",
      description: "Follow log output (like tail -f) — enabled by default, use --no-follow to print and exit",
      default: true,
      alias: "f",
    },
  },
  async run({ args }) {
    const logFile = PATHS.logFile
    const displayPath = logFile.replace(homedir(), "~")

    if (!existsSync(logFile)) {
      process.stdout.write(`No log file at ${displayPath}\n`)
      process.stdout.write("Start the daemon first: devmate start\n")
      return
    }

    const n = Math.max(1, parseInt(String(args.tail), 10) || 100)
    const content = readFileSync(logFile, "utf8")
    const lines = content.split("\n").filter(Boolean)
    const toShow = lines.slice(-n)

    if (process.stdout.isTTY) {
      process.stdout.write(`${ANSI.dim}==> ${displayPath} <==${ANSI.reset}\n`)
    }
    for (const line of toShow) {
      process.stdout.write(formatLine(line) + "\n")
    }

    if (!args.follow) return

    let offset = Buffer.byteLength(content, "utf8")

    function readNewContent() {
      let fd: number | undefined
      try {
        fd = openSync(logFile, "r")
        const { size } = fstatSync(fd)
        if (size < offset) offset = 0
        if (size <= offset) return
        const buf = Buffer.alloc(size - offset)
        readSync(fd, buf, 0, buf.length, offset)
        offset = size
        const newLines = buf.toString("utf8").split("\n").filter(Boolean)
        for (const line of newLines) {
          process.stdout.write(formatLine(line) + "\n")
        }
      } catch {}
      finally {
        if (fd !== undefined) closeSync(fd)
      }
    }

    watchFile(logFile, { interval: 300 }, (_curr, _prev) => readNewContent())

    const cleanup = () => {
      unwatchFile(logFile)
      process.exit(0)
    }
    process.on("SIGINT", cleanup)
    process.on("SIGTERM", cleanup)

    await new Promise<never>(() => {})
  },
})
