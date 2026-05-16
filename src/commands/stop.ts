import { defineCommand } from "citty"
import { unloadAgent } from "../daemon/launchd"
import { removePid } from "../daemon/pid"
import { FriendlyError, LaunchctlError } from "../shared/errors"

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
  process.stdout.write("jira-assistant stopped\n")
}

export default defineCommand({
  meta: { name: "stop", description: "Stop the jira-assistant daemon" },
  async run() {
    await stopCommand()
  },
})
