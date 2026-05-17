import { join, dirname } from "path"
import { rename } from "fs/promises"
import { PATHS } from "../shared/paths"
import { LaunchctlError, FriendlyError, launchctlHint } from "../shared/errors"
import { mkdirSync } from "node:fs"

export interface AgentStatus {
  running: boolean
  pid?: number
  exitCode?: number
}

function xmlEscape(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;")
}

const FORWARDED_ENV_KEYS = ["HOME", "PATH", "USER", "ANTHROPIC_API_KEY", "CLAUDE_CONFIG_DIR"]

export function generatePlist(binaryPath: string): string {
  try { mkdirSync(dirname(PATHS.logFile), { recursive: true }) } catch {}

  const envEntries = FORWARDED_ENV_KEYS
    .filter(k => process.env[k])
    .map(k => `        <key>${k}</key>\n        <string>${xmlEscape(process.env[k]!)}</string>`)
    .join("\n")

  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>net.devm8</string>
    <key>ProgramArguments</key>
    <array>
        <string>${xmlEscape(binaryPath)}</string>
        <string>daemon</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
${envEntries}
    </dict>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>RunAtLoad</key>
    <false/>
</dict>
</plist>
`
}

export async function writePlist(binaryPath: string, filePath: string = PATHS.plistFile): Promise<void> {
  const dir = dirname(filePath)
  const tmp = join(dir, `.plist-tmp-${process.pid}-${Date.now()}`)
  await Bun.write(tmp, generatePlist(binaryPath))
  await rename(tmp, filePath)
}

async function runLaunchctl(args: string[]): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  const proc = Bun.spawn(["launchctl", ...args], {
    stdout: "pipe",
    stderr: "pipe",
  })
  const exitCode = await proc.exited
  const stdout = await new Response(proc.stdout).text()
  const stderr = await new Response(proc.stderr).text()
  return { exitCode, stdout, stderr }
}

export async function loadAgent(): Promise<void> {
  const { exitCode, stderr } = await runLaunchctl(["load", "-w", PATHS.plistFile])
  if (exitCode !== 0) {
    throw new LaunchctlError(stderr, launchctlHint(stderr))
  }
}

export async function unloadAgent(): Promise<void> {
  const { exitCode, stderr } = await runLaunchctl(["unload", "-w", PATHS.plistFile])
  if (exitCode !== 0) {
    throw new LaunchctlError(stderr, launchctlHint(stderr))
  }
}

function parsePrintOutput(output: string): AgentStatus {
  const pidMatch = output.match(/\bpid\s*=\s*(\d+)/)
  const stateMatch = output.match(/\bstate\s*=\s*(\w+)/)
  const exitCodeMatch = output.match(/last exit code\s*=\s*(-?\d+)/)

  const pid = pidMatch ? parseInt(pidMatch[1], 10) : undefined
  const state = stateMatch ? stateMatch[1] : undefined
  const exitCode = exitCodeMatch ? parseInt(exitCodeMatch[1], 10) : undefined

  const running = state === "running" || (pid !== undefined && state !== "waiting" && state !== "stopped")
  return running ? { running: true, pid } : { running: false, exitCode }
}

function parseListOutput(output: string): AgentStatus {
  const lines = output.trim().split("\n")
  const line = lines.find(l => l.split("\t")[2]?.trim() === "net.devm8")
  if (!line) return { running: false }

  const [pidStr, exitCodeStr] = line.split("\t")
  const pid = pidStr && pidStr !== "-" ? parseInt(pidStr, 10) : undefined
  const exitCode = exitCodeStr ? parseInt(exitCodeStr, 10) : undefined

  if (pid !== undefined && !isNaN(pid)) {
    return { running: true, pid }
  }
  return { running: false, exitCode: exitCode !== undefined && !isNaN(exitCode) ? exitCode : undefined }
}

export async function agentStatus(): Promise<AgentStatus> {
  if (!process.getuid) {
    throw new FriendlyError("agentStatus requires a POSIX environment", "Run on macOS")
  }
  const uid = process.getuid()

  const { exitCode: printExit, stdout: printOut } = await runLaunchctl([
    "print",
    `gui/${uid}/net.devm8`,
  ])

  if (printExit === 0) {
    return parsePrintOutput(printOut)
  }

  const { exitCode: listExit, stdout: listOut } = await runLaunchctl([
    "list",
    "net.devm8",
  ])

  if (listExit !== 0) {
    return { running: false }
  }

  return parseListOutput(listOut)
}
