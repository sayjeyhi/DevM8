diff --git a/src/bot/commands/create.ts b/src/bot/commands/create.ts
index 336ce12..cc78462 100644
--- a/src/bot/commands/create.ts
+++ b/src/bot/commands/create.ts
@@ -1 +1,89 @@
-export {}
+import type { Context } from "grammy"
+import { JiraAuthError, JiraNotFoundError } from "../../shared/errors"
+
+interface Clients {
+  jira: {
+    createIssue(title: string, description: string): Promise<{ key: string }>
+  }
+  claude: {
+    ask(prompt: string): Promise<string>
+  }
+}
+
+export const ENRICH_PROMPT_TEMPLATE = `You are a Jira ticket writer. Given the title and description provided by the user, write a well-formatted Jira ticket description in plain text.
+
+<title>{title}</title>
+<description>{description}</description>
+
+Return only the formatted description. No preamble, no metadata.`
+
+export const EXPAND_PROMPT_TEMPLATE = `You are a Jira ticket writer. Given the title provided by the user, write a Jira ticket description with 3-5 sentences and 3-5 acceptance criteria bullet points.
+
+<title>{title}</title>
+
+Return only the formatted description. No preamble, no metadata.`
+
+export async function handleCreate(ctx: Context, clients: Clients): Promise<void> {
+  const match = ((ctx.match as string) ?? "").trim()
+
+  if (!match) {
+    await ctx.reply("Usage: /create <title> [-- <description>]")
+    return
+  }
+
+  await ctx.replyWithChatAction("typing")
+  const typingInterval = setInterval(() => {
+    ctx.replyWithChatAction("typing").catch(() => {})
+  }, 4000)
+
+  try {
+    let title: string
+    let description: string
+
+    const separatorIdx = match.indexOf(" -- ")
+    if (separatorIdx !== -1) {
+      // Path A: enrich with Claude
+      title = match.slice(0, separatorIdx)
+      const rawDescription = match.slice(separatorIdx + 4)
+      let enriched = rawDescription
+      try {
+        const prompt = ENRICH_PROMPT_TEMPLATE.replace("{title}", title).replace(
+          "{description}",
+          rawDescription,
+        )
+        enriched = await clients.claude.ask(prompt)
+      } catch {
+        // silent fallback — use raw description
+      }
+      description = enriched
+    } else {
+      // Path B: expand with Claude
+      title = match
+      let expanded = ""
+      try {
+        const prompt = EXPAND_PROMPT_TEMPLATE.replace("{title}", title)
+        expanded = await clients.claude.ask(prompt)
+      } catch {
+        // silent fallback — use empty description
+      }
+      description = expanded
+    }
+
+    const issue = await clients.jira.createIssue(title, description)
+    await ctx.reply(`Created: ${issue.key}`)
+  } catch (err) {
+    if (err instanceof JiraAuthError) {
+      await ctx.reply("Authentication failed. Please check your Jira API token.")
+      return
+    }
+    if (err instanceof JiraNotFoundError) {
+      await ctx.reply("Jira resource not found.")
+      return
+    }
+    const message = err instanceof Error ? err.message : String(err)
+    console.log({ event: "error", command: "create", errorMessage: message })
+    await ctx.reply("Something went wrong. Please try again.")
+  } finally {
+    clearInterval(typingInterval)
+  }
+}
diff --git a/tests/bot/commands/create.test.ts b/tests/bot/commands/create.test.ts
new file mode 100644
index 0000000..6090140
--- /dev/null
+++ b/tests/bot/commands/create.test.ts
@@ -0,0 +1,138 @@
+import { describe, it, expect, mock, beforeEach } from "bun:test"
+import { handleCreate, ENRICH_PROMPT_TEMPLATE, EXPAND_PROMPT_TEMPLATE } from "../../../src/bot/commands/create"
+import { JiraAuthError } from "../../../src/shared/errors"
+
+function makeCtx(match: string) {
+  return {
+    match,
+    reply: mock().mockResolvedValue({}),
+    replyWithChatAction: mock().mockResolvedValue({}),
+  }
+}
+
+type MockClients = {
+  jira: { createIssue: ReturnType<typeof mock> }
+  claude: { ask: ReturnType<typeof mock> }
+}
+
+function makeClients(
+  createIssueImpl: unknown = { key: "ENG-1" },
+  askImpl: unknown = "Claude enriched description",
+): MockClients {
+  return {
+    jira: {
+      createIssue:
+        createIssueImpl instanceof Error
+          ? mock().mockRejectedValue(createIssueImpl)
+          : mock().mockResolvedValue(createIssueImpl),
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
+// ─── Template assertions ───────────────────────────────────────────────────
+
+describe("ENRICH_PROMPT_TEMPLATE", () => {
+  it("contains <title> and <description> XML delimiters", () => {
+    expect(ENRICH_PROMPT_TEMPLATE).toContain("<title>")
+    expect(ENRICH_PROMPT_TEMPLATE).toContain("<description>")
+  })
+})
+
+describe("EXPAND_PROMPT_TEMPLATE", () => {
+  it("contains <title> XML delimiter", () => {
+    expect(EXPAND_PROMPT_TEMPLATE).toContain("<title>")
+  })
+})
+
+// ─── Handler tests ──────────────────────────────────────────────────────────
+
+describe("handleCreate", () => {
+  it("enrich path: -- separator → Claude called with title+description, issue created", async () => {
+    const ctx = makeCtx("Fix login timeout -- auth expires too early")
+    const clients = makeClients({ key: "ENG-99" })
+
+    await handleCreate(ctx as never, clients as never)
+
+    expect(clients.claude.ask).toHaveBeenCalledTimes(1)
+    const prompt = clients.claude.ask.mock.calls[0][0] as string
+    expect(prompt).toContain("Fix login timeout")
+    expect(prompt).toContain("auth expires too early")
+
+    expect(clients.jira.createIssue).toHaveBeenCalledTimes(1)
+
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply).toContain("Created:")
+    expect(reply).toContain("ENG-99")
+  })
+
+  it("expand path: no -- separator → Claude called with title only, issue created", async () => {
+    const ctx = makeCtx("Fix login timeout")
+    const clients = makeClients({ key: "ENG-42" })
+
+    await handleCreate(ctx as never, clients as never)
+
+    expect(clients.claude.ask).toHaveBeenCalledTimes(1)
+    const prompt = clients.claude.ask.mock.calls[0][0] as string
+    expect(prompt).toContain("Fix login timeout")
+
+    expect(clients.jira.createIssue).toHaveBeenCalledTimes(1)
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply).toContain("Created:")
+  })
+
+  it("enrich path: Claude fails → raw description passed to Jira, issue still created", async () => {
+    const ctx = makeCtx("Fix login timeout -- auth expires too early")
+    const clients = makeClients({ key: "ENG-1" }, new Error("Claude down"))
+
+    await handleCreate(ctx as never, clients as never)
+
+    expect(clients.jira.createIssue).toHaveBeenCalledTimes(1)
+    const [, desc] = clients.jira.createIssue.mock.calls[0] as [string, string]
+    expect(desc).toBe("auth expires too early")
+
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply).toContain("Created:")
+  })
+
+  it("expand path: Claude fails → empty description passed to Jira, issue still created", async () => {
+    const ctx = makeCtx("Fix login timeout")
+    const clients = makeClients({ key: "ENG-1" }, new Error("Claude down"))
+
+    await handleCreate(ctx as never, clients as never)
+
+    expect(clients.jira.createIssue).toHaveBeenCalledTimes(1)
+    const [, desc] = clients.jira.createIssue.mock.calls[0] as [string, string]
+    expect(desc).toBe("")
+
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply).toContain("Created:")
+  })
+
+  it("no args → usage reply, no API calls", async () => {
+    const ctx = makeCtx("")
+    const clients = makeClients()
+
+    await handleCreate(ctx as never, clients as never)
+
+    expect(clients.jira.createIssue).not.toHaveBeenCalled()
+    expect(clients.claude.ask).not.toHaveBeenCalled()
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply.toLowerCase()).toMatch(/usage|\/create/)
+  })
+
+  it("JiraAuthError → reply contains auth/token message", async () => {
+    const ctx = makeCtx("Fix something")
+    const clients = makeClients(new JiraAuthError())
+
+    await handleCreate(ctx as never, clients as never)
+
+    const reply = (ctx.reply.mock.calls[0][0] as string).toLowerCase()
+    expect(reply).toMatch(/auth|token/)
+  })
+})
