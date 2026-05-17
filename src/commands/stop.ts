import { defineCommand } from "citty"
import { unloadAgent } from "../daemon/launchd"
import { removePid } from "../daemon/pid"
import { FriendlyError, LaunchctlError } from "../shared/errors"
import { appendToLogFile } from "../logger/index"
import { PATHS } from "../shared/paths"

export async function stopCommand(): Promise<void> {
  try {
    await unloadAgent()
  } catch (err) {
    if (err instanceof LaunchctlError) {
      throw new FriendlyError(`Failed to stop daemon: ${err.message}`, err.hint)
    }
    throw err
  }
  await removePid()
  appendToLogFile(PATHS.logFile, "info", "service stopped", { via: "devm8 stop" })
  process.stdout.write("devm8 stopped\n")
}

export default defineCommand({
  meta: { name: "stop", description: "Stop the DevM8 daemon" },
  async run() {
    await stopCommand()
  },
})
