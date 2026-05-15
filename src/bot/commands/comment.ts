import type { Context } from "grammy"
import { JiraAuthError, JiraNotFoundError } from "../../shared/errors"
import { parseFirstAndRest } from "../utils/parseArgs"

// TODO: replace with import from '../commands/index' once section-07 is complete
interface Clients {
  jira: {
    addComment(key: string, text: string): Promise<void>
  }
}

export async function handleComment(ctx: Context, clients: Clients): Promise<void> {
  const match = ((ctx.match as string) ?? "").trim()

  if (!match) {
    await ctx.reply("Usage: /comment <issue-key> <text>")
    return
  }

  const parsed = parseFirstAndRest(match)
  if (!parsed) {
    await ctx.reply("Usage: /comment <issue-key> <text>")
    return
  }

  const { first: key, rest: text } = parsed

  try {
    await ctx.replyWithChatAction("typing")
    await clients.jira.addComment(key, text)
    await ctx.reply(`Comment added to ${key}`)
  } catch (err) {
    if (err instanceof JiraNotFoundError) {
      await ctx.reply(`Issue ${key} not found.`)
      return
    }
    if (err instanceof JiraAuthError) {
      await ctx.reply("Authentication failed. Please check your Jira API token.")
      return
    }
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "comment", errorMessage: message })
    await ctx.reply("Something went wrong. Please try again.")
  }
}
