import { Bot } from "grammy"
import { apiThrottler } from "@grammyjs/transformer-throttler"
import { autoRetry } from "@grammyjs/auto-retry"
import { loadConfig } from "./config"
import { createAuthMiddleware } from "./middleware/auth"
import { registerCommands } from "./commands/index"
import { JiraClient } from "../jira/JiraClient"
import { ClaudeClient } from "../claude/ClaudeClient"
import type { AppConfig } from "../config/schema"
import type { Logger } from "../logger/index"
import { keepTyping } from "./utils/typing"

export async function startBotFromConfig(
  config: AppConfig,
  signal: AbortSignal,
  logger: Logger,
): Promise<void> {
  const jiraLog = {
    info: (obj: object) => logger.info("jira", obj as Record<string, unknown>),
    error: (obj: object) => logger.error("jira error", obj as Record<string, unknown>),
  }
  const claudeLog = {
    info: (data: Record<string, unknown>) => logger.info("claude", data),
  }

  const jira = new JiraClient(
    {
      host: new URL(config.jira.base_url).host,
      email: config.jira.email,
      apiToken: config.jira.api_token,
      projectKeys: config.jira.project_keys,
    },
    jiraLog,
  )

  if (config.claude.api_key) {
    process.env.ANTHROPIC_API_KEY = config.claude.api_key
  }
  const claude = new ClaudeClient({ binaryPath: config.claude.binary_path }, claudeLog)

  logger.info("jira connecting", { host: new URL(config.jira.base_url).host, projects: config.jira.project_keys })
  try {
    const me = await jira.ping()
    logger.info("jira connected", { user: me.displayName, email: me.emailAddress })
  } catch (err) {
    logger.error("jira connection failed", { message: (err as Error).message })
    throw err
  }

  const bot = new Bot(config.telegram.bot_token)
  bot.api.config.use(apiThrottler())
  bot.api.config.use(autoRetry())

  const allowedIds = new Set(config.telegram.allowed_user_ids)
  bot.use(createAuthMiddleware(allowedIds, e => logger.warn("unauthorized", e)))

  await registerCommands(bot, { jira, claude })

  bot.on("message:text", async ctx => {
    if (ctx.message.text.startsWith("/")) {
      return ctx.reply("Unknown command. Try /help")
    }
    const stopTyping = keepTyping(ctx)
    try {
      const response = await claude.ask(ctx.message.text)
      await ctx.reply(response)
    } catch (err) {
      logger.error("claude reply error", { message: (err as Error).message })
      await ctx.reply("Failed to get a response from Claude. Please try again.")
    } finally {
      stopTyping()
    }
  })

  bot.catch(err => {
    const error = err.error as Error & { type?: string }
    logger.error("bot error", {
      command: err.ctx.message?.text?.split(" ")[0],
      message: error instanceof Error ? error.message : String(error),
      type: error.type ?? "unknown",
    })
    err.ctx.reply("An unexpected error occurred. Please try again.").catch(() => {})
  })

  signal.addEventListener("abort", () => { bot.stop().catch(() => {}) }, { once: true })

  await bot.start({
    onStart: ({ username }) => logger.info("telegram bot ready", { username, allowedUsers: allowedIds.size }),
  })

  logger.info("telegram polling stopped")
}

export async function startBot(): Promise<void> {
  const config = loadConfig()
  process.env.ANTHROPIC_API_KEY = config.claudeApiKey

  const jiraLog = {
    info: (obj: object) => console.log(obj),
    error: (obj: object) => console.error(obj),
  }
  const claudeLog = { info: (data: Record<string, unknown>) => console.log(data) }

  const jira = new JiraClient(
    {
      host: new URL(config.jiraBaseUrl).host,
      email: config.jiraUserEmail,
      apiToken: config.jiraApiToken,
      projectKeys: [config.jiraProjectKey],
    },
    jiraLog,
  )

  const claude = new ClaudeClient(
    { binaryPath: process.env.CLAUDE_BINARY_PATH ?? "claude" },
    claudeLog,
  )

  const bot = new Bot(config.telegramBotToken)
  bot.api.config.use(apiThrottler())
  bot.api.config.use(autoRetry())
  bot.use(createAuthMiddleware(config.allowedUserIds))
  await registerCommands(bot, { jira, claude })

  bot.on("message:text", async ctx => {
    if (ctx.message.text.startsWith("/")) {
      return ctx.reply("Unknown command. Try /help")
    }
    const stopTyping = keepTyping(ctx)
    try {
      const response = await claude.ask(ctx.message.text)
      await ctx.reply(response)
    } catch (err) {
      console.error({ event: "claude_reply_error", message: (err as Error).message })
      await ctx.reply("Failed to get a response from Claude. Please try again.")
    } finally {
      stopTyping()
    }
  })

  bot.catch(err => {
    const error = err.error as Error & { type?: string }
    console.error({
      event: "error",
      command: err.ctx.message?.text?.split(" ")[0],
      errorMessage: error instanceof Error ? error.message : String(error),
      errorType: error.type ?? "unknown",
    })
    err.ctx.reply("An unexpected error occurred. Please try again.").catch(() => {})
  })

  process.on("SIGTERM", async () => { await bot.stop(); process.exit(0) })
  process.on("SIGINT", async () => { await bot.stop(); process.exit(0) })

  await bot.start()
}

if (import.meta.main) {
  startBot().catch(console.error)
}
