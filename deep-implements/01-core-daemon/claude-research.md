# Research Findings: 01-core-daemon

## 1. Bun TypeScript CLI — `bun build --compile`

### Standalone Binary

```bash
bun build ./cli.ts --compile --outfile jira-assistant
# Production:
bun build ./cli.ts --compile --minify --sourcemap --bytecode --outfile jira-assistant
```

- `--bytecode`: pre-compiles; ~2x faster startup for large apps
- `--outfile` required (not `--outdir`) with `--compile`
- Cross-compile with `--target bun-darwin-arm64`, `bun-linux-x64`, etc.

### CLI Library: citty (Recommended)

Zero-dependency, TypeScript-native, built on `util.parseArgs`. Best for structured subcommands with lazy loading:

```typescript
import { defineCommand, runMain } from "citty";

const main = defineCommand({
  meta: { name: "jira-assistant", version: "1.0.0" },
  subCommands: {
    start: () => import("./commands/start.js").then(m => m.default),
    stop:  () => import("./commands/stop.js").then(m => m.default),
    status: () => import("./commands/status.js").then(m => m.default),
    config: () => import("./commands/config.js").then(m => m.default),
  },
});

runMain(main);
```

Alternatively: `util.parseArgs` (built-in, no deps) for simpler option parsing.

### Known Gotchas

- Native node-gyp modules fail in compiled binaries — prefer pure-JS alternatives
- No shell constructs in ProgramArguments; no `--daemonize` flags (launchd manages process)
- Check `process.stdout.isTTY` before ANSI color output

---

## 2. macOS launchd LaunchAgents

### User-Level Agent

Use `~/Library/LaunchAgents/` (no sudo needed). Label: reverse-domain notation.

**Key plist fields:**

| Field | Value for jira-assistant |
|---|---|
| `Label` | `net.jira-assistant` |
| `ProgramArguments` | `["/path/to/jira-assistant", "daemon"]` |
| `RunAtLoad` | `true` |
| `KeepAlive` | `true` |
| `StandardOutPath` | `~/.config/jira-assistant/logs/app.log` |
| `StandardErrorPath` | `~/.config/jira-assistant/logs/app.err` |
| `ThrottleInterval` | `10` (default; seconds between restarts) |

**launchctl commands:**

```bash
# Legacy (still works, simpler for CLI tools):
launchctl load -w ~/Library/LaunchAgents/net.jira-assistant.plist   # start
launchctl unload ~/Library/LaunchAgents/net.jira-assistant.plist     # stop
launchctl list | grep net.jira-assistant                             # status

# Modern (Ventura+):
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/net.jira-assistant.plist
launchctl bootout  gui/$(id -u) ~/Library/LaunchAgents/net.jira-assistant.plist
launchctl print    gui/$(id -u)/net.jira-assistant
```

### Critical Rules

1. **No daemonize flag** — launchd tracks the launched process directly; process must run in foreground
2. **No shell constructs** in ProgramArguments (no `|`, `>`, `$HOME`); use absolute paths
3. **No PID file needed** — launchd manages PIDs. If spec requires it, write from within the daemon process itself.
4. Validate plist: `plutil -lint ~/Library/LaunchAgents/net.jira-assistant.plist`
5. Plist permissions: `0644`, owned by user

---

## 3. TOML Parsing in Bun

### Bun Built-in vs smol-toml

- **Bun native TOML**: supports `import config from "./config.toml"` (static file import only). No `TOML.parse()` API yet (open issue #22219). No stringify, no runtime parsing.
- **smol-toml**: recommended for runtime parsing. TOML 1.1.0 compliant, TypeScript-native, ESM, fastest benchmark.

```bash
bun add smol-toml zod
```

```typescript
import { parse } from "smol-toml";
import { z } from "zod";

const ConfigSchema = z.object({
  telegram: z.object({ bot_token: z.string().min(1) }),
  jira: z.object({
    base_url: z.string().url(),
    api_token: z.string().min(1),
    email: z.string().email(),
    project_key: z.string().min(1),
  }),
  claude: z.object({ binary_path: z.string().min(1) }),
  app: z.object({ log_level: z.enum(["info", "debug", "error"]).default("info") }),
});

export type AppConfig = z.infer<typeof ConfigSchema>;

export async function loadConfig(path: string): Promise<AppConfig> {
  const raw = parse(await Bun.file(path).text());
  return ConfigSchema.parse(raw); // throws ZodError with clear message on failure
}
```

**Decision:** Use `smol-toml` + `zod` for runtime config loading. Skip zod-config (over-abstraction for this use case).

---

## 4. Interactive CLI Prompt Wizard

### Library: @clack/prompts (Recommended)

TypeScript-native, ESM, minimal (~2KB), Bun-compatible.

```bash
bun add @clack/prompts
```

**Multi-step wizard pattern (one field at a time with validation):**

```typescript
import { intro, outro, text, confirm, isCancel, cancel } from "@clack/prompts";

intro("jira-assistant setup");

async function prompt<T>(fn: () => Promise<T>): Promise<T> {
  const val = await fn();
  if (isCancel(val)) { cancel("Setup cancelled."); process.exit(0); }
  return val as T;
}

const botToken = await prompt(() => text({
  message: "Telegram bot token:",
  validate: v => v.trim().length < 10 ? "Invalid token" : undefined,
}));

// ... repeat for each field
outro("Config saved!");
```

### TTY Detection

```typescript
const isInteractive = process.stdout.isTTY && !process.env.CI;
if (!isInteractive) {
  console.error("jira-assistant config requires an interactive terminal");
  process.exit(1);
}
```

### Auto-detect Claude binary path

```typescript
import { which } from "bun"; // or: Bun.which("claude")

const detectedPath = Bun.which("claude") ?? "";
```

---

## Testing Preferences (New Project)

No existing test suite. Recommended setup:

- **Framework**: Bun's built-in test runner (`bun test`) — zero config, Jest-compatible API
- **Unit tests**: config loading/validation, wizard logic, plist generation
- **Integration tests**: spin up daemon, check PID file written, check launchd status (marked as manual/CI-skip on non-macOS)
- **Test file convention**: `src/**/*.test.ts` or `tests/`
- **Mocking**: Bun's built-in `mock()` for `launchctl` subprocess calls

Sources:
- https://bun.com/docs/bundler/executables
- https://www.launchd.info/
- https://github.com/squirrelchat/smol-toml
- https://www.npmjs.com/package/@clack/prompts
- https://bun.sh/guides/runtime/import-toml
