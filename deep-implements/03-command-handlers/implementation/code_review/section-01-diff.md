diff --git a/src/bot/bot.ts b/src/bot/bot.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/bot.ts
@@ -0,0 +1 @@
+export {}
diff --git a/src/bot/commands/comment.ts b/src/bot/commands/comment.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/commands/comment.ts
@@ -0,0 +1 @@
+export {}
diff --git a/src/bot/commands/create.ts b/src/bot/commands/create.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/commands/create.ts
@@ -0,0 +1 @@
+export {}
diff --git a/src/bot/commands/help.ts b/src/bot/commands/help.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/commands/help.ts
@@ -0,0 +1 @@
+export {}
diff --git a/src/bot/commands/index.ts b/src/bot/commands/index.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/commands/index.ts
@@ -0,0 +1 @@
+export {}
diff --git a/src/bot/commands/move.ts b/src/bot/commands/move.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/commands/move.ts
@@ -0,0 +1 @@
+export {}
diff --git a/src/bot/commands/solve.ts b/src/bot/commands/solve.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/commands/solve.ts
@@ -0,0 +1 @@
+export {}
diff --git a/src/bot/config.ts b/src/bot/config.ts
new file mode 100644
index 0000000..60434c6
--- /dev/null
+++ b/src/bot/config.ts
@@ -0,0 +1,43 @@
+export interface Config {
+  telegramBotToken: string
+  jiraBaseUrl: string
+  jiraProjectKey: string
+  jiraUserEmail: string
+  jiraApiToken: string
+  claudeApiKey: string
+  allowedUserIds: Set<number>
+}
+
+export function loadConfig(): Config {
+  function required(key: string): string {
+    const val = process.env[key]
+    if (val === undefined || val === "") {
+      throw new Error(`Missing required environment variable: ${key}`)
+    }
+    return val
+  }
+
+  const allowedUserIdsEnv = process.env["ALLOWED_USER_IDS"]
+  if (allowedUserIdsEnv === undefined) {
+    throw new Error("Missing required environment variable: ALLOWED_USER_IDS")
+  }
+
+  const allowedUserIds = new Set(
+    allowedUserIdsEnv
+      .split(",")
+      .map(s => s.trim())
+      .filter(s => s !== "")
+      .map(Number)
+      .filter(n => !isNaN(n)),
+  )
+
+  return {
+    telegramBotToken: required("TELEGRAM_BOT_TOKEN"),
+    jiraBaseUrl: required("JIRA_BASE_URL"),
+    jiraProjectKey: required("JIRA_PROJECT_KEY"),
+    jiraUserEmail: required("JIRA_USER_EMAIL"),
+    jiraApiToken: required("JIRA_API_TOKEN"),
+    claudeApiKey: required("CLAUDE_API_KEY"),
+    allowedUserIds,
+  }
+}
diff --git a/src/bot/middleware/auth.ts b/src/bot/middleware/auth.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/middleware/auth.ts
@@ -0,0 +1 @@
+export {}
diff --git a/src/bot/utils/parseArgs.ts b/src/bot/utils/parseArgs.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/utils/parseArgs.ts
@@ -0,0 +1 @@
+export {}
diff --git a/src/bot/utils/splitMessage.ts b/src/bot/utils/splitMessage.ts
new file mode 100644
index 0000000..336ce12
--- /dev/null
+++ b/src/bot/utils/splitMessage.ts
@@ -0,0 +1 @@
+export {}
diff --git a/tests/bot/config.test.ts b/tests/bot/config.test.ts
new file mode 100644
index 0000000..a0e3389
--- /dev/null
+++ b/tests/bot/config.test.ts
@@ -0,0 +1,91 @@
+import { describe, it, expect, beforeEach, afterEach } from "bun:test"
+import { loadConfig } from "../../src/bot/config"
+
+const ENV_KEYS = [
+  "TELEGRAM_BOT_TOKEN",
+  "JIRA_BASE_URL",
+  "JIRA_PROJECT_KEY",
+  "JIRA_USER_EMAIL",
+  "JIRA_API_TOKEN",
+  "CLAUDE_API_KEY",
+  "ALLOWED_USER_IDS",
+] as const
+
+const VALID_ENV: Record<string, string> = {
+  TELEGRAM_BOT_TOKEN: "bot123:token",
+  JIRA_BASE_URL: "https://test.atlassian.net",
+  JIRA_PROJECT_KEY: "PROJ",
+  JIRA_USER_EMAIL: "user@example.com",
+  JIRA_API_TOKEN: "jira-secret",
+  CLAUDE_API_KEY: "claude-secret",
+  ALLOWED_USER_IDS: "123,456",
+}
+
+let savedEnv: Record<string, string | undefined> = {}
+
+beforeEach(() => {
+  savedEnv = {}
+  for (const key of ENV_KEYS) {
+    savedEnv[key] = process.env[key]
+  }
+  Object.assign(process.env, VALID_ENV)
+})
+
+afterEach(() => {
+  for (const [key, val] of Object.entries(savedEnv)) {
+    if (val === undefined) delete process.env[key]
+    else process.env[key] = val
+  }
+})
+
+describe("loadConfig()", () => {
+  it("returns valid Config with all required fields present", () => {
+    const config = loadConfig()
+    expect(config.telegramBotToken).toBe("bot123:token")
+    expect(config.jiraBaseUrl).toBe("https://test.atlassian.net")
+    expect(config.jiraProjectKey).toBe("PROJ")
+    expect(config.jiraUserEmail).toBe("user@example.com")
+    expect(config.jiraApiToken).toBe("jira-secret")
+    expect(config.claudeApiKey).toBe("claude-secret")
+    expect(config.allowedUserIds).toEqual(new Set([123, 456]))
+  })
+
+  it("throws with message mentioning TELEGRAM_BOT_TOKEN when it is missing", () => {
+    delete process.env.TELEGRAM_BOT_TOKEN
+    expect(() => loadConfig()).toThrow("TELEGRAM_BOT_TOKEN")
+  })
+
+  it("throws when ALLOWED_USER_IDS is missing", () => {
+    delete process.env.ALLOWED_USER_IDS
+    expect(() => loadConfig()).toThrow("ALLOWED_USER_IDS")
+  })
+
+  it("parses ALLOWED_USER_IDS='123,456' into Set containing 123 and 456", () => {
+    process.env.ALLOWED_USER_IDS = "123,456"
+    const config = loadConfig()
+    expect(config.allowedUserIds.has(123)).toBe(true)
+    expect(config.allowedUserIds.has(456)).toBe(true)
+    expect(config.allowedUserIds.size).toBe(2)
+  })
+
+  it("trims whitespace from ALLOWED_USER_IDS entries ('123, 456')", () => {
+    process.env.ALLOWED_USER_IDS = "123, 456"
+    const config = loadConfig()
+    expect(config.allowedUserIds.has(123)).toBe(true)
+    expect(config.allowedUserIds.has(456)).toBe(true)
+    expect(config.allowedUserIds.size).toBe(2)
+  })
+
+  it("filters NaN entries from ALLOWED_USER_IDS without throwing ('abc,456')", () => {
+    process.env.ALLOWED_USER_IDS = "abc,456"
+    const config = loadConfig()
+    expect(config.allowedUserIds.has(456)).toBe(true)
+    expect(config.allowedUserIds.size).toBe(1)
+  })
+
+  it("returns empty Set when ALLOWED_USER_IDS is empty string", () => {
+    process.env.ALLOWED_USER_IDS = ""
+    const config = loadConfig()
+    expect(config.allowedUserIds.size).toBe(0)
+  })
+})
