import { defineCommand } from "citty"
import { rename, chmod, unlink } from "node:fs/promises"
import { readFileSync } from "node:fs"
import { execSync } from "node:child_process"
import { createHash } from "node:crypto"
import { FriendlyError } from "../shared/errors"
import { agentStatus } from "../daemon/launchd"
import { stopCommand } from "./stop"
import { startCommand } from "./start"

declare const __VERSION__: string

const REPO = "sayjeyhi/DevM8"

function currentVersion(): string {
  return typeof __VERSION__ !== "undefined" ? __VERSION__ : "0.0.0-dev"
}

async function fetchLatestVersion(): Promise<string> {
  const resp = await fetch(`https://github.com/${REPO}/releases/latest`, { redirect: "follow" })
  const tag = resp.url.split("/").pop() ?? ""
  if (!tag.startsWith("v")) throw new FriendlyError("Could not determine latest version from GitHub")
  return tag
}

function binaryName(): string {
  const { platform, arch } = process
  if (platform === "darwin" && arch === "arm64") return "devm8-macos-arm64"
  if (platform === "darwin" && arch === "x64")   return "devm8-macos-x64"
  if (platform === "linux"  && arch === "x64")   return "devm8-linux-x64"
  throw new FriendlyError(
    `Unsupported platform: ${platform}/${arch}`,
    "Pre-built binaries are available for macOS arm64/x64 and Linux x64 only."
  )
}

function semverGt(a: string, b: string): boolean {
  const parse = (v: string) => v.replace(/^v/, "").split(".").map(n => parseInt(n, 10) || 0)
  const [a0, a1, a2] = parse(a)
  const [b0, b1, b2] = parse(b)
  if (a0 !== b0) return a0 > b0
  if (a1 !== b1) return a1 > b1
  return a2 > b2
}

async function downloadTo(url: string, dest: string): Promise<void> {
  const resp = await fetch(url)
  if (!resp.ok) throw new FriendlyError(`Download failed (HTTP ${resp.status})`, url)
  await Bun.write(dest, await resp.arrayBuffer())
}

function verifyChecksum(binaryPath: string, checksumsPath: string, name: string): void {
  const text = readFileSync(checksumsPath, "utf8")
  const line = text.split("\n").find(l => l.trimEnd().endsWith(`  ${name}`))
  if (!line) throw new FriendlyError(`No checksum entry for ${name} in checksums.txt`)
  const expected = line.split(/\s+/)[0]
  const actual = createHash("sha256").update(readFileSync(binaryPath)).digest("hex")
  if (actual !== expected) throw new FriendlyError("Checksum mismatch — download may be corrupted. Please retry.")
}

export default defineCommand({
  meta: { name: "update", description: "Update DevM8 to the latest release" },
  async run() {
    const current = currentVersion()
    if (current === "0.0.0-dev") {
      process.stdout.write("Running in dev mode — skipping update.\n")
      return
    }

    process.stdout.write(`Current version: ${current}\n`)
    process.stdout.write("Checking for updates...\n")

    const latest = await fetchLatestVersion()
    process.stdout.write(`Latest version:  ${latest}\n`)

    if (!semverGt(latest, current)) {
      process.stdout.write("Already up to date.\n")
      return
    }

    const name = binaryName()
    const base = `https://github.com/${REPO}/releases/download/${latest}`
    const dest = process.execPath
    const tmpBin = dest + ".new"
    const tmpChecksums = dest + ".checksums"

    try {
      process.stdout.write(`Downloading ${name} ${latest}...\n`)
      await downloadTo(`${base}/${name}`, tmpBin)
      await downloadTo(`${base}/checksums.txt`, tmpChecksums)
      verifyChecksum(tmpBin, tmpChecksums, name)
      await chmod(tmpBin, 0o755)

      if (process.platform === "darwin") {
        try { execSync(`xattr -d com.apple.quarantine "${tmpBin}"`, { stdio: "ignore" }) } catch {}
      }

      let wasRunning = false
      if (process.platform === "darwin") {
        wasRunning = (await agentStatus()).running
        if (wasRunning) {
          process.stdout.write("Stopping service...\n")
          await stopCommand()
        }
      }

      await rename(tmpBin, dest)
      process.stdout.write(`Updated to ${latest}.\n`)

      if (wasRunning) {
        process.stdout.write("Restarting service...\n")
        await startCommand()
      }
    } finally {
      await unlink(tmpBin).catch(() => {})
      await unlink(tmpChecksums).catch(() => {})
    }
  },
})
