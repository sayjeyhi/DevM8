diff --git a/src/bot/bot.ts b/src/bot/bot.ts
index 336ce12..5fea01b 100644
--- a/src/bot/bot.ts
+++ b/src/bot/bot.ts
@@ -1 +1,76 @@
-export {}
+import { Bot } from "grammy"
+import { apiThrottler } from "@grammyjs/transformer-throttler"
+import { autoRetry } from "@grammyjs/auto-retry"
+import { loadConfig } from "./config"
+import { createAuthMiddleware } from "./middleware/auth"
+import { registerCommands } from "./commands/index"
+import { JiraClient } from "../jira/JiraClient"
+import { ClaudeClient } from "../claude/ClaudeClient"
+
+export async function startBot(): Promise<void> {
+  const config = loadConfig()
+
+  // Make API key available to the claude CLI subprocess (read from process.env on spawn)
+  process.env.ANTHROPIC_API_KEY = config.claudeApiKey
+
+  const jiraHost = new URL(config.jiraBaseUrl).host
+
+  const logger = {
+    info: (obj: object) => console.log(obj),
+    error: (obj: object) => console.error(obj),
+  }
+
+  const jira = new JiraClient(
+    {
+      host: jiraHost,
+      email: config.jiraUserEmail,
+      apiToken: config.jiraApiToken,
+      projectKey: config.jiraProjectKey,
+    },
+    logger,
+  )
+
+  const claude = new ClaudeClient(
+    {
+      binaryPath: process.env.CLAUDE_BINARY_PATH ?? "claude",
+    },
+    logger,
+  )
+
+  const bot = new Bot(config.telegramBotToken)
+
+  // Rate-limit and retry transformers must be on bot.api (outbound calls), not on bot (inbound middleware)
+  bot.api.config.use(apiThrottler())
+  bot.api.config.use(autoRetry())
+
+  bot.use(createAuthMiddleware(config.allowedUserIds))
+
+  await registerCommands(bot, { jira, claude })
+
+  // Unknown command fallback — must be registered after all command handlers
+  bot.on("message", ctx => ctx.reply("Unknown command. Try /help"))
+
+  bot.catch(err => {
+    const error = err.error as Error & { type?: string }
+    // Log sanitized error — never log full error object (may embed Authorization headers in cause)
+    console.log({
+      event: "error",
+      command: err.ctx.message?.text?.split(" ")[0],
+      errorMessage: error instanceof Error ? error.message : String(error),
+      errorType: error.type ?? "unknown",
+    })
+    err.ctx.reply("An unexpected error occurred. Please try again.").catch(() => {})
+  })
+
+  // Graceful shutdown: clear in-flight polling before process exit to avoid duplicate delivery
+  process.on("SIGTERM", async () => {
+    await bot.stop()
+  })
+  process.on("SIGINT", async () => {
+    await bot.stop()
+  })
+
+  await bot.start()
+}
+
+startBot().catch(console.error)
diff --git a/src/bot/commands/index.ts b/src/bot/commands/index.ts
index 336ce12..dcc1d84 100644
--- a/src/bot/commands/index.ts
+++ b/src/bot/commands/index.ts
@@ -1 +1,27 @@
-export {}
+import type { Bot, Context } from "grammy"
+import { CommandGroup } from "@grammyjs/commands"
+import type { JiraClient } from "../../jira/JiraClient"
+import type { ClaudeClient } from "../../claude/ClaudeClient"
+import { handleCreate } from "./create"
+import { handleMove } from "./move"
+import { handleComment } from "./comment"
+import { handleHelp } from "./help"
+import { handleSolve } from "./solve"
+
+export interface Clients {
+  jira: JiraClient
+  claude: ClaudeClient
+}
+
+export async function registerCommands(bot: Bot, clients: Clients): Promise<void> {
+  const commands = new CommandGroup<Context>()
+
+  commands.command("create", "Create a new Jira ticket", ctx => handleCreate(ctx, clients))
+  commands.command("move", "Move a ticket to a new status", ctx => handleMove(ctx, clients))
+  commands.command("comment", "Add a comment to a ticket", ctx => handleComment(ctx, clients))
+  commands.command("solve", "Ask Claude for a solution to a ticket", ctx => handleSolve(ctx, clients))
+  commands.command("help", "Show available commands", ctx => handleHelp(ctx))
+
+  bot.use(commands)
+  await commands.setCommands(bot)
+}
diff --git a/tests/bot/commands/index.test.ts b/tests/bot/commands/index.test.ts
new file mode 100644
index 0000000..4239709
--- /dev/null
+++ b/tests/bot/commands/index.test.ts
@@ -0,0 +1,126 @@
+import { describe, it, expect, mock, beforeAll } from "bun:test"
+
+// Mock @grammyjs/commands before importing registerCommands to avoid CJS/ESM
+// interop conflict with grammy's Composer when running the full test suite.
+const mockCommandMethods: Record<string, ReturnType<typeof mock>> = {}
+const MockCommandGroup = mock().mockImplementation(() => ({
+  command: mock().mockImplementation((_name: string, _desc: string, handler: unknown) => {
+    mockCommandMethods[_name as string] = handler as ReturnType<typeof mock>
+    return {}
+  }),
+  setCommands: mock().mockResolvedValue(undefined),
+  middleware: mock().mockReturnValue(mock()),
+}))
+
+mock.module("@grammyjs/commands", () => ({ CommandGroup: MockCommandGroup }))
+
+const { registerCommands } = await import("../../../src/bot/commands/index")
+import type { Clients } from "../../../src/bot/commands/index"
+
+function makeBot() {
+  return {
+    use: mock().mockReturnValue(undefined),
+    api: {
+      setMyCommands: mock().mockResolvedValue(true),
+      raw: {
+        setMyCommands: mock().mockResolvedValue({ ok: true, result: true }),
+      },
+    },
+  }
+}
+
+function makeClients(): Clients {
+  return {
+    jira: {
+      createIssue: mock().mockResolvedValue({ key: "ENG-1", summary: "t", status: "To Do", description: "", url: "" }),
+      getIssue: mock().mockResolvedValue({ key: "ENG-1", summary: "t", status: "To Do", description: "", url: "" }),
+      transitionIssue: mock().mockResolvedValue(undefined),
+      addComment: mock().mockResolvedValue(undefined),
+    } as unknown as Clients["jira"],
+    claude: {
+      ask: mock().mockResolvedValue("Claude response"),
+    } as unknown as Clients["claude"],
+  }
+}
+
+describe("registerCommands", () => {
+  it("calls bot.use() once to install command dispatch", async () => {
+    const bot = makeBot()
+    const clients = makeClients()
+
+    await registerCommands(bot as never, clients)
+
+    expect(bot.use).toHaveBeenCalledTimes(1)
+  })
+
+  it("bot.use receives an object with middleware() method (CommandGroup)", async () => {
+    const bot = makeBot()
+    const clients = makeClients()
+
+    await registerCommands(bot as never, clients)
+
+    const arg = bot.use.mock.calls[0][0]
+    expect(typeof (arg as { middleware?: unknown }).middleware).toBe("function")
+  })
+
+  it("registers all 5 commands (create, move, comment, solve, help)", async () => {
+    const commandInstance = {
+      command: mock(),
+      setCommands: mock().mockResolvedValue(undefined),
+      middleware: mock().mockReturnValue(mock()),
+    }
+    MockCommandGroup.mockImplementationOnce(() => commandInstance)
+
+    const bot = makeBot()
+    const clients = makeClients()
+
+    await registerCommands(bot as never, clients)
+
+    const names = (commandInstance.command.mock.calls as [string][]).map(c => c[0])
+    expect(names).toContain("create")
+    expect(names).toContain("move")
+    expect(names).toContain("comment")
+    expect(names).toContain("solve")
+    expect(names).toContain("help")
+  })
+
+  it("calls setCommands to sync Telegram command menu", async () => {
+    const setCommands = mock().mockResolvedValue(undefined)
+    const commandInstance = {
+      command: mock(),
+      setCommands,
+      middleware: mock().mockReturnValue(mock()),
+    }
+    MockCommandGroup.mockImplementationOnce(() => commandInstance)
+
+    const bot = makeBot()
+    const clients = makeClients()
+
+    await registerCommands(bot as never, clients)
+
+    expect(setCommands).toHaveBeenCalledWith(bot)
+  })
+})
+
+describe("Clients interface", () => {
+  it("Clients has jira and claude fields", () => {
+    const clients = makeClients()
+    expect(clients.jira).toBeDefined()
+    expect(clients.claude).toBeDefined()
+  })
+
+  it("clients.jira has required Jira methods", () => {
+    const clients = makeClients()
+    const jira = clients.jira as unknown as Record<string, unknown>
+    expect(typeof jira.createIssue).toBe("function")
+    expect(typeof jira.getIssue).toBe("function")
+    expect(typeof jira.transitionIssue).toBe("function")
+    expect(typeof jira.addComment).toBe("function")
+  })
+
+  it("clients.claude has ask method", () => {
+    const clients = makeClients()
+    const claude = clients.claude as unknown as Record<string, unknown>
+    expect(typeof claude.ask).toBe("function")
+  })
+})
