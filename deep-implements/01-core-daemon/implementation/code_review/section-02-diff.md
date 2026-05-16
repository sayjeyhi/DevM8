diff --git a/01-core-daemon/src/config/loader.ts b/01-core-daemon/src/config/loader.ts
new file mode 100644
index 0000000..5535904
--- /dev/null
+++ b/01-core-daemon/src/config/loader.ts
@@ -0,0 +1,56 @@
+import { parse, stringify } from "smol-toml"
+import { dirname } from "path"
+import { writeFile, rename, mkdir, chmod, stat } from "node:fs/promises"
+import { AppConfigSchema, type AppConfig } from "./schema"
+import { FriendlyError } from "../shared/errors"
+import { PATHS } from "../shared/paths"
+
+export async function loadConfig(configPath?: string): Promise<AppConfig> {
+  const resolvedPath = configPath ?? PATHS.configFile
+
+  let rawText: string
+  try {
+    rawText = await Bun.file(resolvedPath).text()
+  } catch (e: unknown) {
+    const code = (e as NodeJS.ErrnoException).code
+    if (code === "ENOENT" || !await Bun.file(resolvedPath).exists()) {
+      throw new FriendlyError(
+        `Config file not found at ${resolvedPath}. Run \`jira-assistant config\` to create it.`,
+        "Run `jira-assistant config` to set up your configuration."
+      )
+    }
+    throw e
+  }
+
+  let parsed: unknown
+  try {
+    parsed = parse(rawText)
+  } catch (e: unknown) {
+    throw new FriendlyError(`Failed to parse config file: ${(e as Error).message}`)
+  }
+
+  const result = AppConfigSchema.safeParse(parsed)
+  if (!result.success) {
+    const lines = result.error.issues.map((issue) => {
+      const field = issue.path.join(".") || "unknown"
+      return `${field}: ${issue.message}`
+    })
+    throw new FriendlyError(`Invalid config:\n${lines.join("\n")}`)
+  }
+
+  return result.data
+}
+
+export async function configExists(configPath?: string): Promise<boolean> {
+  return Bun.file(configPath ?? PATHS.configFile).exists()
+}
+
+export async function writeConfig(config: AppConfig, configPath?: string): Promise<void> {
+  const resolvedPath = configPath ?? PATHS.configFile
+  await mkdir(dirname(resolvedPath), { recursive: true })
+  const toml = stringify(config as Record<string, unknown>)
+  const tmpPath = resolvedPath + ".tmp"
+  await writeFile(tmpPath, toml, "utf8")
+  await rename(tmpPath, resolvedPath)
+  await chmod(resolvedPath, 0o600)
+}
diff --git a/01-core-daemon/src/config/schema.ts b/01-core-daemon/src/config/schema.ts
new file mode 100644
index 0000000..2eb8279
--- /dev/null
+++ b/01-core-daemon/src/config/schema.ts
@@ -0,0 +1,28 @@
+import { z } from "zod"
+
+export const BOT_TOKEN_REGEX = /^\d+:[A-Za-z0-9_-]{20,}$/
+export const PROJECT_KEY_REGEX = /^[A-Z][A-Z0-9_]+$/
+export const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/
+
+export const AppConfigSchema = z.object({
+  telegram: z.object({
+    bot_token: z.string().regex(BOT_TOKEN_REGEX),
+  }),
+  jira: z.object({
+    base_url: z.string().refine((v) => {
+      if (!v.startsWith("https://")) return false
+      try { new URL(v); return true } catch { return false }
+    }, { message: "Must be a valid HTTPS URL" }),
+    api_token: z.string().min(1),
+    email: z.string().refine((v) => EMAIL_REGEX.test(v), { message: "Must be a valid email address" }),
+    project_key: z.string().regex(PROJECT_KEY_REGEX),
+  }),
+  claude: z.object({
+    binary_path: z.string().min(1),
+  }),
+  app: z.object({
+    log_level: z.enum(["info", "debug", "error"]).default("info"),
+  }).default({ log_level: "info" }),
+})
+
+export type AppConfig = z.infer<typeof AppConfigSchema>
diff --git a/01-core-daemon/src/config/wizard.ts b/01-core-daemon/src/config/wizard.ts
new file mode 100644
index 0000000..71e98d7
--- /dev/null
+++ b/01-core-daemon/src/config/wizard.ts
@@ -0,0 +1,68 @@
+import { intro, outro, group, text, isCancel } from "@clack/prompts"
+import { BOT_TOKEN_REGEX, PROJECT_KEY_REGEX, EMAIL_REGEX, type AppConfig } from "./schema"
+import { FriendlyError } from "../shared/errors"
+
+export async function runWizard(existing?: AppConfig): Promise<AppConfig> {
+  if (!process.stdin.isTTY) {
+    throw new FriendlyError(
+      "Cannot run wizard in non-interactive mode.",
+      "Attach a TTY or provide config manually."
+    )
+  }
+
+  intro("jira-assistant setup")
+
+  const result = await group({
+    bot_token: () => text({
+      message: "Telegram bot token",
+      initialValue: existing?.telegram.bot_token,
+      validate: (v) => BOT_TOKEN_REGEX.test(v) ? undefined : "Invalid Telegram bot token format. Expected format: 123456:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef",
+    }),
+    base_url: () => text({
+      message: "Jira base URL (e.g. https://mycompany.atlassian.net)",
+      initialValue: existing?.jira.base_url,
+      validate: (v) => {
+        if (!v.startsWith("https://")) return "Must be a valid HTTPS URL"
+        try { new URL(v) } catch { return "Must be a valid HTTPS URL" }
+      },
+    }),
+    api_token: () => text({
+      message: "Jira API token",
+      initialValue: existing?.jira.api_token,
+      validate: (v) => v.length > 0 ? undefined : "API token is required",
+    }),
+    email: () => text({
+      message: "Jira account email",
+      initialValue: existing?.jira.email,
+      validate: (v) => EMAIL_REGEX.test(v) ? undefined : "Must be a valid email address",
+    }),
+    project_key: () => text({
+      message: "Jira project key (e.g. MYPROJECT)",
+      initialValue: existing?.jira.project_key,
+      validate: (v) => PROJECT_KEY_REGEX.test(v) ? undefined : "Must be uppercase letters only, e.g. MYPROJECT",
+    }),
+    binary_path: () => text({
+      message: "Path to claude binary",
+      initialValue: existing?.claude.binary_path ?? (Bun.which("claude") ?? ""),
+      validate: async (v) => (await Bun.file(v).exists()) ? undefined : "File not found at this path",
+    }),
+  })
+
+  if (isCancel(result)) {
+    throw new FriendlyError("Setup cancelled.")
+  }
+
+  const r = result as {
+    bot_token: string; base_url: string; api_token: string
+    email: string; project_key: string; binary_path: string
+  }
+
+  outro("Config saved!")
+
+  return {
+    telegram: { bot_token: r.bot_token },
+    jira: { base_url: r.base_url, api_token: r.api_token, email: r.email, project_key: r.project_key },
+    claude: { binary_path: r.binary_path },
+    app: { log_level: existing?.app.log_level ?? "info" },
+  }
+}
diff --git a/01-core-daemon/tests/config/loader.test.ts b/01-core-daemon/tests/config/loader.test.ts
new file mode 100644
index 0000000..ed90442
--- /dev/null
+++ b/01-core-daemon/tests/config/loader.test.ts
@@ -0,0 +1,145 @@
+import { describe, it, expect, beforeEach, afterEach } from "bun:test"
+import { join } from "path"
+import { mkdtemp, rm, stat } from "node:fs/promises"
+import { tmpdir } from "os"
+import { loadConfig, configExists, writeConfig } from "../../src/config/loader"
+import { FriendlyError } from "../../src/shared/errors"
+import type { AppConfig } from "../../src/config/schema"
+
+const VALID_TOML = `
+[telegram]
+bot_token = "123456789:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh"
+
+[jira]
+base_url = "https://mycompany.atlassian.net"
+api_token = "my-api-token"
+email = "user@example.com"
+project_key = "MYPROJECT"
+
+[claude]
+binary_path = "/usr/local/bin/claude"
+
+[app]
+log_level = "info"
+`
+
+const VALID_CONFIG: AppConfig = {
+  telegram: { bot_token: "123456789:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh" },
+  jira: {
+    base_url: "https://mycompany.atlassian.net",
+    api_token: "my-api-token",
+    email: "user@example.com",
+    project_key: "MYPROJECT",
+  },
+  claude: { binary_path: "/usr/local/bin/claude" },
+  app: { log_level: "info" },
+}
+
+let tmpDir: string
+
+beforeEach(async () => {
+  tmpDir = await mkdtemp(join(tmpdir(), "jira-assistant-test-"))
+})
+
+afterEach(async () => {
+  await rm(tmpDir, { recursive: true, force: true })
+})
+
+describe("loadConfig", () => {
+  it("parses valid TOML and returns AppConfig shape", async () => {
+    const configPath = join(tmpDir, "config.toml")
+    await Bun.write(configPath, VALID_TOML)
+    const config = await loadConfig(configPath)
+    expect(config.telegram.bot_token).toBe("123456789:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh")
+    expect(config.jira.base_url).toBe("https://mycompany.atlassian.net")
+    expect(config.app.log_level).toBe("info")
+  })
+
+  it("defaults app.log_level to 'info' when omitted", async () => {
+    const configPath = join(tmpDir, "config.toml")
+    const toml = VALID_TOML.replace(/log_level = "info"\n/, "")
+    await Bun.write(configPath, toml)
+    const config = await loadConfig(configPath)
+    expect(config.app.log_level).toBe("info")
+  })
+
+  it("throws FriendlyError listing all invalid fields when required field missing", async () => {
+    const configPath = join(tmpDir, "config.toml")
+    const toml = `
+[jira]
+base_url = "https://mycompany.atlassian.net"
+api_token = "token"
+email = "user@example.com"
+project_key = "PROJ"
+
+[claude]
+binary_path = "/usr/bin/claude"
+`
+    await Bun.write(configPath, toml)
+    let err: unknown
+    try { await loadConfig(configPath) } catch (e) { err = e }
+    expect(err).toBeInstanceOf(FriendlyError)
+    const msg = (err as FriendlyError).message
+    expect(msg).toContain("telegram")
+  })
+
+  it("throws FriendlyError on malformed TOML", async () => {
+    const configPath = join(tmpDir, "config.toml")
+    await Bun.write(configPath, "key = ")
+    let err: unknown
+    try { await loadConfig(configPath) } catch (e) { err = e }
+    expect(err).toBeInstanceOf(FriendlyError)
+  })
+
+  it("throws FriendlyError with jira-assistant config hint when file not found", async () => {
+    let err: unknown
+    try { await loadConfig("/nonexistent/path/config.toml") } catch (e) { err = e }
+    expect(err).toBeInstanceOf(FriendlyError)
+    const friendly = err as FriendlyError
+    expect(friendly.message).toContain("jira-assistant config")
+  })
+})
+
+describe("configExists", () => {
+  it("returns false when file does not exist", async () => {
+    const result = await configExists(join(tmpDir, "nonexistent.toml"))
+    expect(result).toBe(false)
+  })
+
+  it("returns true when file exists", async () => {
+    const configPath = join(tmpDir, "config.toml")
+    await Bun.write(configPath, VALID_TOML)
+    const result = await configExists(configPath)
+    expect(result).toBe(true)
+  })
+})
+
+describe("writeConfig", () => {
+  it("creates missing directory and writes file", async () => {
+    const configPath = join(tmpDir, "nested", "dir", "config.toml")
+    await writeConfig(VALID_CONFIG, configPath)
+    const exists = await Bun.file(configPath).exists()
+    expect(exists).toBe(true)
+  })
+
+  it("round-trips config (writeConfig then loadConfig returns equal object)", async () => {
+    const configPath = join(tmpDir, "config.toml")
+    await writeConfig(VALID_CONFIG, configPath)
+    const loaded = await loadConfig(configPath)
+    expect(loaded).toEqual(VALID_CONFIG)
+  })
+
+  it("sets file permissions to 0o600", async () => {
+    const configPath = join(tmpDir, "config.toml")
+    await writeConfig(VALID_CONFIG, configPath)
+    const s = await stat(configPath)
+    expect(s.mode & 0o777).toBe(0o600)
+  })
+
+  it("uses atomic write (no leftover .tmp files after success)", async () => {
+    const configPath = join(tmpDir, "config.toml")
+    await writeConfig(VALID_CONFIG, configPath)
+    const tmpFile = await Bun.file(configPath + ".tmp").exists()
+    expect(tmpFile).toBe(false)
+  })
+})
diff --git a/01-core-daemon/tests/config/wizard.test.ts b/01-core-daemon/tests/config/wizard.test.ts
new file mode 100644
index 0000000..cb6e7bc
--- /dev/null
+++ b/01-core-daemon/tests/config/wizard.test.ts
@@ -0,0 +1,22 @@
+import { describe, it, expect } from "bun:test"
+import { runWizard } from "../../src/config/wizard"
+import { FriendlyError } from "../../src/shared/errors"
+
+describe("runWizard", () => {
+  it("throws FriendlyError in non-interactive (non-TTY) environment", async () => {
+    // In test runner, process.stdin.isTTY is falsy
+    let err: unknown
+    try { await runWizard() } catch (e) { err = e }
+    expect(err).toBeInstanceOf(FriendlyError)
+  })
+
+  it.todo("prompts for telegram.bot_token and validates format")
+  it.todo("prompts for jira.base_url and validates HTTPS URL")
+  it.todo("prompts for jira.api_token (non-empty)")
+  it.todo("prompts for jira.email and validates format")
+  it.todo("prompts for jira.project_key and validates uppercase")
+  it.todo("auto-fills claude.binary_path from PATH when not in existing config")
+  it.todo("preserves existing config values as initial values")
+  it.todo("throws FriendlyError on Ctrl+C cancel")
+  it.todo("returns AppConfig without writing to disk")
+})
