import type { Context } from "grammy"
import { JiraAuthError, JiraNotFoundError } from "../../shared/errors"

// TODO: replace with import from './index' once section-07-registration-and-bot is complete
interface Clients {
  jira: {
    createIssue(title: string, description: string): Promise<{ key: string }>
  }
  claude: {
    ask(prompt: string): Promise<string>
  }
}

export const ENRICH_PROMPT_TEMPLATE = `You are a Jira ticket writer. Given the title and description provided by the user, write a well-formatted Jira ticket description in plain text.

<title>{title}</title>
<description>{description}</description>

Return only the formatted description. No preamble, no metadata.`

export const EXPAND_PROMPT_TEMPLATE = `You are a Jira ticket writer. Given the title provided by the user, write a Jira ticket description with 3-5 sentences and 3-5 acceptance criteria bullet points.

<title>{title}</title>

Return only the formatted description. No preamble, no metadata.`

export async function handleCreate(ctx: Context, clients: Clients): Promise<void> {
  const match = ((ctx.match as string) ?? "").trim()

  if (!match) {
    await ctx.reply("Usage: /create <title> [-- <description>]")
    return
  }

  try {
    await ctx.replyWithChatAction("typing")
    const typingInterval = setInterval(() => {
      ctx.replyWithChatAction("typing").catch(() => {})
    }, 4000)

    try {
      let title: string
      let description: string

      const separatorIdx = match.indexOf(" -- ")
      if (separatorIdx !== -1) {
        // Path A: enrich with Claude
        title = match.slice(0, separatorIdx).trim()
        const rawDescription = match.slice(separatorIdx + 4).trim()
        let enriched = rawDescription
        try {
          // Replace {description} before {title} to prevent cross-substitution injection
          const prompt = ENRICH_PROMPT_TEMPLATE.replace("{description}", rawDescription).replace(
            "{title}",
            title,
          )
          enriched = await clients.claude.ask(prompt)
        } catch {
          // silent fallback — use raw description
        }
        description = enriched
      } else {
        // Path B: expand with Claude
        title = match
        let expanded = ""
        try {
          const prompt = EXPAND_PROMPT_TEMPLATE.replace("{title}", title)
          expanded = await clients.claude.ask(prompt)
        } catch {
          // silent fallback — use empty description
        }
        description = expanded
      }

      const issue = await clients.jira.createIssue(title, description)
      await ctx.reply(`Created: ${issue.key}`)
    } finally {
      clearInterval(typingInterval)
    }
  } catch (err) {
    if (err instanceof JiraAuthError) {
      await ctx.reply("Authentication failed. Please check your Jira API token.")
      return
    }
    if (err instanceof JiraNotFoundError) {
      await ctx.reply("Jira resource not found.")
      return
    }
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "create", errorMessage: message })
    await ctx.reply("Something went wrong. Please try again.")
  }
}
