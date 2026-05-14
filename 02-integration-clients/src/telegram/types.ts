export interface TelegramConfig {
  token: string
  allowedUserIds: number[]
}

export type CommandHandler = (ctx: CommandContext) => Promise<void>

export interface CommandContext {
  chatId: number
  userId: number
  command: string
  args: string[]
  rawText: string
  reply(text: string): Promise<void>
}
