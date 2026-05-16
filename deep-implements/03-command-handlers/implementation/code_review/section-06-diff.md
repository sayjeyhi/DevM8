diff --git a/src/bot/commands/solve.ts b/src/bot/commands/solve.ts
index 336ce12..8b5c724 100644
--- a/src/bot/commands/solve.ts
+++ b/src/bot/commands/solve.ts
@@ -1 +1,87 @@
-export {}
+import type { Context } from "grammy"
+import { JiraAuthError, JiraNotFoundError, ClaudeTimeoutError, ClaudeExitError } from "../../shared/errors"
+import { parseArgs } from "../utils/parseArgs"
+import { splitMessage } from "../utils/splitMessage"
+
+// TODO: replace with import from '../commands/index' once section-07 is complete
+interface Clients {
+  jira: {
+    getIssue(key: string): Promise<{ key: string; summary: string; status: string; description: string }>
+  }
+  claude: {
+    ask(prompt: string): Promise<string>
+  }
+}
+
+export const SOLVE_PROMPT_TEMPLATE = `You are a software engineer analyzing a Jira issue. Provide actionable next steps or a solution approach.
+
+<key>{KEY}</key>
+<title>{TITLE}</title>
+<status>{STATUS}</status>
+<description>{DESCRIPTION}</description>
+
+Analyze the issue and respond with:
+1. A brief assessment of the problem
+2. Concrete next steps or a solution approach
+3. Any potential blockers or risks to consider
+
+Be concise and practical.`
+
+export async function handleSolve(ctx: Context, clients: Clients): Promise<void> {
+  const args = parseArgs(ctx)
+  const key = args[0]
+
+  if (!key) {
+    await ctx.reply("Usage: /solve <ticket-key>")
+    return
+  }
+
+  await ctx.reply(`Analyzing ${key} with Claude...`)
+
+  await ctx.replyWithChatAction("typing")
+  const typingInterval = setInterval(() => {
+    ctx.replyWithChatAction("typing").catch(() => {})
+  }, 4000)
+
+  try {
+    const issue = await clients.jira.getIssue(key)
+
+    // Replace {PLACEHOLDER} tokens with issue fields wrapped in XML delimiters.
+    // Description content is placed inside tags (not concatenated raw) to signal
+    // to Claude that it is data, not instructions — prompt injection defense.
+    const prompt = SOLVE_PROMPT_TEMPLATE
+      .replace("{KEY}", issue.key)
+      .replace("{TITLE}", issue.summary)
+      .replace("{STATUS}", issue.status)
+      .replace("{DESCRIPTION}", issue.description)
+
+    const response = await clients.claude.ask(prompt)
+
+    const chunks = splitMessage(response)
+    for (const chunk of chunks) {
+      await ctx.reply(chunk)
+    }
+  } catch (err) {
+    if (err instanceof JiraNotFoundError) {
+      await ctx.reply(`Ticket ${key} not found.`)
+      return
+    }
+    if (err instanceof JiraAuthError) {
+      await ctx.reply("Jira authentication failed. Check your API token.")
+      return
+    }
+    if (err instanceof ClaudeTimeoutError) {
+      await ctx.reply("Claude timed out. Please try again.")
+      return
+    }
+    if (err instanceof ClaudeExitError) {
+      await ctx.reply("Claude returned an error. Please try again.")
+      return
+    }
+    const message = err instanceof Error ? err.message : String(err)
+    console.log({ event: "error", command: "solve", errorMessage: message })
+    await ctx.reply("Something went wrong. Please try again.")
+  } finally {
+    clearInterval(typingInterval)
+  }
+}
diff --git a/tests/bot/commands/solve.test.ts b/tests/bot/commands/solve.test.ts
new file mode 100644
index 0000000..7cf24f2
--- /dev/null
+++ b/tests/bot/commands/solve.test.ts
@@ -0,0 +1,186 @@
+import { describe, it, expect, mock } from "bun:test"
+import { handleSolve, SOLVE_PROMPT_TEMPLATE } from "../../../src/bot/commands/solve"
+import { JiraAuthError, JiraNotFoundError, ClaudeTimeoutError, ClaudeExitError } from "../../../src/shared/errors"
+
+function makeCtx(match: string) {
+  return {
+    match,
+    reply: mock().mockResolvedValue({}),
+    replyWithChatAction: mock().mockResolvedValue({}),
+  }
+}
+
+const MOCK_ISSUE = {
+  key: "ENG-1",
+  summary: "Fix login bug",
+  status: "In Progress",
+  description: "Users cannot log in.",
+  url: "https://test.atlassian.net/browse/ENG-1",
+}
+
+type MockClients = {
+  jira: { getIssue: ReturnType<typeof mock> }
+  claude: { ask: ReturnType<typeof mock> }
+}
+
+function makeClients(
+  getIssueImpl: unknown = MOCK_ISSUE,
+  askImpl: unknown = "Here is the solution.",
+): MockClients {
+  return {
+    jira: {
+      getIssue:
+        getIssueImpl instanceof Error
+          ? mock().mockRejectedValue(getIssueImpl)
+          : mock().mockResolvedValue(getIssueImpl),
+    },
+    claude: {
+      ask:
+        askImpl instanceof Error
+          ? mock().mockRejectedValue(askImpl)
+          : mock().mockResolvedValue(askImpl),
+    },
+  }
+}
+
+describe("SOLVE_PROMPT_TEMPLATE", () => {
+  it("contains <key> XML delimiter", () => {
+    expect(SOLVE_PROMPT_TEMPLATE).toContain("<key>")
+  })
+
+  it("contains <title> XML delimiter", () => {
+    expect(SOLVE_PROMPT_TEMPLATE).toContain("<title>")
+  })
+
+  it("contains <status> XML delimiter", () => {
+    expect(SOLVE_PROMPT_TEMPLATE).toContain("<status>")
+  })
+
+  it("contains <description> XML delimiter", () => {
+    expect(SOLVE_PROMPT_TEMPLATE).toContain("<description>")
+  })
+})
+
+describe("handleSolve", () => {
+  it("no args → usage reply, no API calls", async () => {
+    const ctx = makeCtx("")
+    const clients = makeClients()
+
+    await handleSolve(ctx as never, clients as never)
+
+    expect(clients.jira.getIssue).not.toHaveBeenCalled()
+    expect(clients.claude.ask).not.toHaveBeenCalled()
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply.toLowerCase()).toMatch(/usage|\/solve/)
+  })
+
+  it("sends intermediate 'Analyzing…' reply before calling getIssue", async () => {
+    const ctx = makeCtx("ENG-1")
+    const clients = makeClients()
+
+    await handleSolve(ctx as never, clients as never)
+
+    const firstReply = ctx.reply.mock.calls[0][0] as string
+    expect(firstReply.toLowerCase()).toContain("analyzing")
+    expect(firstReply).toContain("ENG-1")
+    expect(clients.jira.getIssue).toHaveBeenCalledWith("ENG-1")
+  })
+
+  it("calls ClaudeClient.ask after fetching issue, final reply contains response", async () => {
+    const ctx = makeCtx("ENG-1")
+    const clients = makeClients()
+
+    await handleSolve(ctx as never, clients as never)
+
+    expect(clients.claude.ask).toHaveBeenCalledTimes(1)
+    const replies = ctx.reply.mock.calls.map(c => c[0] as string)
+    expect(replies.some(r => r.includes("Here is the solution."))).toBe(true)
+  })
+
+  it("sends single content reply (no [N/M] prefix) for short Claude response", async () => {
+    const ctx = makeCtx("ENG-1")
+    const clients = makeClients(MOCK_ISSUE, "Short response.")
+
+    await handleSolve(ctx as never, clients as never)
+
+    expect(ctx.reply.mock.calls.length).toBe(2) // intermediate + content
+    const contentReply = ctx.reply.mock.calls[1][0] as string
+    expect(contentReply).not.toMatch(/^\[\d+\/\d+\]/)
+  })
+
+  it("sends multiple [N/M]-prefixed replies for long Claude response (>4096 chars)", async () => {
+    const ctx = makeCtx("ENG-1")
+    const longResponse = "word ".repeat(5000) // ~25,000 chars
+    const clients = makeClients(MOCK_ISSUE, longResponse)
+
+    await handleSolve(ctx as never, clients as never)
+
+    expect(ctx.reply.mock.calls.length).toBeGreaterThan(2)
+    const contentReplies = ctx.reply.mock.calls.slice(1).map(c => c[0] as string)
+    for (const r of contentReplies) {
+      expect(r).toMatch(/^\[\d+\/\d+\]/)
+    }
+  })
+
+  it("JiraNotFoundError → reply contains key, Claude not called", async () => {
+    const ctx = makeCtx("ENG-999")
+    const clients = makeClients(new JiraNotFoundError("ENG-999"))
+
+    await handleSolve(ctx as never, clients as never)
+
+    expect(clients.claude.ask).not.toHaveBeenCalled()
+    const replies = ctx.reply.mock.calls.map(c => c[0] as string)
+    expect(replies.some(r => r.includes("ENG-999"))).toBe(true)
+  })
+
+  it("JiraAuthError → reply contains auth/token mention", async () => {
+    const ctx = makeCtx("ENG-1")
+    const clients = makeClients(new JiraAuthError())
+
+    await handleSolve(ctx as never, clients as never)
+
+    const replies = ctx.reply.mock.calls.map(c => c[0] as string)
+    expect(replies.some(r => r.toLowerCase().match(/auth|token/))).toBe(true)
+  })
+
+  it("ClaudeTimeoutError → reply contains 'timed out'", async () => {
+    const ctx = makeCtx("ENG-1")
+    const clients = makeClients(MOCK_ISSUE, new ClaudeTimeoutError(30000))
+
+    await handleSolve(ctx as never, clients as never)
+
+    const replies = ctx.reply.mock.calls.map(c => c[0] as string)
+    expect(replies.some(r => r.toLowerCase().includes("timed out"))).toBe(true)
+  })
+
+  it("ClaudeExitError → reply contains 'error'", async () => {
+    const ctx = makeCtx("ENG-1")
+    const clients = makeClients(MOCK_ISSUE, new ClaudeExitError(1, "stderr"))
+
+    await handleSolve(ctx as never, clients as never)
+
+    const replies = ctx.reply.mock.calls.map(c => c[0] as string)
+    expect(replies.some(r => r.toLowerCase().includes("error"))).toBe(true)
+  })
+
+  it("sends typing action before API call", async () => {
+    const ctx = makeCtx("ENG-1")
+    const clients = makeClients()
+
+    await handleSolve(ctx as never, clients as never)
+
+    expect(ctx.replyWithChatAction).toHaveBeenCalledWith("typing")
+  })
+
+  it("handles Jira description of 10,000+ chars without crashing", async () => {
+    const ctx = makeCtx("ENG-1")
+    const bigIssue = { ...MOCK_ISSUE, description: "x".repeat(12000) }
+    const clients = makeClients(bigIssue)
+
+    await handleSolve(ctx as never, clients as never)
+
+    expect(clients.claude.ask).toHaveBeenCalledTimes(1)
+    const prompt = clients.claude.ask.mock.calls[0][0] as string
+    expect(prompt.length).toBeGreaterThan(10000)
+  })
+})
