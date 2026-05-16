diff --git a/src/bot/commands/comment.ts b/src/bot/commands/comment.ts
index 336ce12..6eba9e2 100644
--- a/src/bot/commands/comment.ts
+++ b/src/bot/commands/comment.ts
@@ -1 +1,40 @@
-export {}
+import type { Context } from "grammy"
+import { JiraNotFoundError } from "../../shared/errors"
+import { parseFirstAndRest } from "../utils/parseArgs"
+
+interface Clients {
+  jira: {
+    addComment(key: string, text: string): Promise<void>
+  }
+}
+
+export async function handleComment(ctx: Context, clients: Clients): Promise<void> {
+  const match = ((ctx.match as string) ?? "").trim()
+
+  if (!match) {
+    await ctx.reply("Usage: /comment <issue-key> <text>")
+    return
+  }
+
+  const parsed = parseFirstAndRest(match)
+  if (!parsed) {
+    await ctx.reply("Usage: /comment <issue-key> <text>")
+    return
+  }
+
+  const { first: key, rest: text } = parsed
+
+  try {
+    await ctx.replyWithChatAction("typing")
+    await clients.jira.addComment(key, text)
+    await ctx.reply(`Comment added to ${key}`)
+  } catch (err) {
+    if (err instanceof JiraNotFoundError) {
+      await ctx.reply(`Issue ${key} not found.`)
+      return
+    }
+    const message = err instanceof Error ? err.message : String(err)
+    console.log({ event: "error", command: "comment", errorMessage: message })
+    await ctx.reply("Something went wrong. Please try again.")
+  }
+}
diff --git a/src/bot/commands/help.ts b/src/bot/commands/help.ts
index 336ce12..fe34ca0 100644
--- a/src/bot/commands/help.ts
+++ b/src/bot/commands/help.ts
@@ -1 +1,22 @@
-export {}
+import type { Context } from "grammy"
+
+export const HELP_TEXT = `Jira Assistant Commands:
+
+/create <title> [-- <description>]
+  Create a Jira issue. Claude enriches the description if provided.
+
+/move <issue-key> <status>
+  Transition an issue to a new status (e.g. "In Progress").
+
+/comment <issue-key> <text>
+  Add a comment to an existing issue.
+
+/solve <issue-key>
+  Analyze an issue with Claude and post a solution as a comment.
+
+/help
+  Show this reference.`
+
+export async function handleHelp(ctx: Context): Promise<void> {
+  await ctx.reply(HELP_TEXT)
+}
diff --git a/src/bot/commands/move.ts b/src/bot/commands/move.ts
index 336ce12..11697b1 100644
--- a/src/bot/commands/move.ts
+++ b/src/bot/commands/move.ts
@@ -1 +1,44 @@
-export {}
+import type { Context } from "grammy"
+import { InvalidTransitionError, JiraNotFoundError } from "../../shared/errors"
+import { parseFirstAndRest } from "../utils/parseArgs"
+
+interface Clients {
+  jira: {
+    transitionIssue(key: string, status: string): Promise<void>
+  }
+}
+
+export async function handleMove(ctx: Context, clients: Clients): Promise<void> {
+  const match = ((ctx.match as string) ?? "").trim()
+
+  if (!match) {
+    await ctx.reply("Usage: /move <issue-key> <status>")
+    return
+  }
+
+  const parsed = parseFirstAndRest(match)
+  if (!parsed) {
+    await ctx.reply("Usage: /move <issue-key> <status>")
+    return
+  }
+
+  const { first: key, rest: status } = parsed
+
+  try {
+    await ctx.replyWithChatAction("typing")
+    await clients.jira.transitionIssue(key, status)
+    await ctx.reply(`Moved ${key} → ${status}`)
+  } catch (err) {
+    if (err instanceof InvalidTransitionError) {
+      await ctx.reply(`Cannot move to "${status}". Available: ${err.available.join(", ")}`)
+      return
+    }
+    if (err instanceof JiraNotFoundError) {
+      await ctx.reply(`Issue ${key} not found.`)
+      return
+    }
+    const message = err instanceof Error ? err.message : String(err)
+    console.log({ event: "error", command: "move", errorMessage: message })
+    await ctx.reply("Something went wrong. Please try again.")
+  }
+}
diff --git a/tests/bot/commands/comment.test.ts b/tests/bot/commands/comment.test.ts
new file mode 100644
index 0000000..7c325ae
--- /dev/null
+++ b/tests/bot/commands/comment.test.ts
@@ -0,0 +1,90 @@
+import { describe, it, expect, mock } from "bun:test"
+import { handleComment } from "../../../src/bot/commands/comment"
+import { JiraNotFoundError } from "../../../src/shared/errors"
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
+  jira: { addComment: ReturnType<typeof mock> }
+}
+
+function makeClients(addCommentImpl: unknown = undefined): MockClients {
+  return {
+    jira: {
+      addComment:
+        addCommentImpl instanceof Error
+          ? mock().mockRejectedValue(addCommentImpl)
+          : mock().mockResolvedValue(addCommentImpl),
+    },
+  }
+}
+
+describe("handleComment", () => {
+  it("valid args → addComment called with key and text, success reply", async () => {
+    const ctx = makeCtx("ENG-1 Fixed the bug with   extra spaces")
+    const clients = makeClients()
+
+    await handleComment(ctx as never, clients as never)
+
+    expect(clients.jira.addComment).toHaveBeenCalledWith("ENG-1", "Fixed the bug with   extra spaces")
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply).toContain("ENG-1")
+  })
+
+  it("preserves internal spacing in comment text", async () => {
+    const ctx = makeCtx("ENG-1 Fixed the bug with   extra spaces")
+    const clients = makeClients()
+
+    await handleComment(ctx as never, clients as never)
+
+    const [, text] = clients.jira.addComment.mock.calls[0] as [string, string]
+    expect(text).toBe("Fixed the bug with   extra spaces")
+  })
+
+  it("sends typing action before API call", async () => {
+    const ctx = makeCtx("ENG-1 Some comment")
+    const clients = makeClients()
+
+    await handleComment(ctx as never, clients as never)
+
+    expect(ctx.replyWithChatAction).toHaveBeenCalledWith("typing")
+  })
+
+  it("no args → usage reply, no API calls", async () => {
+    const ctx = makeCtx("")
+    const clients = makeClients()
+
+    await handleComment(ctx as never, clients as never)
+
+    expect(clients.jira.addComment).not.toHaveBeenCalled()
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply.toLowerCase()).toMatch(/usage|\/comment/)
+  })
+
+  it("key only, no comment text → usage reply, no API calls", async () => {
+    const ctx = makeCtx("ENG-1")
+    const clients = makeClients()
+
+    await handleComment(ctx as never, clients as never)
+
+    expect(clients.jira.addComment).not.toHaveBeenCalled()
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply.toLowerCase()).toMatch(/usage|\/comment/)
+  })
+
+  it("JiraNotFoundError → reply contains issue key", async () => {
+    const ctx = makeCtx("ENG-999 Some comment")
+    const clients = makeClients(new JiraNotFoundError("ENG-999"))
+
+    await handleComment(ctx as never, clients as never)
+
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply).toContain("ENG-999")
+  })
+})
diff --git a/tests/bot/commands/help.test.ts b/tests/bot/commands/help.test.ts
new file mode 100644
index 0000000..f6bfb56
--- /dev/null
+++ b/tests/bot/commands/help.test.ts
@@ -0,0 +1,48 @@
+import { describe, it, expect, mock } from "bun:test"
+import { handleHelp, HELP_TEXT } from "../../../src/bot/commands/help"
+
+function makeCtx() {
+  return {
+    reply: mock().mockResolvedValue({}),
+  }
+}
+
+describe("HELP_TEXT", () => {
+  it("contains /create", () => {
+    expect(HELP_TEXT).toContain("/create")
+  })
+
+  it("contains /move", () => {
+    expect(HELP_TEXT).toContain("/move")
+  })
+
+  it("contains /comment", () => {
+    expect(HELP_TEXT).toContain("/comment")
+  })
+
+  it("contains /solve", () => {
+    expect(HELP_TEXT).toContain("/solve")
+  })
+
+  it("contains /help", () => {
+    expect(HELP_TEXT).toContain("/help")
+  })
+})
+
+describe("handleHelp", () => {
+  it("replies with HELP_TEXT", async () => {
+    const ctx = makeCtx()
+
+    await handleHelp(ctx as never)
+
+    expect(ctx.reply).toHaveBeenCalledWith(HELP_TEXT)
+  })
+
+  it("makes no API calls — pure reply", async () => {
+    const ctx = makeCtx()
+
+    await handleHelp(ctx as never)
+
+    expect(ctx.reply).toHaveBeenCalledTimes(1)
+  })
+})
diff --git a/tests/bot/commands/move.test.ts b/tests/bot/commands/move.test.ts
new file mode 100644
index 0000000..d613aa0
--- /dev/null
+++ b/tests/bot/commands/move.test.ts
@@ -0,0 +1,104 @@
+import { describe, it, expect, mock } from "bun:test"
+import { handleMove } from "../../../src/bot/commands/move"
+import { InvalidTransitionError, JiraNotFoundError } from "../../../src/shared/errors"
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
+  jira: { transitionIssue: ReturnType<typeof mock> }
+}
+
+function makeClients(transitionImpl: unknown = undefined): MockClients {
+  return {
+    jira: {
+      transitionIssue:
+        transitionImpl instanceof Error
+          ? mock().mockRejectedValue(transitionImpl)
+          : mock().mockResolvedValue(transitionImpl),
+    },
+  }
+}
+
+describe("handleMove", () => {
+  it("valid args → transitionIssue called, reply contains key and status", async () => {
+    const ctx = makeCtx("ENG-1 In Progress")
+    const clients = makeClients()
+
+    await handleMove(ctx as never, clients as never)
+
+    expect(clients.jira.transitionIssue).toHaveBeenCalledWith("ENG-1", "In Progress")
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply).toContain("ENG-1")
+    expect(reply).toContain("In Progress")
+  })
+
+  it("sends typing action before API call", async () => {
+    const ctx = makeCtx("ENG-1 In Progress")
+    const clients = makeClients()
+
+    await handleMove(ctx as never, clients as never)
+
+    expect(ctx.replyWithChatAction).toHaveBeenCalledWith("typing")
+  })
+
+  it("multi-word status passed as single string to transitionIssue", async () => {
+    const ctx = makeCtx("ENG-1 In Progress")
+    const clients = makeClients()
+
+    await handleMove(ctx as never, clients as never)
+
+    const [, status] = clients.jira.transitionIssue.mock.calls[0] as [string, string]
+    expect(status).toBe("In Progress")
+  })
+
+  it("no args → usage reply, no API calls", async () => {
+    const ctx = makeCtx("")
+    const clients = makeClients()
+
+    await handleMove(ctx as never, clients as never)
+
+    expect(clients.jira.transitionIssue).not.toHaveBeenCalled()
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply.toLowerCase()).toMatch(/usage|\/move/)
+  })
+
+  it("key only, no status → usage reply, no API calls", async () => {
+    const ctx = makeCtx("ENG-1")
+    const clients = makeClients()
+
+    await handleMove(ctx as never, clients as never)
+
+    expect(clients.jira.transitionIssue).not.toHaveBeenCalled()
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply.toLowerCase()).toMatch(/usage|\/move/)
+  })
+
+  it("InvalidTransitionError → reply contains Available: and transition names", async () => {
+    const ctx = makeCtx("ENG-1 Unknown Status")
+    const err = new InvalidTransitionError("Unknown Status", ["To Do", "Done"])
+    const clients = makeClients(err)
+
+    await handleMove(ctx as never, clients as never)
+
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply).toContain("Available:")
+    expect(reply).toContain("To Do")
+    expect(reply).toContain("Done")
+  })
+
+  it("JiraNotFoundError → reply contains issue key", async () => {
+    const ctx = makeCtx("ENG-999 Done")
+    const clients = makeClients(new JiraNotFoundError("ENG-999"))
+
+    await handleMove(ctx as never, clients as never)
+
+    const reply = ctx.reply.mock.calls[0][0] as string
+    expect(reply).toContain("ENG-999")
+  })
+})
