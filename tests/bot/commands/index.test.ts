import { describe, it, expect, mock } from "bun:test"

// Mock @grammyjs/commands before importing registerCommands to avoid CJS/ESM
// interop conflict with grammy's Composer when running the full bun test suite.
const MockCommandGroup = mock().mockImplementation(() => ({
  command: mock(),
  setCommands: mock().mockResolvedValue(undefined),
  middleware: mock().mockReturnValue(mock()),
}))

mock.module("@grammyjs/commands", () => ({ CommandGroup: MockCommandGroup }))

const { registerCommands } = await import("../../../src/bot/commands/index")
import type { Clients } from "../../../src/bot/commands/index"

function makeBot() {
  return {
    use: mock().mockReturnValue(undefined),
    api: {
      setMyCommands: mock().mockResolvedValue(true),
      raw: {
        setMyCommands: mock().mockResolvedValue({ ok: true, result: true }),
      },
    },
  }
}

function makeClients(): Clients {
  return {
    jira: {
      createIssue: mock().mockResolvedValue({ key: "ENG-1", summary: "t", status: "To Do", description: "", url: "" }),
      getIssue: mock().mockResolvedValue({ key: "ENG-1", summary: "t", status: "To Do", description: "", url: "" }),
      transitionIssue: mock().mockResolvedValue(undefined),
      addComment: mock().mockResolvedValue(undefined),
    } as unknown as Clients["jira"],
    claude: {
      ask: mock().mockResolvedValue("Claude response"),
    } as unknown as Clients["claude"],
  }
}

describe("registerCommands", () => {
  it("calls bot.use() once to install command dispatch", async () => {
    const bot = makeBot()
    const clients = makeClients()

    await registerCommands(bot as never, clients)

    expect(bot.use).toHaveBeenCalledTimes(1)
  })

  it("bot.use receives a CommandGroup (has middleware() method)", async () => {
    const bot = makeBot()
    const clients = makeClients()

    await registerCommands(bot as never, clients)

    const arg = bot.use.mock.calls[0][0]
    expect(typeof (arg as { middleware?: unknown }).middleware).toBe("function")
  })

  it("registers all 5 commands: create, move, comment, solve, help", async () => {
    const commandInstance = {
      command: mock(),
      setCommands: mock().mockResolvedValue(undefined),
      middleware: mock().mockReturnValue(mock()),
    }
    MockCommandGroup.mockImplementationOnce(() => commandInstance)

    const bot = makeBot()
    const clients = makeClients()

    await registerCommands(bot as never, clients)

    const names = (commandInstance.command.mock.calls as [string][]).map(c => c[0])
    expect(names).toContain("create")
    expect(names).toContain("move")
    expect(names).toContain("comment")
    expect(names).toContain("solve")
    expect(names).toContain("help")
  })

  it("calls setCommands to sync Telegram command menu", async () => {
    const setCommands = mock().mockResolvedValue(undefined)
    const commandInstance = {
      command: mock(),
      setCommands,
      middleware: mock().mockReturnValue(mock()),
    }
    MockCommandGroup.mockImplementationOnce(() => commandInstance)

    const bot = makeBot()
    const clients = makeClients()

    await registerCommands(bot as never, clients)

    expect(setCommands).toHaveBeenCalledWith(bot)
  })

  it("setCommands failure is non-fatal — bot.use still called", async () => {
    const commandInstance = {
      command: mock(),
      setCommands: mock().mockRejectedValue(new Error("Telegram API unreachable")),
      middleware: mock().mockReturnValue(mock()),
    }
    MockCommandGroup.mockImplementationOnce(() => commandInstance)

    const bot = makeBot()
    const clients = makeClients()

    await expect(registerCommands(bot as never, clients)).resolves.toBeUndefined()
    expect(bot.use).toHaveBeenCalledTimes(1)
  })
})
