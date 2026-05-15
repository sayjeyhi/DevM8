import { Bot } from "grammy"
import { apiThrottler } from "@grammyjs/transformer-throttler"
import { autoRetry } from "@grammyjs/auto-retry"
import { loadConfig } from "./config"
import { createAuthMiddleware } from "./middleware/auth"
import { registerCommands } from "./commands/index"
import { JiraClient } from "../jira/JiraClient"
import { ClaudeClient } from "../claude/ClaudeClient"

export async function startBot(): Promise<void> {
  const config = loadConfig()

  // Make API key available to the claude CLI subprocess (read from process.env on spawn)
  process.env.ANTHROPIC_API_KEY = config.claudeApiKey

  const jiraHost = new URL(config.jiraBaseUrl).host

  const logger = {
    info: (obj: object) => console.log(obj),
    error: (obj: object) => console.error(obj),
  }

  const jira = new JiraClient(
    {
      host: jiraHost,
      email: config.jiraUserEmail,
      apiToken: config.jiraApiToken,
      projectKey: config.jiraProjectKey,
    },
    logger,
  )

  const claude = new ClaudeClient(
    {
      binaryPath: process.env.CLAUDE_BINARY_PATH ?? "claude",
    },
    logger,
  )

  const bot = new Bot(config.telegramBotToken)

  // Rate-limit and retry transformers must be on bot.api (outbound calls), not on bot (inbound middleware)
  bot.api.config.use(apiThrottler())
  bot.api.config.use(autoRetry())

  bot.use(createAuthMiddleware(config.allowedUserIds))

  await registerCommands(bot, { jira, claude })

  // Unknown command fallback — only fire for unrecognized slash commands, not plain text
  bot.on("message:text", ctx => {
    if (ctx.message.text.startsWith("/")) {
      return ctx.reply("Unknown command. Try /help")
    }
  })

  bot.catch(err => {
    const error = err.error as Error & { type?: string }
    // Log sanitized error — never log full error object (may embed Authorization headers in cause)
    console.error({
      event: "error",
      command: err.ctx.message?.text?.split(" ")[0],
      errorMessage: error instanceof Error ? error.message : String(error),
      errorType: error.type ?? "unknown",
    })
    err.ctx.reply("An unexpected error occurred. Please try again.").catch(() => {})
  })

  // Graceful shutdown: clear in-flight polling before process exit to avoid duplicate delivery.
  // process.exit() ensures open handles (e.g. pending fetch connections) do not keep process alive.
  process.on("SIGTERM", async () => {
    await bot.stop()
    process.exit(0)
  })
  process.on("SIGINT", async () => {
    await bot.stop()
    process.exit(0)
  })

  await bot.start()
}

startBot().catch(console.error)
