import { describe, it, expect, mock, beforeEach } from "bun:test"
import { handleCreate, ENRICH_PROMPT_TEMPLATE, EXPAND_PROMPT_TEMPLATE } from "../../../src/bot/commands/create"
import { JiraAuthError } from "../../../src/shared/errors"

function makeCtx(match: string) {
  return {
    match,
    reply: mock().mockResolvedValue({}),
    replyWithChatAction: mock().mockResolvedValue({}),
  }
}

type MockClients = {
  jira: { createIssue: ReturnType<typeof mock> }
  claude: { ask: ReturnType<typeof mock> }
}

function makeClients(
  createIssueImpl: unknown = { key: "ENG-1" },
  askImpl: unknown = "Claude enriched description",
): MockClients {
  return {
    jira: {
      createIssue:
        createIssueImpl instanceof Error
          ? mock().mockRejectedValue(createIssueImpl)
          : mock().mockResolvedValue(createIssueImpl),
    },
    claude: {
      ask:
        askImpl instanceof Error
          ? mock().mockRejectedValue(askImpl)
          : mock().mockResolvedValue(askImpl),
    },
  }
}

// ─── Template assertions ───────────────────────────────────────────────────

describe("ENRICH_PROMPT_TEMPLATE", () => {
  it("contains <title> and <description> XML delimiters", () => {
    expect(ENRICH_PROMPT_TEMPLATE).toContain("<title>")
    expect(ENRICH_PROMPT_TEMPLATE).toContain("<description>")
  })
})

describe("EXPAND_PROMPT_TEMPLATE", () => {
  it("contains <title> XML delimiter", () => {
    expect(EXPAND_PROMPT_TEMPLATE).toContain("<title>")
  })

  it("instructs Claude to write acceptance criteria", () => {
    expect(EXPAND_PROMPT_TEMPLATE.toLowerCase()).toContain("acceptance criteria")
  })
})

// ─── Handler tests ──────────────────────────────────────────────────────────

describe("handleCreate", () => {
  it("enrich path: -- separator → Claude called with title+description, issue created", async () => {
    const ctx = makeCtx("Fix login timeout -- auth expires too early")
    const clients = makeClients({ key: "ENG-99" })

    await handleCreate(ctx as never, clients as never)

    expect(clients.claude.ask).toHaveBeenCalledTimes(1)
    const prompt = clients.claude.ask.mock.calls[0][0] as string
    expect(prompt).toContain("Fix login timeout")
    expect(prompt).toContain("auth expires too early")

    expect(clients.jira.createIssue).toHaveBeenCalledTimes(1)

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("Created:")
    expect(reply).toContain("ENG-99")
  })

  it("expand path: no -- separator → Claude called with title only, issue created", async () => {
    const ctx = makeCtx("Fix login timeout")
    const clients = makeClients({ key: "ENG-42" })

    await handleCreate(ctx as never, clients as never)

    expect(clients.claude.ask).toHaveBeenCalledTimes(1)
    const prompt = clients.claude.ask.mock.calls[0][0] as string
    expect(prompt).toContain("Fix login timeout")

    expect(clients.jira.createIssue).toHaveBeenCalledTimes(1)
    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("Created:")
  })

  it("enrich path: Claude fails → raw description passed to Jira, issue still created", async () => {
    const ctx = makeCtx("Fix login timeout -- auth expires too early")
    const clients = makeClients({ key: "ENG-1" }, new Error("Claude down"))

    await handleCreate(ctx as never, clients as never)

    expect(clients.jira.createIssue).toHaveBeenCalledTimes(1)
    const [, desc] = clients.jira.createIssue.mock.calls[0] as [string, string]
    expect(desc).toBe("auth expires too early")

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("Created:")
  })

  it("expand path: Claude fails → empty description passed to Jira, issue still created", async () => {
    const ctx = makeCtx("Fix login timeout")
    const clients = makeClients({ key: "ENG-1" }, new Error("Claude down"))

    await handleCreate(ctx as never, clients as never)

    expect(clients.jira.createIssue).toHaveBeenCalledTimes(1)
    const [, desc] = clients.jira.createIssue.mock.calls[0] as [string, string]
    expect(desc).toBe("")

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("Created:")
  })

  it("no args → usage reply, no API calls", async () => {
    const ctx = makeCtx("")
    const clients = makeClients()

    await handleCreate(ctx as never, clients as never)

    expect(clients.jira.createIssue).not.toHaveBeenCalled()
    expect(clients.claude.ask).not.toHaveBeenCalled()
    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply.toLowerCase()).toMatch(/usage|\/create/)
  })

  it("JiraAuthError → reply contains auth/token message", async () => {
    const ctx = makeCtx("Fix something")
    const clients = makeClients(new JiraAuthError())

    await handleCreate(ctx as never, clients as never)

    const reply = (ctx.reply.mock.calls[0][0] as string).toLowerCase()
    expect(reply).toMatch(/auth|token/)
  })

  it("JiraNotFoundError → reply sent, no crash", async () => {
    const ctx = makeCtx("Fix something")
    const { JiraNotFoundError } = await import("../../../src/shared/errors")
    const clients = makeClients(new JiraNotFoundError("PROJ-1"))

    await handleCreate(ctx as never, clients as never)

    expect(ctx.reply).toHaveBeenCalledTimes(1)
  })

  it("enrich path: title trimmed after -- split", async () => {
    const ctx = makeCtx("Fix login timeout  --  auth issue")
    const clients = makeClients({ key: "ENG-1" }, new Error("Claude down"))

    await handleCreate(ctx as never, clients as never)

    const [title] = clients.jira.createIssue.mock.calls[0] as [string, string]
    expect(title).toBe("Fix login timeout")
  })
})
