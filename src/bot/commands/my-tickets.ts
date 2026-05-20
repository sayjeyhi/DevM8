import type { Context } from "grammy"
import type { Clients } from "./index"
import { solveByKey, handleRepoPicker } from "./solve"
import { keepTyping } from "../utils/typing"
import { JiraAuthError } from "../../shared/errors"
import type { JiraIssue } from "../../jira/types"

export const PAGE_SIZE = 5
const DESC_LIMIT = 800

// tokens[pageN] = nextPageToken needed to fetch page N (undefined = first page, no token needed)
const pageCache = new Map<number, { tokens: Map<number, string | undefined>; hasNext: Map<number, boolean>; status?: string; projectKey?: string }>()

// chatId → selected project key, set during project picker step
const projectCache = new Map<number, string>()

// Stores available transitions per chatId+key while user picks one
const transitionCache = new Map<number, Map<string, Array<{ id: string; name: string }>>>()

// chatId → ticket key: next text message from that chat is a comment for that key
export const pendingComments = new Map<number, string>()

interface CallbackButton {
  text: string
  callback_data: string
}

interface InlineKeyboardMarkup {
  inline_keyboard: CallbackButton[][]
}

const STATUS_EMOJI: Record<string, string> = {
  "to do": "⬜",
  "open": "⬜",
  "backlog": "⬜",
  "in progress": "🔵",
  "in review": "🟣",
  "blocked": "🔴",
  "done": "✅",
  "closed": "✅",
  "resolved": "✅",
  "cancelled": "⛔",
}

export function statusEmoji(status: string): string {
  return STATUS_EMOJI[status.toLowerCase()] ?? "⚪"
}

export function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
}

export function formatTicketsPage(issues: JiraIssue[], page: number, hasNext: boolean, botUsername?: string, status?: string): string {
  const from = page * PAGE_SIZE + 1
  const to = from + issues.length - 1
  const range = hasNext ? `${from}–${to}+` : `${from}–${to}`
  const statusLabel = status ? ` · ${statusEmoji(status)} ${escapeHtml(status)}` : ""

  const lines = issues.map(({ key, summary, status: s, url }, i) => {
    const num = from + i
    const emoji = statusEmoji(s)
    const detailLink = botUsername
      ? ` · <a href="https://t.me/${botUsername}?start=detail_${key}">Details</a>`
      : ""
    return `${num}. <b>${escapeHtml(key)}: ${escapeHtml(summary)}</b>\n   ${emoji} ${escapeHtml(s)} · <a href="${url}">Open in Jira</a>${detailLink}`
  })

  return `📋 Your tickets (${range})${statusLabel}:\n\n${lines.join("\n\n")}`
}

export function buildNavKeyboard(page: number, hasPrev: boolean, hasNext: boolean): InlineKeyboardMarkup | undefined {
  const buttons: CallbackButton[] = []
  if (hasPrev) buttons.push({ text: "← Prev", callback_data: `myt:p:${page - 1}` })
  if (hasNext) buttons.push({ text: "Next →", callback_data: `myt:p:${page + 1}` })
  if (buttons.length === 0) return undefined
  return { inline_keyboard: [buttons] }
}

export function buildDetailsActionKeyboard(key: string): InlineKeyboardMarkup {
  return {
    inline_keyboard: [[
      { text: "🤖 Solve", callback_data: `tkt:solve:${key}` },
      { text: "↗️ Move", callback_data: `tkt:move:${key}` },
      { text: "💬 Comment", callback_data: `tkt:comment:${key}` },
    ]],
  }
}

const CATEGORY_ORDER: Record<string, number> = { "To Do": 0, "In Progress": 1, "Done": 2 }

export async function handleMyTickets(ctx: Context, clients: Clients): Promise<void> {
  const { jira } = clients
  const configuredKeys = jira.projectKeys

  if (configuredKeys.length === 1) {
    await handleMyTicketsProject(ctx, clients, configuredKeys[0])
    return
  }

  const stopTyping = keepTyping(ctx)
  try {
    let projects: Array<{ key: string; name: string }>
    try {
      const all = await jira.getProjects()
      const keySet = new Set(configuredKeys)
      const filtered = all.filter(p => keySet.has(p.key))
      projects = filtered.length > 0 ? filtered : configuredKeys.map(k => ({ key: k, name: k }))
    } catch {
      projects = configuredKeys.map(k => ({ key: k, name: k }))
    }
    stopTyping()

    const buttons: CallbackButton[] = projects.map(p => ({
      text: `📁 ${p.key} — ${p.name}`,
      callback_data: `myt:proj:${p.key}`,
    }))

    const rows: CallbackButton[][] = []
    for (let i = 0; i < buttons.length; i += 2) rows.push(buttons.slice(i, i + 2))

    await ctx.reply("Select a project:", {
      reply_markup: { inline_keyboard: rows },
    })
  } catch (err) {
    stopTyping()
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "my_tickets", errorMessage: message })
    await ctx.reply("Something went wrong. Please try again.")
  }
}

export async function handleMyTicketsProject(ctx: Context, { jira }: Clients, projectKey: string): Promise<void> {
  const chatId = ctx.chat!.id
  projectCache.set(chatId, projectKey)
  const stopTyping = keepTyping(ctx)
  try {
    const statuses = await jira.getStatuses().finally(stopTyping)

    const seen = new Set<string>()
    const sorted = statuses
      .filter(s => { const dup = seen.has(s.name); seen.add(s.name); return !dup })
      .sort((a, b) => (CATEGORY_ORDER[a.category] ?? 99) - (CATEGORY_ORDER[b.category] ?? 99))

    const buttons: CallbackButton[] = [
      { text: "📋 All", callback_data: "myt:s:" },
      ...sorted.map(s => ({ text: `${statusEmoji(s.name)} ${s.name}`, callback_data: `myt:s:${s.name}` })),
    ]

    const rows: CallbackButton[][] = []
    for (let i = 0; i < buttons.length; i += 3) rows.push(buttons.slice(i, i + 3))

    await ctx.reply(`<b>${escapeHtml(projectKey)}</b> — select a status:`, {
      parse_mode: "HTML",
      reply_markup: { inline_keyboard: rows },
    })
  } catch (err) {
    stopTyping()
    if (err instanceof JiraAuthError) {
      await ctx.reply("Authentication failed. Please check your Jira API token.")
      return
    }
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "my_tickets_project", projectKey, errorMessage: message })
    await ctx.reply("Something went wrong. Please try again.")
  } finally {
    if (ctx.callbackQuery) await ctx.answerCallbackQuery()
  }
}

export async function handleMyTicketsStatus(ctx: Context, { jira }: Clients, status: string): Promise<void> {
  const chatId = ctx.chat!.id
  const projectKey = projectCache.get(chatId)
  try {
    const { issues, nextPageToken } = await jira.getMyIssues(PAGE_SIZE, undefined, status || undefined, projectKey)

    if (issues.length === 0) {
      const label = status ? `with status <b>${escapeHtml(status)}</b>` : ""
      await ctx.reply(`No tickets assigned to you${label ? ` ${label}` : ""}.`, { parse_mode: "HTML" })
      return
    }

    const tokens = new Map<number, string | undefined>([[0, undefined]])
    const hasNext = new Map<number, boolean>([[0, !!nextPageToken]])
    if (nextPageToken) tokens.set(1, nextPageToken)
    pageCache.set(chatId, { tokens, hasNext, status: status || undefined, projectKey })

    const botUsername = ctx.me?.username
    const text = formatTicketsPage(issues, 0, !!nextPageToken, botUsername, status || undefined)
    const keyboard = buildNavKeyboard(0, false, !!nextPageToken)

    await ctx.reply(text, { parse_mode: "HTML", reply_markup: keyboard })
  } catch (err) {
    if (err instanceof JiraAuthError) {
      await ctx.reply("Authentication failed. Please check your Jira API token.")
      return
    }
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "my_tickets_status", errorMessage: message })
    await ctx.reply("Something went wrong. Please try again.")
  } finally {
    if (ctx.callbackQuery) await ctx.answerCallbackQuery()
  }
}

export async function handleMyTicketsPage(ctx: Context, { jira }: Clients, page: number): Promise<void> {
  try {
    const chatId = ctx.chat!.id
    const cache = pageCache.get(chatId)
    const token = cache?.tokens.get(page)

    const { issues, nextPageToken } = await jira.getMyIssues(PAGE_SIZE, token, cache?.status, cache?.projectKey)

    if (cache) {
      cache.hasNext.set(page, !!nextPageToken)
      if (nextPageToken) cache.tokens.set(page + 1, nextPageToken)
    }

    const hasNext = !!nextPageToken
    const hasPrev = page > 0
    const botUsername = ctx.me?.username
    const text = formatTicketsPage(issues, page, hasNext, botUsername, cache?.status)
    const keyboard = buildNavKeyboard(page, hasPrev, hasNext)

    await ctx.editMessageText(text, { parse_mode: "HTML", reply_markup: keyboard })
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "my_tickets_page", errorMessage: message })
  } finally {
    await ctx.answerCallbackQuery()
  }
}

export async function handleTicketDetails(ctx: Context, { jira }: Clients, key: string): Promise<void> {
  try {
    const issue = await jira.getIssue(key)
    const emoji = statusEmoji(issue.status)

    const rawDesc = issue.description
    const truncated = rawDesc && rawDesc.length > DESC_LIMIT
    const descText = rawDesc
      ? `<pre>${escapeHtml(rawDesc.slice(0, DESC_LIMIT))}${truncated ? "\n…" : ""}</pre>`
      : "<i>No description</i>"

    const text = [
      `<b>${escapeHtml(issue.key)}: ${escapeHtml(issue.summary)}</b>`,
      `${emoji} ${escapeHtml(issue.status)} · <a href="${issue.url}">Open in Jira</a>`,
      descText,
    ].join("\n\n")

    await ctx.reply(text, { parse_mode: "HTML", reply_markup: buildDetailsActionKeyboard(key) })
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "ticket_details", key, errorMessage: message })
    await ctx.reply("Could not load ticket details. Please try again.")
  } finally {
    if (ctx.callbackQuery) await ctx.answerCallbackQuery()
  }
}

export async function handleSolveTicket(ctx: Context, clients: Clients, key: string): Promise<void> {
  if (clients.gitMap) {
    await handleRepoPicker(ctx, clients, key)
    return
  }
  await ctx.answerCallbackQuery()
  await solveByKey(ctx, clients, key)
}

export async function handleMoveStart(ctx: Context, { jira }: Clients, key: string): Promise<void> {
  try {
    const chatId = ctx.chat!.id
    const transitions = await jira.getTransitions(key)

    if (!transitionCache.has(chatId)) transitionCache.set(chatId, new Map())
    transitionCache.get(chatId)!.set(key, transitions)

    const buttons = transitions.map((t, i) => ({ text: t.name, callback_data: `tkt:trn:${key}:${i}` }))
    const rows: CallbackButton[][] = []
    for (let i = 0; i < buttons.length; i += 2) rows.push(buttons.slice(i, i + 2))

    await ctx.reply(`Move <b>${escapeHtml(key)}</b> to:`, {
      parse_mode: "HTML",
      reply_markup: { inline_keyboard: rows },
    })
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "ticket_move_start", key, errorMessage: message })
    await ctx.reply("Could not fetch transitions. Please try again.")
  } finally {
    await ctx.answerCallbackQuery()
  }
}

export async function handleMoveExecute(ctx: Context, { jira }: Clients, key: string, idx: number): Promise<void> {
  try {
    const chatId = ctx.chat!.id
    const transition = transitionCache.get(chatId)?.get(key)?.[idx]
    if (!transition) {
      await ctx.reply("Transition expired. Please tap Move again.")
      return
    }
    await jira.transitionIssue(key, transition.name)
    await ctx.editMessageText(`✅ Moved <b>${escapeHtml(key)}</b> → ${escapeHtml(transition.name)}`, { parse_mode: "HTML" })
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    console.log({ event: "error", command: "ticket_move_execute", key, errorMessage: message })
    await ctx.reply("Could not move ticket. Please try again.")
  } finally {
    await ctx.answerCallbackQuery()
  }
}

export async function handleCommentStart(ctx: Context, key: string): Promise<void> {
  const chatId = ctx.chat!.id
  pendingComments.set(chatId, key)
  await ctx.reply(`Type your comment for <b>${escapeHtml(key)}</b> and send it:`, { parse_mode: "HTML" })
  await ctx.answerCallbackQuery()
}
