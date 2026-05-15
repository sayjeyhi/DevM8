import { describe, it, expect, mock } from "bun:test"
import { handleComment } from "../../../src/bot/commands/comment"
import { JiraAuthError, JiraNotFoundError } from "../../../src/shared/errors"

function makeCtx(match: string) {
  return {
    match,
    reply: mock().mockResolvedValue({}),
    replyWithChatAction: mock().mockResolvedValue({}),
  }
}

type MockClients = {
  jira: { addComment: ReturnType<typeof mock> }
}

function makeClients(addCommentImpl: unknown = undefined): MockClients {
  return {
    jira: {
      addComment:
        addCommentImpl instanceof Error
          ? mock().mockRejectedValue(addCommentImpl)
          : mock().mockResolvedValue(addCommentImpl),
    },
  }
}

describe("handleComment", () => {
  it("valid args → addComment called with key and text preserving internal spaces, success reply", async () => {
    const ctx = makeCtx("ENG-1 Fixed the bug with   extra spaces")
    const clients = makeClients()

    await handleComment(ctx as never, clients as never)

    expect(clients.jira.addComment).toHaveBeenCalledWith("ENG-1", "Fixed the bug with   extra spaces")
    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("ENG-1")
  })

  it("sends typing action before API call", async () => {
    const ctx = makeCtx("ENG-1 Some comment")
    const clients = makeClients()

    await handleComment(ctx as never, clients as never)

    expect(ctx.replyWithChatAction).toHaveBeenCalledWith("typing")
  })

  it("no args → usage reply, no API calls", async () => {
    const ctx = makeCtx("")
    const clients = makeClients()

    await handleComment(ctx as never, clients as never)

    expect(clients.jira.addComment).not.toHaveBeenCalled()
    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply.toLowerCase()).toMatch(/usage|\/comment/)
  })

  it("key only, no comment text → usage reply, no API calls", async () => {
    const ctx = makeCtx("ENG-1")
    const clients = makeClients()

    await handleComment(ctx as never, clients as never)

    expect(clients.jira.addComment).not.toHaveBeenCalled()
    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply.toLowerCase()).toMatch(/usage|\/comment/)
  })

  it("JiraNotFoundError → reply contains issue key", async () => {
    const ctx = makeCtx("ENG-999 Some comment")
    const clients = makeClients(new JiraNotFoundError("ENG-999"))

    await handleComment(ctx as never, clients as never)

    const reply = ctx.reply.mock.calls[0][0] as string
    expect(reply).toContain("ENG-999")
  })

  it("JiraAuthError → reply contains auth/token message", async () => {
    const ctx = makeCtx("ENG-1 Some comment")
    const clients = makeClients(new JiraAuthError())

    await handleComment(ctx as never, clients as never)

    const reply = (ctx.reply.mock.calls[0][0] as string).toLowerCase()
    expect(reply).toMatch(/auth|token/)
  })

  it("generic error → 'something went wrong' reply", async () => {
    const ctx = makeCtx("ENG-1 Some comment")
    const clients = makeClients(new Error("Network failure"))

    await handleComment(ctx as never, clients as never)

    const reply = (ctx.reply.mock.calls[0][0] as string).toLowerCase()
    expect(reply).toContain("something went wrong")
  })
})
