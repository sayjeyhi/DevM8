diff --git a/01-core-daemon/bun.lock b/01-core-daemon/bun.lock
new file mode 100644
index 0000000..2fffa9a
--- /dev/null
+++ b/01-core-daemon/bun.lock
@@ -0,0 +1,34 @@
+{
+  "lockfileVersion": 1,
+  "configVersion": 1,
+  "workspaces": {
+    "": {
+      "name": "jira-assistant",
+      "dependencies": {
+        "@clack/prompts": "latest",
+        "citty": "latest",
+        "smol-toml": "latest",
+        "zod": "latest",
+      },
+    },
+  },
+  "packages": {
+    "@clack/core": ["@clack/core@1.3.1", "", { "dependencies": { "fast-wrap-ansi": "^0.2.0", "sisteransi": "^1.0.5" } }, "sha512-fT1qHVGAag4IEkrupZ6lRRbNCs1vS9P01KB/sG8zKgvUztbYtFBtQpjSITNwooDZ83tpsPzP0mRNs1/KVszCRA=="],
+
+    "@clack/prompts": ["@clack/prompts@1.4.0", "", { "dependencies": { "@clack/core": "1.3.1", "fast-string-width": "^3.0.2", "fast-wrap-ansi": "^0.2.0", "sisteransi": "^1.0.5" } }, "sha512-S0My7XPGIgpRWMDG8uRqalbgT+a6FmCUdOW+HaIOVVpUPHOb7RrpvjTjiODadKp06fsrVDJZlIzc6yCTp4AnxA=="],
+
+    "citty": ["citty@0.2.2", "", {}, "sha512-+6vJA3L98yv+IdfKGZHBNiGW5KHn22e/JwID0Strsz8h4S/csAu/OuICwxrg44k5MRiZHWIo8XXuJgQTriRP4w=="],
+
+    "fast-string-truncated-width": ["fast-string-truncated-width@3.0.3", "", {}, "sha512-0jjjIEL6+0jag3l2XWWizO64/aZVtpiGE3t0Zgqxv0DPuxiMjvB3M24fCyhZUO4KomJQPj3LTSUnDP3GpdwC0g=="],
+
+    "fast-string-width": ["fast-string-width@3.0.2", "", { "dependencies": { "fast-string-truncated-width": "^3.0.2" } }, "sha512-gX8LrtNEI5hq8DVUfRQMbr5lpaS4nMIWV+7XEbXk2b8kiQIizgnlr12B4dA3ZEx3308ze0O4Q1R+cHts8kyUJg=="],
+
+    "fast-wrap-ansi": ["fast-wrap-ansi@0.2.0", "", { "dependencies": { "fast-string-width": "^3.0.2" } }, "sha512-rLV8JHxTyhVmFYhBJuMujcrHqOT2cnO5Zxj37qROj23CP39GXubJRBUFF0z8KFK77Uc0SukZUf7JZhsVEQ6n8w=="],
+
+    "sisteransi": ["sisteransi@1.0.5", "", {}, "sha512-bLGGlR1QxBcynn2d5YmDX4MGjlZvy2MRBDRNHLJ8VI6l6+9FUiyTFNJ0IveOSP0bcXgVDPRcfGqA0pjaqUpfVg=="],
+
+    "smol-toml": ["smol-toml@1.6.1", "", {}, "sha512-dWUG8F5sIIARXih1DTaQAX4SsiTXhInKf1buxdY9DIg4ZYPZK5nGM1VRIYmEbDbsHt7USo99xSLFu5Q1IqTmsg=="],
+
+    "zod": ["zod@4.4.3", "", {}, "sha512-ytENFjIJFl2UwYglde2jchW2Hwm4GJFLDiSXWdTrJQBIN9Fcyp7n4DhxJEiWNAJMV1/BqWfW/kkg71UDcHJyTQ=="],
+  }
+}
diff --git a/01-core-daemon/package.json b/01-core-daemon/package.json
new file mode 100644
index 0000000..1955a8d
--- /dev/null
+++ b/01-core-daemon/package.json
@@ -0,0 +1,15 @@
+{
+  "name": "jira-assistant",
+  "version": "0.1.0",
+  "scripts": {
+    "build": "bun run build.ts",
+    "test": "bun test",
+    "start": "bun run src/index.ts"
+  },
+  "dependencies": {
+    "smol-toml": "latest",
+    "zod": "latest",
+    "@clack/prompts": "latest",
+    "citty": "latest"
+  }
+}
diff --git a/01-core-daemon/src/shared/errors.ts b/01-core-daemon/src/shared/errors.ts
new file mode 100644
index 0000000..bd4cf14
--- /dev/null
+++ b/01-core-daemon/src/shared/errors.ts
@@ -0,0 +1,31 @@
+export class FriendlyError extends Error {
+  readonly hint?: string
+  constructor(message: string, hint?: string) {
+    super(message)
+    this.name = "FriendlyError"
+    this.hint = hint
+  }
+}
+
+export class LaunchctlError extends FriendlyError {
+  readonly rawOutput: string
+  constructor(stderr: string, hint: string) {
+    super(`launchctl failed: ${stderr}`, hint)
+    this.name = "LaunchctlError"
+    this.rawOutput = stderr
+  }
+}
+
+export function launchctlHint(stderr: string): string {
+  const s = stderr.toLowerCase()
+  if (s.includes("no such file or directory")) {
+    return "Make sure you ran `jira-assistant start` first"
+  }
+  if (s.includes("operation already in progress")) {
+    return "Daemon may already be running; check `jira-assistant status`"
+  }
+  if (s.includes("permission denied")) {
+    return "Check file permissions on the plist"
+  }
+  return "Run `jira-assistant status` for more info"
+}
diff --git a/01-core-daemon/src/shared/paths.ts b/01-core-daemon/src/shared/paths.ts
new file mode 100644
index 0000000..2140d35
--- /dev/null
+++ b/01-core-daemon/src/shared/paths.ts
@@ -0,0 +1,15 @@
+import { homedir } from "os"
+import { join } from "path"
+
+const home = homedir()
+
+export const PATHS = {
+  configDir:       join(home, ".config/jira-assistant"),
+  configFile:      join(home, ".config/jira-assistant/config.toml"),
+  restartsFile:    join(home, ".config/jira-assistant/restarts.json"),
+  logsDir:         join(home, ".config/jira-assistant/logs"),
+  logFile:         join(home, ".config/jira-assistant/logs/app.log"),
+  pidFile:         join(home, ".config/jira-assistant/daemon.pid"),
+  plistFile:       join(home, "Library/LaunchAgents/net.jira-assistant.plist"),
+  launchAgentsDir: join(home, "Library/LaunchAgents"),
+}
diff --git a/01-core-daemon/tests/shared/errors.test.ts b/01-core-daemon/tests/shared/errors.test.ts
new file mode 100644
index 0000000..cb48c6b
--- /dev/null
+++ b/01-core-daemon/tests/shared/errors.test.ts
@@ -0,0 +1,63 @@
+import { describe, it, expect } from "bun:test"
+import { FriendlyError, LaunchctlError, launchctlHint } from "../../src/shared/errors"
+
+describe("FriendlyError", () => {
+  it("is an instance of Error", () => {
+    const err = new FriendlyError("test message")
+    expect(err).toBeInstanceOf(Error)
+  })
+
+  it("exposes hint property", () => {
+    const err = new FriendlyError("test message", "try this")
+    expect(err.hint).toBe("try this")
+  })
+
+  it("hint is undefined when not provided", () => {
+    const err = new FriendlyError("test message")
+    expect(err.hint).toBeUndefined()
+  })
+
+  it("message is accessible via .message", () => {
+    const err = new FriendlyError("hello world")
+    expect(err.message).toBe("hello world")
+  })
+})
+
+describe("LaunchctlError", () => {
+  it("is an instance of FriendlyError", () => {
+    const err = new LaunchctlError("some stderr", "some hint")
+    expect(err).toBeInstanceOf(FriendlyError)
+  })
+
+  it("exposes rawOutput property", () => {
+    const err = new LaunchctlError("stderr output here", "hint text")
+    expect(err.rawOutput).toBe("stderr output here")
+  })
+
+  it("hint is accessible", () => {
+    const err = new LaunchctlError("stderr", "my hint")
+    expect(err.hint).toBe("my hint")
+  })
+})
+
+describe("launchctlHint", () => {
+  it("maps 'No such file or directory' to jira-assistant start hint", () => {
+    const hint = launchctlHint("error: No such file or directory")
+    expect(hint).toContain("jira-assistant start")
+  })
+
+  it("maps 'Operation already in progress' to jira-assistant status hint", () => {
+    const hint = launchctlHint("error: Operation already in progress")
+    expect(hint).toContain("jira-assistant status")
+  })
+
+  it("maps 'Permission denied' to file permissions hint", () => {
+    const hint = launchctlHint("error: Permission denied")
+    expect(hint).toContain("plist")
+  })
+
+  it("returns fallback hint for unknown errors", () => {
+    const hint = launchctlHint("some unknown error")
+    expect(hint).toContain("jira-assistant status")
+  })
+})
diff --git a/01-core-daemon/tests/shared/paths.test.ts b/01-core-daemon/tests/shared/paths.test.ts
new file mode 100644
index 0000000..894d49d
--- /dev/null
+++ b/01-core-daemon/tests/shared/paths.test.ts
@@ -0,0 +1,41 @@
+import { describe, it, expect } from "bun:test"
+import { homedir } from "os"
+import { PATHS } from "../../src/shared/paths"
+
+describe("PATHS", () => {
+  it("all values are absolute paths (no ~ literals)", () => {
+    for (const value of Object.values(PATHS)) {
+      expect(value).not.toContain("~")
+      expect(value.startsWith("/")).toBe(true)
+    }
+  })
+
+  it("configDir starts with homedir()", () => {
+    expect(PATHS.configDir.startsWith(homedir())).toBe(true)
+  })
+
+  it("plistFile contains Library/LaunchAgents and ends with .plist", () => {
+    expect(PATHS.plistFile).toContain("Library/LaunchAgents")
+    expect(PATHS.plistFile.endsWith(".plist")).toBe(true)
+  })
+
+  it("restartsFile ends with restarts.json", () => {
+    expect(PATHS.restartsFile.endsWith("restarts.json")).toBe(true)
+  })
+
+  it("logFile ends with app.log", () => {
+    expect(PATHS.logFile.endsWith("app.log")).toBe(true)
+  })
+
+  it("pidFile ends with daemon.pid", () => {
+    expect(PATHS.pidFile.endsWith("daemon.pid")).toBe(true)
+  })
+
+  it("configFile ends with config.toml", () => {
+    expect(PATHS.configFile.endsWith("config.toml")).toBe(true)
+  })
+
+  it("logsDir is a prefix of logFile", () => {
+    expect(PATHS.logFile.startsWith(PATHS.logsDir)).toBe(true)
+  })
+})
diff --git a/01-core-daemon/tsconfig.json b/01-core-daemon/tsconfig.json
new file mode 100644
index 0000000..78864af
--- /dev/null
+++ b/01-core-daemon/tsconfig.json
@@ -0,0 +1,10 @@
+{
+  "compilerOptions": {
+    "target": "ESNext",
+    "module": "ESNext",
+    "moduleResolution": "bundler",
+    "strict": true,
+    "skipLibCheck": true
+  },
+  "include": ["src/**/*", "tests/**/*"]
+}
