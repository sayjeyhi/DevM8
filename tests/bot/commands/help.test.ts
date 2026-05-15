import { describe, it, expect, mock } from "bun:test"
import { handleHelp, HELP_TEXT } from "../../../src/bot/commands/help"

function makeCtx() {
  return {
    reply: mock().mockResolvedValue({}),
  }
}

describe("HELP_TEXT", () => {
  it("contains /create", () => {
    expect(HELP_TEXT).toContain("/create")
  })

  it("contains /move", () => {
    expect(HELP_TEXT).toContain("/move")
  })

  it("contains /comment", () => {
    expect(HELP_TEXT).toContain("/comment")
  })

  it("contains /solve", () => {
    expect(HELP_TEXT).toContain("/solve")
  })

  it("contains /help", () => {
    expect(HELP_TEXT).toContain("/help")
  })
})

describe("handleHelp", () => {
  it("replies with HELP_TEXT", async () => {
    const ctx = makeCtx()

    await handleHelp(ctx as never)

    expect(ctx.reply).toHaveBeenCalledWith(HELP_TEXT)
  })

  it("makes no API calls — pure reply", async () => {
    const ctx = makeCtx()

    await handleHelp(ctx as never)

    expect(ctx.reply).toHaveBeenCalledTimes(1)
  })
})
