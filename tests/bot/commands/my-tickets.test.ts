import { describe, it, expect, mock } from "bun:test"
import {
  handleMyTickets,
  handleMyTicketsProject,
  handleMyTicketsStatus,
  handleMyTicketsPage,
  handleTicketDetails,
  statusEmoji,
  escapeHtml,
  formatTicketsPage,
  buildNavKeyboard,
  PAGE_SIZE,
} from "../../../src/bot/commands/my-tickets"
import { JiraAuthError } from "../../../src/shared/errors"
import type { JiraIssue } from "../../../src/jira/types"

function makeIssue(n: number, status = "In Progress"): JiraIssue {
  return {
    key: `ENG-${n}`,
    summary: `Issue ${n}`,
    status,
    description: "",
    url: `https://jira.example.com/browse/ENG-${n}`,
  }
}

function makeCtx(chatId = 42) {
  return {
    chat: { id: chatId },
    reply: mock().mockResolvedValue({}),
    replyWithChatAction: mock().mockResolvedValue({}),
    editMessageText: mock().mockResolvedValue({}),
    answerCallbackQuery: mock().mockResolvedValue({}),
  }
}

const MOCK_STATUSES = [
  { id: "1", name: "To Do", category: "To Do" },
  { id: "2", name: "In Progress", category: "In Progress" },
  { id: "3", name: "Done", category: "Done" },
]

const MOCK_PROJECTS = [
  { key: "MP", name: "Main Project" },
  { key: "BZ", name: "Blaze" },
]

function makeClients(opts: {
  listResult?: unknown
  issueResult?: unknown
  statusesResult?: unknown
  projectsResult?: unknown
  projectKeys?: string[]
} = {}) {
  const {
    listResult = { issues: [], nextPageToken: undefined },
    issueResult = makeIssue(1),
    statusesResult = MOCK_STATUSES,
    projectsResult = MOCK_PROJECTS,
    projectKeys = ["MP", "BZ"],
  } = opts
  return {
    jira: {
      projectKeys,
      getProjects:
        projectsResult instanceof Error
          ? mock().mockRejectedValue(projectsResult)
          : mock().mockResolvedValue(projectsResult),
      getStatuses:
        statusesResult instanceof Error
          ? mock().mockRejectedValue(statusesResult)
          : mock().mockResolvedValue(statusesResult),
      getMyIssues:
        listResult instanceof Error
          ? mock().mockRejectedValue(listResult)
          : mock().mockResolvedValue(listResult),
      getIssue:
        issueResult instanceof Error
          ? mock().mockRejectedValue(issueResult)
          : mock().mockResolvedValue(issueResult),
    },
  }
}

describe("statusEmoji", () => {
  it("returns ✅ for Done", () => { expect(statusEmoji("Done")).toBe("✅") })
  it("returns ✅ for Closed", () => { expect(statusEmoji("Closed")).toBe("✅") })
  it("returns ✅ for Resolved", () => { expect(statusEmoji("Resolved")).toBe("✅") })
  it("returns 🔵 for In Progress", () => { expect(statusEmoji("In Progress")).toBe("🔵") })
  it("returns 🟣 for In Review", () => { expect(statusEmoji("In Review")).toBe("🟣") })
  it("returns ⬜ for To Do", () => { expect(statusEmoji("To Do")).toBe("⬜") })
  it("returns ⬜ for Backlog", () => { expect(statusEmoji("Backlog")).toBe("⬜") })
  it("returns 🔴 for Blocked", () => { expect(statusEmoji("Blocked")).toBe("🔴") })
  it("returns ⛔ for Cancelled", () => { expect(statusEmoji("Cancelled")).toBe("⛔") })
  it("returns ⚪ for unknown status", () => { expect(statusEmoji("Weird Status")).toBe("⚪") })
  it("is case-insensitive", () => { expect(statusEmoji("IN PROGRESS")).toBe("🔵") })
})

describe("escapeHtml", () => {
  it("escapes ampersand", () => { expect(escapeHtml("a & b")).toBe("a &amp; b") })
  it("escapes less-than", () => { expect(escapeHtml("a < b")).toBe("a &lt; b") })
  it("escapes greater-than", () => { expect(escapeHtml("a > b")).toBe("a &gt; b") })
  it("passes plain text unchanged", () => { expect(escapeHtml("hello world")).toBe("hello world") })
  it("escapes multiple entities in one string", () => {
    expect(escapeHtml("<script>alert('xss')</script>")).toBe("&lt;script&gt;alert('xss')&lt;/script&gt;")
  })
})

describe("formatTicketsPage", () => {
  it("wraps key and summary in bold tag", () => {
    const issues = [makeIssue(1)]
    const text = formatTicketsPage(issues, 0, false)
    expect(text).toContain("<b>ENG-1: Issue 1</b>")
  })

  it("includes Open in Jira link", () => {
    const issues = [makeIssue(1)]
    const text = formatTicketsPage(issues, 0, false)
    expect(text).toContain('<a href="https://jira.example.com/browse/ENG-1">Open in Jira</a>')
  })

  it("includes Details deep link when botUsername provided", () => {
    const issues = [makeIssue(1)]
    const text = formatTicketsPage(issues, 0, false, "mybot")
    expect(text).toContain('<a href="https://t.me/mybot?start=detail_ENG-1">Details</a>')
  })

  it("omits Details link when no botUsername", () => {
    const issues = [makeIssue(1)]
    const text = formatTicketsPage(issues, 0, false)
    expect(text).not.toContain("Details")
  })

  it("shows range without + when no next page", () => {
    const issues = [makeIssue(1)]
    const text = formatTicketsPage(issues, 0, false)
    expect(text).toContain("1–1")
    expect(text).not.toContain("+")
  })

  it("appends + to range when more pages exist", () => {
    const issues = Array.from({ length: PAGE_SIZE }, (_, i) => makeIssue(i + 1))
    const text = formatTicketsPage(issues, 0, true)
    expect(text).toContain(`1–${PAGE_SIZE}+`)
  })

  it("calculates range from offset on page 2", () => {
    const issues = [makeIssue(6), makeIssue(7)]
    const text = formatTicketsPage(issues, 1, false)
    expect(text).toContain("6–7")
  })

  it("numbers items from correct offset", () => {
    const issues = [makeIssue(6), makeIssue(7)]
    const text = formatTicketsPage(issues, 1, false)
    expect(text).toContain("6. <b>ENG-6")
    expect(text).toContain("7. <b>ENG-7")
  })

  it("shows status emoji for each ticket", () => {
    const issues = [makeIssue(1, "Done"), makeIssue(2, "In Progress")]
    const text = formatTicketsPage(issues, 0, false)
    expect(text).toContain("✅")
    expect(text).toContain("🔵")
  })

  it("escapes HTML in summary", () => {
    const issue = { ...makeIssue(1), summary: "Fix <bug> & crash" }
    const text = formatTicketsPage([issue], 0, false)
    expect(text).toContain("Fix &lt;bug&gt; &amp; crash")
    expect(text).not.toContain("<bug>")
  })

  it("separates tickets with blank lines", () => {
    const issues = [makeIssue(1), makeIssue(2)]
    const text = formatTicketsPage(issues, 0, false)
    expect(text).toContain("\n\n")
  })
})

describe("buildNavKeyboard", () => {
  it("returns undefined when no prev and no next", () => {
    expect(buildNavKeyboard(0, false, false)).toBeUndefined()
  })

  it("returns keyboard with Next when hasNext", () => {
    const kb = buildNavKeyboard(0, false, true)!
    const buttons = kb.inline_keyboard[0]
    expect(buttons.some(b => b.text.includes("Next"))).toBe(true)
    expect(buttons.some(b => b.text.includes("Prev"))).toBe(false)
  })

  it("returns keyboard with Prev when hasPrev", () => {
    const kb = buildNavKeyboard(1, true, false)!
    const buttons = kb.inline_keyboard[0]
    expect(buttons.some(b => b.text.includes("Prev"))).toBe(true)
    expect(buttons.some(b => b.text.includes("Next"))).toBe(false)
  })

  it("middle page has both Prev and Next", () => {
    const kb = buildNavKeyboard(1, true, true)!
    const buttons = kb.inline_keyboard[0]
    expect(buttons.some(b => b.text.includes("Prev"))).toBe(true)
    expect(buttons.some(b => b.text.includes("Next"))).toBe(true)
  })

  it("Prev callback points to previous page", () => {
    const kb = buildNavKeyboard(2, true, true)!
    const prev = kb.inline_keyboard[0].find(b => b.text.includes("Prev"))!
    expect(prev.callback_data).toBe("myt:p:1")
  })

  it("Next callback points to next page", () => {
    const kb = buildNavKeyboard(1, true, true)!
    const next = kb.inline_keyboard[0].find(b => b.text.includes("Next"))!
    expect(next.callback_data).toBe("myt:p:2")
  })
})

describe("handleMyTickets (project picker)", () => {
  it("fetches projects and shows picker buttons filtered to configured keys", async () => {
    const ctx = makeCtx()
    const clients = makeClients()

    await handleMyTickets(ctx as never, clients as never)

    expect(clients.jira.getProjects).toHaveBeenCalled()
    const opts = ctx.reply.mock.calls[0][1] as { reply_markup?: { inline_keyboard: { callback_data: string }[][] } }
    const callbacks = opts?.reply_markup?.inline_keyboard?.flat().map(b => b.callback_data) ?? []
    expect(callbacks).toContain("myt:proj:MP")
    expect(callbacks).toContain("myt:proj:BZ")
  })

  it("project buttons include project key and name in text", async () => {
    const ctx = makeCtx()
    const clients = makeClients()

    await handleMyTickets(ctx as never, clients as never)

    const opts = ctx.reply.mock.calls[0][1] as { reply_markup?: { inline_keyboard: { text: string }[][] } }
    const texts = opts?.reply_markup?.inline_keyboard?.flat().map(b => b.text) ?? []
    expect(texts.some(t => t.includes("MP"))).toBe(true)
    expect(texts.some(t => t.includes("BZ"))).toBe(true)
  })

  it("getProjects failure → falls back to key-only picker buttons", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ projectsResult: new JiraAuthError() })

    await handleMyTickets(ctx as never, clients as never)

    // Fallback: still shows picker with configured keys
    const opts = ctx.reply.mock.calls[0][1] as { reply_markup?: { inline_keyboard: { callback_data: string }[][] } }
    const callbacks = opts?.reply_markup?.inline_keyboard?.flat().map(b => b.callback_data) ?? []
    expect(callbacks).toContain("myt:proj:MP")
    expect(callbacks).toContain("myt:proj:BZ")
  })

  it("generic getProjects failure → still shows picker (graceful degradation)", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ projectsResult: new Error("Network failure") })

    await handleMyTickets(ctx as never, clients as never)

    expect(ctx.reply).toHaveBeenCalled()
    const firstArg = ctx.reply.mock.calls[0][0] as string
    expect(firstArg).toBe("Select a project:")
  })

  it("skips project picker and shows status picker when only 1 project configured", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ projectKeys: ["MP"] })

    await handleMyTickets(ctx as never, clients as never)

    // getStatuses is called (from handleMyTicketsProject), not getProjects
    expect(clients.jira.getStatuses).toHaveBeenCalled()
    expect(clients.jira.getProjects).not.toHaveBeenCalled()
    const opts = ctx.reply.mock.calls[0][1] as { reply_markup?: { inline_keyboard: { callback_data: string }[][] } }
    const callbacks = opts?.reply_markup?.inline_keyboard?.flat().map(b => b.callback_data) ?? []
    expect(callbacks).toContain("myt:s:")
  })
})

describe("handleMyTicketsProject", () => {
  it("fetches statuses and shows status picker with All button", async () => {
    const ctx = makeCtx()
    const clients = makeClients()

    await handleMyTicketsProject(ctx as never, clients as never, "MP")

    expect(clients.jira.getStatuses).toHaveBeenCalled()
    const opts = ctx.reply.mock.calls[0][1] as { reply_markup?: { inline_keyboard: { callback_data: string }[][] } }
    const callbacks = opts?.reply_markup?.inline_keyboard?.flat().map(b => b.callback_data) ?? []
    expect(callbacks).toContain("myt:s:")
    expect(callbacks).toContain("myt:s:In Progress")
  })

  it("status buttons carry status name in callback_data", async () => {
    const ctx = makeCtx()
    const clients = makeClients()

    await handleMyTicketsProject(ctx as never, clients as never, "MP")

    const opts = ctx.reply.mock.calls[0][1] as { reply_markup?: { inline_keyboard: { callback_data: string }[][] } }
    const callbacks = opts?.reply_markup?.inline_keyboard?.flat().map(b => b.callback_data) ?? []
    expect(callbacks).toContain("myt:s:To Do")
    expect(callbacks).toContain("myt:s:Done")
  })

  it("shows project key in reply header", async () => {
    const ctx = makeCtx()
    const clients = makeClients()

    await handleMyTicketsProject(ctx as never, clients as never, "BZ")

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("BZ")
  })

  it("answers callback query", async () => {
    const ctx = { ...makeCtx(), callbackQuery: { id: "cq1" } }
    const clients = makeClients()

    await handleMyTicketsProject(ctx as never, clients as never, "MP")

    expect(ctx.answerCallbackQuery).toHaveBeenCalled()
  })

  it("JiraAuthError → replies with auth/token error message", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ statusesResult: new JiraAuthError() })

    await handleMyTicketsProject(ctx as never, clients as never, "MP")

    const reply = (ctx.reply.mock.calls[0][0] as string).toLowerCase()
    expect(reply).toMatch(/auth|token/)
  })
})

describe("handleMyTicketsStatus", () => {
  it("replies with no-tickets message when list is empty", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ listResult: { issues: [], nextPageToken: undefined } })

    await handleMyTicketsStatus(ctx as never, clients as never, "In Progress")

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply.toLowerCase()).toContain("no ticket")
  })

  it("calls getMyIssues with PAGE_SIZE, no token, status, and undefined projectKey when no project selected", async () => {
    const ctx = makeCtx(200)
    const clients = makeClients({ listResult: { issues: [makeIssue(1)], nextPageToken: undefined } })

    await handleMyTicketsStatus(ctx as never, clients as never, "In Progress")

    expect(clients.jira.getMyIssues).toHaveBeenCalledWith(PAGE_SIZE, undefined, "In Progress", undefined)
  })

  it("passes projectKey from projectCache when project was selected", async () => {
    const chatId = 201
    const ctx = makeCtx(chatId)
    const setupClients = makeClients()
    await handleMyTicketsProject(ctx as never, setupClients as never, "MP")

    const clients = makeClients({ listResult: { issues: [makeIssue(1)], nextPageToken: undefined } })
    await handleMyTicketsStatus(ctx as never, clients as never, "In Progress")

    expect(clients.jira.getMyIssues).toHaveBeenCalledWith(PAGE_SIZE, undefined, "In Progress", "MP")
  })

  it("passes undefined status to getMyIssues when empty string (All)", async () => {
    const ctx = makeCtx(202)
    const clients = makeClients({ listResult: { issues: [makeIssue(1)], nextPageToken: undefined } })

    await handleMyTicketsStatus(ctx as never, clients as never, "")

    expect(clients.jira.getMyIssues).toHaveBeenCalledWith(PAGE_SIZE, undefined, undefined, undefined)
  })

  it("sends HTML-formatted reply with bold title", async () => {
    const ctx = makeCtx(203)
    const clients = makeClients({ listResult: { issues: [makeIssue(1)], nextPageToken: undefined } })

    await handleMyTicketsStatus(ctx as never, clients as never, "")

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("<b>ENG-1")
    const opts = ctx.reply.mock.calls[0][1] as { parse_mode?: string }
    expect(opts?.parse_mode).toBe("HTML")
  })

  it("shows status label in header when status selected", async () => {
    const ctx = makeCtx(204)
    const clients = makeClients({ listResult: { issues: [makeIssue(1)], nextPageToken: undefined } })

    await handleMyTicketsStatus(ctx as never, clients as never, "In Progress")

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("In Progress")
  })

  it("adds nav keyboard when nextPageToken present", async () => {
    const ctx = makeCtx(205)
    const issues = Array.from({ length: PAGE_SIZE }, (_, i) => makeIssue(i + 1))
    const clients = makeClients({ listResult: { issues, nextPageToken: "cursor-abc" } })

    await handleMyTicketsStatus(ctx as never, clients as never, "")

    const opts = ctx.reply.mock.calls[0][1] as { reply_markup?: { inline_keyboard: unknown[][] } }
    expect(opts?.reply_markup?.inline_keyboard).toHaveLength(1)
  })
})

describe("handleMyTicketsPage", () => {
  it("calls getMyIssues with PAGE_SIZE", async () => {
    const ctx = makeCtx(1001)
    const issues = [makeIssue(6)]
    const clients = makeClients({ listResult: { issues, nextPageToken: undefined } })

    await handleMyTicketsPage(ctx as never, clients as never, 1)

    expect(clients.jira.getMyIssues).toHaveBeenCalledWith(PAGE_SIZE, undefined, undefined, undefined)
  })

  it("uses cached token when available", async () => {
    const chatId = 99
    const ctx = makeCtx(chatId)
    const setupClients = makeClients({
      listResult: { issues: Array.from({ length: PAGE_SIZE }, (_, i) => makeIssue(i + 1)), nextPageToken: "tok-page1" },
    })
    await handleMyTicketsStatus(ctx as never, setupClients as never, "")

    const pageClients = makeClients({ listResult: { issues: [makeIssue(6)], nextPageToken: undefined } })
    await handleMyTicketsPage(ctx as never, pageClients as never, 1)

    expect(pageClients.jira.getMyIssues).toHaveBeenCalledWith(PAGE_SIZE, "tok-page1", undefined, undefined)
  })

  it("calls editMessageText with HTML-formatted ticket list", async () => {
    const ctx = makeCtx()
    const issues = [makeIssue(6)]
    const clients = makeClients({ listResult: { issues, nextPageToken: undefined } })

    await handleMyTicketsPage(ctx as never, clients as never, 1)

    expect(ctx.editMessageText).toHaveBeenCalled()
    const text = ctx.editMessageText.mock.calls[0][0] as string
    expect(text).toContain("<b>ENG-6")
    const opts = ctx.editMessageText.mock.calls[0][1] as { parse_mode?: string }
    expect(opts?.parse_mode).toBe("HTML")
  })

  it("always calls answerCallbackQuery on success", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ listResult: { issues: [makeIssue(1)], nextPageToken: undefined } })

    await handleMyTicketsPage(ctx as never, clients as never, 0)

    expect(ctx.answerCallbackQuery).toHaveBeenCalled()
  })

  it("still calls answerCallbackQuery on error", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ listResult: new Error("Network failure") })

    await handleMyTicketsPage(ctx as never, clients as never, 1)

    expect(ctx.answerCallbackQuery).toHaveBeenCalled()
  })
})

describe("handleTicketDetails", () => {
  it("fetches issue by key and replies with HTML", async () => {
    const ctx = makeCtx()
    const issue = { ...makeIssue(1), description: "Some description" }
    const clients = makeClients({ issueResult: issue })

    await handleTicketDetails(ctx as never, clients as never, "ENG-1")

    expect(clients.jira.getIssue).toHaveBeenCalledWith("ENG-1")
    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("<b>ENG-1: Issue 1</b>")
    expect(reply).toContain("Some description")
    const opts = ctx.reply.mock.calls[0][1] as { parse_mode?: string }
    expect(opts?.parse_mode).toBe("HTML")
  })

  it("shows no-description fallback when description is empty", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ issueResult: makeIssue(1) })

    await handleTicketDetails(ctx as never, clients as never, "ENG-1")

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("<i>No description</i>")
  })

  it("includes Jira link", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ issueResult: makeIssue(1) })

    await handleTicketDetails(ctx as never, clients as never, "ENG-1")

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain(`href="https://jira.example.com/browse/ENG-1"`)
  })

  it("calls answerCallbackQuery when invoked as callback", async () => {
    const ctx = { ...makeCtx(), callbackQuery: { id: "cq1" } }
    const clients = makeClients({ issueResult: makeIssue(1) })

    await handleTicketDetails(ctx as never, clients as never, "ENG-1")

    expect(ctx.answerCallbackQuery).toHaveBeenCalled()
  })

  it("skips answerCallbackQuery when invoked from command (no callbackQuery)", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ issueResult: makeIssue(1) })

    await handleTicketDetails(ctx as never, clients as never, "ENG-1")

    expect(ctx.answerCallbackQuery).not.toHaveBeenCalled()
  })

  it("replies with error message on failure", async () => {
    const ctx = makeCtx()
    const clients = makeClients({ issueResult: new Error("Not found") })

    await handleTicketDetails(ctx as never, clients as never, "ENG-1")

    const reply = (ctx.reply.mock.calls[0][0] as string).toLowerCase()
    expect(reply).toContain("could not load")
  })

  it("escapes HTML in ticket description", async () => {
    const ctx = makeCtx()
    const issue = { ...makeIssue(1), description: "Fix <bug> & crash" }
    const clients = makeClients({ issueResult: issue })

    await handleTicketDetails(ctx as never, clients as never, "ENG-1")

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("Fix &lt;bug&gt; &amp; crash")
    expect(reply).not.toContain("<bug>")
  })
})
