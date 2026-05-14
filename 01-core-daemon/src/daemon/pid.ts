import { join, dirname } from "path"
import { rename, unlink } from "fs/promises"
import { PATHS } from "../shared/paths"

export async function writePid(pid: number, filePath: string = PATHS.pidFile): Promise<void> {
  const dir = dirname(filePath)
  const tmp = join(dir, `.pid-tmp-${process.pid}-${Date.now()}`)
  await Bun.write(tmp, `${pid}\n`)
  await rename(tmp, filePath)
}

export async function readPid(filePath: string = PATHS.pidFile): Promise<number | null> {
  try {
    const text = await Bun.file(filePath).text()
    const n = parseInt(text.trim(), 10)
    return isNaN(n) ? null : n
  } catch {
    return null
  }
}

export async function removePid(filePath: string = PATHS.pidFile): Promise<void> {
  try {
    await unlink(filePath)
  } catch {}
}

export async function isProcessRunning(pid: number): Promise<boolean> {
  try {
    process.kill(pid, 0)
    return true
  } catch (err: any) {
    if (err?.code === "ESRCH") return false
    throw err
  }
}
