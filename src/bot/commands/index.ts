import type { Bot, Context } from "grammy"
import { CommandGroup } from "@grammyjs/commands"
import type { JiraClient } from "../../jira/JiraClient"
import type { ClaudeClient } from "../../claude/ClaudeClient"
import { handleCreate } from "./create"
import { handleMove } from "./move"
import { handleComment } from "./comment"
import { handleHelp } from "./help"
import { handleSolve } from "./solve"
import { handleLogs } from "./logs"
import { handleMyTickets } from "./my-tickets"

export interface Clients {
  jira: JiraClient
  claude: ClaudeClient
}

export async function registerCommands(bot: Bot, clients: Clients): Promise<void> {
  const commands = new CommandGroup<Context>()

  commands.command("create", "Create a new Jira ticket", ctx => handleCreate(ctx, clients))
  commands.command("move", "Move a ticket to a new status", ctx => handleMove(ctx, clients))
  commands.command("comment", "Add a comment to a ticket", ctx => handleComment(ctx, clients))
  commands.command("solve", "Ask Claude for a solution to a ticket", ctx => handleSolve(ctx, clients))
  commands.command("my_tickets", "List your last 10 assigned Jira tickets", ctx => handleMyTickets(ctx, clients))
  commands.command("logs", "Show recent daemon logs", ctx => handleLogs(ctx))
  commands.command("help", "Show available commands", ctx => handleHelp(ctx))

  bot.use(commands)
  // setCommands syncs the UI menu — failure is non-fatal, bot routing still works
  try {
    await commands.setCommands(bot)
  } catch (err) {
    console.error({ event: "setCommands_failed", errorMessage: err instanceof Error ? err.message : String(err) })
  }
}
