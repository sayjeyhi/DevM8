import { defineCommand } from "citty"
import { createLogger } from "../logger/index"
import { rotateIfNeeded } from "../logger/rotate"
import { loadConfig } from "../config/loader"
import { writePid, removePid } from "../daemon/pid"
import { RestartTracker } from "../daemon/restart-tracker"
import { PATHS } from "../shared/paths"
import { FriendlyError } from "../shared/errors"
import { startPolling } from "../index"

export async function daemonCommand(): Promise<void> {
  let config
  try {
    config = await loadConfig()
  } catch (err) {
    if (err instanceof FriendlyError) {
      process.stderr.write(`${err.message}\n`)
      process.exit(1)
    }
    throw err
  }

  const logger = createLogger(config.app.log_level)
  const restartTracker = new RestartTracker(PATHS.restartsFile, 10, 60_000)

  await writePid(process.pid)

  const shutdownController = new AbortController()
  let pollingPromise: Promise<void> = Promise.resolve()

  await rotateIfNeeded(PATHS.logFile)
  const rotateInterval = setInterval(() => rotateIfNeeded(PATHS.logFile), 60 * 60 * 1000)

  process.on("SIGTERM", async () => {
    clearInterval(rotateInterval)
    shutdownController.abort()
    try { await pollingPromise } catch {}
    await removePid()
    logger.info("shutdown complete")
    process.exit(0)
  })

  try {
    pollingPromise = startPolling(shutdownController.signal)
    await pollingPromise
  } catch (err) {
    const limitExceeded = await restartTracker.recordRestart()
    if (limitExceeded) {
      logger.warn("restart limit exceeded, shutting down")
      process.exit(0)
    }
    process.exit(1)
  }
}

export default defineCommand({
  meta: { name: "daemon", description: "Run the daemon process (used by launchd)" },
  async run() {
    await daemonCommand()
  },
})
