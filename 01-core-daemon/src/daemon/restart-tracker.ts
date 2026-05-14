import { join, dirname } from "path"
import { rename } from "fs/promises"

export class RestartTracker {
  private readonly filePath: string
  private readonly maxRestarts: number
  private readonly windowMs: number

  constructor(filePath: string, maxRestarts: number = 10, windowMs: number = 60_000) {
    this.filePath = filePath
    this.maxRestarts = maxRestarts
    this.windowMs = windowMs
  }

  async recordRestart(): Promise<boolean> {
    const timestamps = await this.read()
    const now = Date.now()
    timestamps.push(now)
    const pruned = timestamps.filter(t => now - t <= this.windowMs)
    await this.write(pruned)
    return pruned.length >= this.maxRestarts
  }

  async reset(): Promise<void> {
    await this.write([])
  }

  private async read(): Promise<number[]> {
    try {
      const text = await Bun.file(this.filePath).text()
      const parsed = JSON.parse(text)
      if (Array.isArray(parsed)) return parsed as number[]
      return []
    } catch {
      return []
    }
  }

  private async write(timestamps: number[]): Promise<void> {
    const dir = dirname(this.filePath)
    const tmp = join(dir, `.restarts-tmp-${process.pid}-${Date.now()}`)
    await Bun.write(tmp, JSON.stringify(timestamps))
    await rename(tmp, this.filePath)
  }
}
