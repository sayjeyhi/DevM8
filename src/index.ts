import { defineCommand, runMain } from "citty"
import { FriendlyError, ConfigMissingError } from "./shared/errors"
import type { Logger } from "./logger/index"
import type { AppConfig } from "./config/schema"
import { startBotFromConfig } from "./bot/bot"

declare const __VERSION__: string

export async function startPolling(signal: AbortSignal, logger?: Logger, config?: AppConfig): Promise<void> {
  if (!config) {
    logger?.warn("no config provided — bot not starting")
    await new Promise<void>(resolve => signal.addEventListener("abort", () => resolve(), { once: true }))
    return
  }
  await startBotFromConfig(config, signal, logger ?? {
    info: (msg, meta) => console.log("[INFO]", msg, meta ?? ""),
    warn: (msg, meta) => console.warn("[WARN]", msg, meta ?? ""),
    error: (msg, meta) => console.error("[ERROR]", msg, meta ?? ""),
    debug: (msg, meta) => console.debug("[DEBUG]", msg, meta ?? ""),
  })
}

const appVersion = typeof __VERSION__ !== "undefined" ? __VERSION__ : "0.0.0-dev"

const main = defineCommand({
  meta: {
    name: "devm8",
    version: appVersion,
    description: "Manage your DevM8 Telegram bot daemon",
  },
  subCommands: {
    start:  () => import("./commands/start").then(m => m.default),
    stop:   () => import("./commands/stop").then(m => m.default),
    status: () => import("./commands/status").then(m => m.default),
    config: () => import("./commands/config").then(m => m.default),
    update: () => import("./commands/update").then(m => m.default),
    logs:   () => import("./commands/logs").then(m => m.default),
    daemon:   () => import("./commands/daemon").then(m => m.default),
    slackmap: () => import("./commands/slackmap").then(m => m.default),
  },
})

runMain(main).catch(async err => {
  if (err instanceof ConfigMissingError) {
    process.stdout.write("\nNo config found — starting setup...\n\n")
    try {
      const { configCommand } = await import("./commands/config")
      await configCommand()
      process.stdout.write("\nRun your command again to continue.\n")
    } catch (wizardErr) {
      if (wizardErr instanceof FriendlyError) {
        process.stderr.write(`Setup error: ${wizardErr.message}\n`)
      }
      process.exit(1)
    }
    return
  }
  if (err instanceof FriendlyError) {
    process.stderr.write(`Error: ${err.message}\n`)
    if (err.hint) process.stderr.write(`Hint: ${err.hint}\n`)
    process.exit(1)
  }
  throw err
})
