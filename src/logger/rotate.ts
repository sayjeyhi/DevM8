import { rename, stat, unlink } from "fs/promises"

export async function rotateIfNeeded(
  logFile: string,
  maxBytes: number = 10 * 1024 * 1024,
  keepCount: number = 5
): Promise<void> {
  const info = await stat(logFile).catch(() => null)
  if (!info) return
  if (info.size < maxBytes) return

  // Shift rotated files: app.log.(keepCount-1) → app.log.keepCount, etc.
  for (let i = keepCount - 1; i >= 1; i--) {
    const src = `${logFile}.${i}`
    if (await stat(src).catch(() => null)) {
      await rename(src, `${logFile}.${i + 1}`)
    }
  }

  // Remove overflow if it somehow exists (edge case safety)
  await unlink(`${logFile}.${keepCount + 1}`).catch(() => undefined)

  // Rotate active log → app.log.1
  await rename(logFile, `${logFile}.1`)

  // Create fresh empty log file
  await Bun.write(logFile, "")
}
