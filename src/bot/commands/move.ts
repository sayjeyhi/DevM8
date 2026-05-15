import type { Context } from "grammy"
import { InvalidTransitionError, JiraAuthError, JiraNotFoundError } from "../../shared/errors"
import { parseFirstAndRest } from "../utils/parseArgs"

// TODO: replace with import from '../commands/index' once section-07 is complete
interface Clients {
  jira: {
    transitionIssue(key: string, status: string): Promise<void>
  }
}

export async function handleMove(ctx: Context, clients: Clients): Promise<void> {
  const match = ((ctx.match as string) ?? "").trim()

  if (!match) {
    await ctx.reply("Usage: /move <issue-key> <status>")
    return
  }

  const parsed = parseFirstAndRest(match)
  if (!parsed) {
    await ctx.reply("Usage: /move <issue-key> <status>")
    return
  }

  const { first: key, rest: status } = parsed

  try {
    await ctx.replyWithChatAction("typing")
    await clients.jira.transitionIssue(key, status)
    await ctx.reply(`Moved ${key} → ${status}`)
  } catch (err) {
    if (err instanceof InvalidTransitionError) {
      await ctx.reply(`Cannot move to "${status}". Available: ${err.available.join(", ")}`)
      return
    }
    if (err instanceof JiraNotFoundError) {
      await ctx.reply(`Issue ${key} not found.`)
      return
    }
    if (err instanceof JiraAuthError) {
      await ctx.reply("Authentication failed. Please check your Jira API token.")
      return
    }
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "move", errorMessage: message })
    await ctx.reply("Something went wrong. Please try again.")
  }
}
