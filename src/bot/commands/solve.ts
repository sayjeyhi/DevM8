import type { Context } from "grammy"
import { JiraAuthError, JiraNotFoundError, ClaudeTimeoutError, ClaudeExitError } from "../../shared/errors"
import { parseArgs } from "../utils/parseArgs"
import { splitMessage } from "../utils/splitMessage"

interface Clients {
  jira: {
    getIssue(key: string): Promise<{ key: string; summary: string; status: string; description: string }>
  }
  claude: {
    ask(prompt: string, options?: { onProgress?: (lines: string[]) => Promise<void> }): Promise<string>
  }
}

export const SOLVE_PROMPT_TEMPLATE = `You are a software engineer analyzing a Jira issue. Provide actionable next steps or a solution approach.

<key>{KEY}</key>
<title>{TITLE}</title>
<status>{STATUS}</status>
<description>{DESCRIPTION}</description>

Analyze the issue and respond with:
1. A brief assessment of the problem
2. Concrete next steps or a solution approach
3. Any potential blockers or risks to consider

Be concise and practical.`

function escHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
}

export async function solveByKey(ctx: Context, clients: Clients, key: string): Promise<void> {
  const progressMsg = await ctx.reply(
    `🔍 Analyzing <b>${escHtml(key)}</b> with Claude...`,
    { parse_mode: "HTML" },
  )
  const chatId = ctx.chat!.id
  const msgId = progressMsg.message_id

  const editProgress = async (text: string) => {
    try {
      await ctx.api.editMessageText(chatId, msgId, text, { parse_mode: "HTML" })
    } catch { /* ignore "message not modified" and other transient edit errors */ }
  }

  try {
    await ctx.replyWithChatAction("typing")

    const issue = await clients.jira.getIssue(key)

    // Description content is placed inside XML tags to signal it is data,
    // not instructions — prompt injection defense.
    const prompt = SOLVE_PROMPT_TEMPLATE
      .replace("{KEY}", issue.key)
      .replace("{TITLE}", issue.summary)
      .replace("{STATUS}", issue.status)
      .replace("{DESCRIPTION}", issue.description ?? "")

    const response = await clients.claude.ask(prompt, {
      onProgress: async (lines: string[]) => {
        const preview = lines.map(escHtml).join('\n')
        await editProgress(
          `🔍 Analyzing <b>${escHtml(key)}</b> with Claude...\n\n<pre>${preview}</pre>`,
        )
      },
    })

    await editProgress(`✅ Analysis complete for <b>${escHtml(key)}</b>`)
    for (const chunk of splitMessage(response)) {
      await ctx.reply(chunk)
    }
  } catch (err) {
    if (err instanceof JiraNotFoundError) { await ctx.reply(`Ticket ${key} not found.`); return }
    if (err instanceof JiraAuthError) { await ctx.reply("Jira authentication failed. Check your API token."); return }
    if (err instanceof ClaudeTimeoutError) { await ctx.reply("Claude timed out. Please try again."); return }
    if (err instanceof ClaudeExitError) {
      console.log({ event: "error", command: "solve", key, exitCode: err.exitCode, stderr: err.stderr.slice(0, 500) })
      const isAuthError = /not logged in|please run \/login/i.test(err.stderr)
      await ctx.reply(
        isAuthError
          ? "Claude is not authenticated. Run `claude login` in your terminal (not the app), then restart the bot. Alternatively set ANTHROPIC_API_KEY in your environment."
          : "Claude returned an error. Please try again."
      )
      return
    }
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "solve", errorMessage: message })
    await ctx.reply("Something went wrong. Please try again.")
  }
}

export async function handleSolve(ctx: Context, clients: Clients): Promise<void> {
  const args = parseArgs(ctx)
  const key = args[0]
  if (!key) {
    await ctx.reply("Usage: /solve <ticket-key>")
    return
  }
  await solveByKey(ctx, clients, key)
}
