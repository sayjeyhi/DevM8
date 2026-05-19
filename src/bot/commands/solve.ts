import type { Context } from "grammy"
import { JiraAuthError, JiraNotFoundError, ClaudeTimeoutError, ClaudeExitError } from "../../shared/errors"
import type { AskOptions } from "../../claude/types"
import type { GitClient } from "../../git/GitClient"
import { parseArgs } from "../utils/parseArgs"
import { splitMessage } from "../utils/splitMessage"

interface Clients {
  jira: {
    getIssue(key: string): Promise<{ key: string; summary: string; status: string; description: string }>
  }
  claude: {
    ask(prompt: string, options?: AskOptions): Promise<string>
  }
  git?: GitClient
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

export async function solveByKey(
  ctx: Context,
  clients: Clients,
  key: string,
  options?: { cwd?: string },
): Promise<void> {
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
      cwd: options?.cwd,
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

function branchName(key: string): string {
  return `feat/${key.toLowerCase()}`
}

export async function handleBranchPicker(ctx: Context, clients: Clients, key: string): Promise<void> {
  const git = clients.git!
  let branch: string
  let clean: boolean

  try {
    ;[branch, clean] = await Promise.all([git.currentBranch(), git.isClean()])
  } catch {
    // git unavailable — skip picker, solve directly
    await ctx.answerCallbackQuery()
    await solveByKey(ctx, clients, key, { cwd: git.repoPath })
    return
  }

  const statusLine = clean ? "✅ working tree clean" : "⚠️ uncommitted changes"
  const newBranch = branchName(key)

  await ctx.reply(
    [
      `🌿 <b>${escHtml(key)}</b> — select branch:`,
      ``,
      `Branch: <code>${escHtml(branch)}</code>`,
      `Status: ${statusLine}`,
    ].join("\n"),
    {
      parse_mode: "HTML",
      reply_markup: {
        inline_keyboard: [[
          { text: `🌿 New branch (${newBranch})`, callback_data: `tkt:branch:${key}:new` },
          { text: `📌 Stay on ${branch}`, callback_data: `tkt:branch:${key}:curr` },
        ]],
      },
    },
  )
  await ctx.answerCallbackQuery()
}

export async function handleBranchChoice(
  ctx: Context,
  clients: Clients,
  key: string,
  choice: "new" | "curr",
): Promise<void> {
  const git = clients.git!
  const cwd = git.repoPath

  if (choice === "new") {
    const branch = branchName(key)
    const statusMsg = await ctx.reply(
      `⏳ Checking out <code>${escHtml(branch)}</code>...`,
      { parse_mode: "HTML" },
    )
    const chatId = ctx.chat!.id
    const msgId = statusMsg.message_id

    try {
      await git.checkoutNewBranchFromMain(branch)
      await ctx.api.editMessageText(
        chatId, msgId,
        `✅ On branch <code>${escHtml(branch)}</code>`,
        { parse_mode: "HTML" },
      )
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      console.log({ event: "error", command: "branch_checkout", branch, errorMessage: msg })
      await ctx.api.editMessageText(
        chatId, msgId,
        `⚠️ Branch checkout failed — continuing on current branch`,
        { parse_mode: "HTML" },
      )
    }
  }

  await ctx.answerCallbackQuery()
  await solveByKey(ctx, clients, key, { cwd })
}

export async function handleSolve(ctx: Context, clients: Clients): Promise<void> {
  const args = parseArgs(ctx)
  const key = args[0]
  if (!key) {
    await ctx.reply("Usage: /solve <ticket-key>")
    return
  }
  if (clients.git) {
    await handleBranchPicker(ctx, clients, key)
    return
  }
  await solveByKey(ctx, clients, key)
}
