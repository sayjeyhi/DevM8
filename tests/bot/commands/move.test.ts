import { describe, it, expect, mock } from "bun:test"
import { handleMove } from "../../../src/bot/commands/move"
import { InvalidTransitionError, JiraAuthError, JiraNotFoundError } from "../../../src/shared/errors"

function makeCtx(match: string) {
  return {
    match,
    reply: mock().mockResolvedValue({}),
    replyWithChatAction: mock().mockResolvedValue({}),
  }
}

type MockClients = {
  jira: { transitionIssue: ReturnType<typeof mock> }
}

function makeClients(transitionImpl: unknown = undefined): MockClients {
  return {
    jira: {
      transitionIssue:
        transitionImpl instanceof Error
          ? mock().mockRejectedValue(transitionImpl)
          : mock().mockResolvedValue(transitionImpl),
    },
  }
}

describe("handleMove", () => {
  it("valid args → transitionIssue called, reply contains key and status", async () => {
    const ctx = makeCtx("ENG-1 In Progress")
    const clients = makeClients()

    await handleMove(ctx as never, clients as never)

    expect(clients.jira.transitionIssue).toHaveBeenCalledWith("ENG-1", "In Progress")
    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("ENG-1")
    expect(reply).toContain("In Progress")
  })

  it("sends typing action before API call", async () => {
    const ctx = makeCtx("ENG-1 In Progress")
    const clients = makeClients()

    await handleMove(ctx as never, clients as never)

    expect(ctx.replyWithChatAction).toHaveBeenCalledWith("typing")
  })

  it("multi-word status passed as single string to transitionIssue", async () => {
    const ctx = makeCtx("ENG-1 In Progress")
    const clients = makeClients()

    await handleMove(ctx as never, clients as never)

    const [, status] = clients.jira.transitionIssue.mock.calls[0] as [string, string]
    expect(status).toBe("In Progress")
  })

  it("no args → usage reply, no API calls", async () => {
    const ctx = makeCtx("")
    const clients = makeClients()

    await handleMove(ctx as never, clients as never)

    expect(clients.jira.transitionIssue).not.toHaveBeenCalled()
    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply.toLowerCase()).toMatch(/usage|\/move/)
  })

  it("key only, no status → usage reply, no API calls", async () => {
    const ctx = makeCtx("ENG-1")
    const clients = makeClients()

    await handleMove(ctx as never, clients as never)

    expect(clients.jira.transitionIssue).not.toHaveBeenCalled()
    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply.toLowerCase()).toMatch(/usage|\/move/)
  })

  it("InvalidTransitionError → reply contains Available: and transition names", async () => {
    const ctx = makeCtx("ENG-1 Unknown Status")
    const err = new InvalidTransitionError("Unknown Status", ["To Do", "Done"])
    const clients = makeClients(err)

    await handleMove(ctx as never, clients as never)

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("Available:")
    expect(reply).toContain("To Do")
    expect(reply).toContain("Done")
  })

  it("JiraNotFoundError → reply contains issue key", async () => {
    const ctx = makeCtx("ENG-999 Done")
    const clients = makeClients(new JiraNotFoundError("ENG-999"))

    await handleMove(ctx as never, clients as never)

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("ENG-999")
  })

  it("JiraAuthError → reply contains auth/token message", async () => {
    const ctx = makeCtx("ENG-1 Done")
    const clients = makeClients(new JiraAuthError())

    await handleMove(ctx as never, clients as never)

    const reply = (ctx.reply.mock.calls[0][0] as string).toLowerCase()
    expect(reply).toMatch(/auth|token/)
  })

  it("generic error → 'something went wrong' reply, error logged", async () => {
    const ctx = makeCtx("ENG-1 Done")
    const clients = makeClients(new Error("Network failure"))

    await handleMove(ctx as never, clients as never)

    const reply = (ctx.reply.mock.calls[0][0] as string).toLowerCase()
    expect(reply).toContain("something went wrong")
  })
})
