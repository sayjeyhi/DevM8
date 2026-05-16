import { Bot } from 'grammy'
import type { TelegramConfig, CommandHandler } from './types'
import { splitMessage } from './splitMessage'
import {
  JiraAuthError,
  JiraNotFoundError,
  InvalidTransitionError,
  ClaudeTimeoutError,
  ClaudeExitError,
} from '../shared/errors'

interface Logger {
  info(obj: object): void
  error(obj: object): void
}

export class TelegramClient {
  private readonly bot: Bot
  private readonly config: TelegramConfig
  private readonly logger: Logger

  constructor(config: TelegramConfig, logger: Logger) {
    this.config = config
    this.logger = logger
    this.bot = new Bot(config.token)

    this.bot.catch(async (err) => {
      const chatId = err.ctx.chat?.id ?? 0
      const error = err.error as Error

      let message: string
      let errorType: string

      if (error instanceof JiraAuthError) {
        errorType = error.type
        message = 'Jira authentication failed. Check your API token.'
      } else if (error instanceof JiraNotFoundError) {
        errorType = error.type
        message = `Issue ${error.issueKey} not found.`
      } else if (error instanceof InvalidTransitionError) {
        errorType = error.type
        message = `Transition '${error.attempted}' not found. Valid: ${error.available.join(', ')}`
      } else if (error instanceof ClaudeTimeoutError) {
        errorType = error.type
        message = `Claude timed out after ${error.timeoutMs}ms.`
      } else if (error instanceof ClaudeExitError) {
        errorType = error.type
        message = `Claude subprocess failed (exit ${error.exitCode}).`
      } else {
        errorType = 'unknown'
        message = 'Something went wrong. Try again.'
        this.logger.error({ event: 'error', errorType, chatId })
      }

      this.logger.info({ event: 'error', errorType, chatId })

      try {
        await err.ctx.reply(message)
      } catch {
        this.logger.error({ event: 'reply_failed', chatId })
      }
    })

    this.bot.on('message:text', (ctx) => ctx.reply('Use /help to see available commands.'))
  }

  onCommand(command: string, handler: CommandHandler): void {
    this.bot.command(command, async (ctx) => {
      const userId = ctx.from?.id
      if (userId === undefined || !this.config.allowedUserIds.includes(userId)) {
        this.logger.info({ event: 'unauthorized', chatId: ctx.chat.id, command })
        return
      }

      const chatId = ctx.chat.id
      const rawText = ctx.message?.text ?? ''
      const args = rawText.split(' ').slice(1)

      this.logger.info({ event: 'command', chatId, command, argCount: args.length })

      await handler({
        chatId,
        userId,
        command,
        args,
        rawText,
        reply: (text: string) => this.sendMessage(chatId, text),
      })
    })
  }

  async sendMessage(chatId: number, text: string): Promise<void> {
    for (const chunk of splitMessage(text)) {
      await this.bot.api.sendMessage(chatId, chunk)
    }
  }

  startPolling(): void {
    this.bot.start({ drop_pending_updates: true }).catch((err: Error) => {
      this.logger.error({ event: 'polling_error', message: err.message })
    })
  }

  async stopPolling(): Promise<void> {
    await this.bot.stop()
  }
}
