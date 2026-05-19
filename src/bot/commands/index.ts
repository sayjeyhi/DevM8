import type { Bot, Context } from "grammy"
import { CommandGroup } from "@grammyjs/commands"
import type { JiraClient } from "../../jira/JiraClient"
import type { ClaudeClient } from "../../claude/ClaudeClient"
import type { GitClient } from "../../git/GitClient"
import { handleCreate } from "./create"
import { handleMove } from "./move"
import { handleComment } from "./comment"
import { handleHelp } from "./help"
import { handleSolve, handleBranchChoice } from "./solve"
import { handleLogs } from "./logs"
import {
  handleMyTickets,
  handleMyTicketsProject,
  handleMyTicketsStatus,
  handleMyTicketsPage,
  handleTicketDetails,
  handleSolveTicket,
  handleMoveStart,
  handleMoveExecute,
  handleCommentStart,
  pendingComments,
} from "./my-tickets"

export interface Clients {
  jira: JiraClient
  claude: ClaudeClient
  git?: GitClient
}

export async function registerCommands(bot: Bot, clients: Clients): Promise<void> {
  // Intercept pending comment replies before command routing
  bot.on("message:text", async (ctx, next) => {
    const chatId = ctx.chat.id
    const key = pendingComments.get(chatId)
    if (key) {
      pendingComments.delete(chatId)
      try {
        await ctx.replyWithChatAction("typing")
        await clients.jira.addComment(key, ctx.message.text)
        await ctx.reply(`✅ Comment added to ${key}`)
      } catch {
        await ctx.reply("Could not add comment. Please try again.")
      }
      return
    }
    return next()
  })

  const commands = new CommandGroup<Context>()

  commands.command("create", "Create a new Jira ticket", ctx => handleCreate(ctx, clients))
  commands.command("move", "Move a ticket to a new status", ctx => handleMove(ctx, clients))
  commands.command("comment", "Add a comment to a ticket", ctx => handleComment(ctx, clients))
  commands.command("solve", "Ask Claude for a solution to a ticket", ctx => handleSolve(ctx, clients))
  commands.command("my_tickets", "List your last 10 assigned Jira tickets", ctx => handleMyTickets(ctx, clients))
  commands.command("logs", "Show recent daemon logs", ctx => handleLogs(ctx))
  commands.command("help", "Show available commands", ctx => handleHelp(ctx))

  bot.use(commands)

  bot.command("start", async ctx => {
    const payload = ctx.match
    if (payload?.startsWith("detail_")) {
      const key = payload.slice(7)
      await handleTicketDetails(ctx, clients, key)
    }
  })

  bot.callbackQuery(/^myt:proj:(.+)$/, async ctx => {
    const projectKey = (ctx.match as RegExpMatchArray)[1]
    await handleMyTicketsProject(ctx, clients, projectKey)
  })

  bot.callbackQuery(/^myt:s:(.*)$/, async ctx => {
    const status = (ctx.match as RegExpMatchArray)[1]
    await handleMyTicketsStatus(ctx, clients, status)
  })

  bot.callbackQuery(/^myt:p:(\d+)$/, async ctx => {
    const page = parseInt((ctx.match as RegExpMatchArray)[1], 10)
    await handleMyTicketsPage(ctx, clients, page)
  })

  bot.callbackQuery(/^tkt:solve:([A-Z]+-\d+)$/, async ctx => {
    const key = (ctx.match as RegExpMatchArray)[1]
    await handleSolveTicket(ctx, clients, key)
  })

  bot.callbackQuery(/^tkt:move:([A-Z]+-\d+)$/, async ctx => {
    const key = (ctx.match as RegExpMatchArray)[1]
    await handleMoveStart(ctx, clients, key)
  })

  bot.callbackQuery(/^tkt:trn:([A-Z]+-\d+):(\d+)$/, async ctx => {
    const [, key, idxStr] = ctx.match as RegExpMatchArray
    await handleMoveExecute(ctx, clients, key, parseInt(idxStr, 10))
  })

  bot.callbackQuery(/^tkt:comment:([A-Z]+-\d+)$/, async ctx => {
    const key = (ctx.match as RegExpMatchArray)[1]
    await handleCommentStart(ctx, key)
  })

  bot.callbackQuery(/^tkt:branch:([A-Z]+-\d+):(new|curr)$/, async ctx => {
    const [, key, choice] = ctx.match as RegExpMatchArray
    await handleBranchChoice(ctx, clients, key, choice as "new" | "curr")
  })

  // setCommands syncs the UI menu — failure is non-fatal, bot routing still works
  try {
    await commands.setCommands(bot)
  } catch (err) {
    console.error({ event: "setCommands_failed", errorMessage: err instanceof Error ? err.message : String(err) })
  }
}
