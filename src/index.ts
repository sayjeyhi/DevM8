import { defineCommand, runMain } from "citty"
import { FriendlyError } from "./shared/errors"

declare const __VERSION__: string

export async function startPolling(signal: AbortSignal): Promise<void> {
  // placeholder — implemented in 02-integration-clients section
  await new Promise<void>(resolve => signal.addEventListener("abort", resolve, { once: true }))
}

const appVersion = typeof __VERSION__ !== "undefined" ? __VERSION__ : "0.0.0-dev"

const main = defineCommand({
  meta: {
    name: "jira-assistant",
    version: appVersion,
    description: "Manage your Jira assistant Telegram bot daemon",
  },
  subCommands: {
    start:  () => import("./commands/start").then(m => m.default),
    stop:   () => import("./commands/stop").then(m => m.default),
    status: () => import("./commands/status").then(m => m.default),
    config: () => import("./commands/config").then(m => m.default),
    daemon: () => import("./commands/daemon").then(m => m.default),
  },
})

runMain(main).catch(err => {
  if (err instanceof FriendlyError) {
    process.stderr.write(`Error: ${err.message}\n`)
    if (err.hint) process.stderr.write(`Hint: ${err.hint}\n`)
    process.exit(1)
  }
  throw err
})
