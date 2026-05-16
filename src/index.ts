import { defineCommand, runMain } from "citty"
import { FriendlyError } from "./shared/errors"
import type { Logger } from "./logger/index"
import type { AppConfig } from "./config/schema"
import { startBotFromConfig } from "./bot/bot"

declare const __VERSION__: string

export async function startPolling(signal: AbortSignal, logger?: Logger, config?: AppConfig): Promise<void> {
  if (!config) {
    logger?.warn("no config provided — bot not starting")
    await new Promise<void>(resolve => signal.addEventListener("abort", resolve, { once: true }))
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
    name: "devmate",
    version: appVersion,
    description: "Manage your DevMate Telegram bot daemon",
  },
  subCommands: {
    start:  () => import("./commands/start").then(m => m.default),
    stop:   () => import("./commands/stop").then(m => m.default),
    status: () => import("./commands/status").then(m => m.default),
    config: () => import("./commands/config").then(m => m.default),
    update: () => import("./commands/update").then(m => m.default),
    logs:   () => import("./commands/logs").then(m => m.default),
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
