# Research: Telegram Bot Patterns & Message Splitting (2025/2026)

## Topic 1: Telegram Bot Slash Command Handler Patterns

### Library Landscape

| Library | Status | Verdict |
|---|---|---|
| **grammY** | Actively maintained | Recommended for new TypeScript projects |
| **Telegraf** | ~2 years stale | OK for existing projects; avoid for new |
| **node-telegram-bot-api** | Minimally maintained | Avoid — no TypeScript, deprecated deps |

grammY key advantages: first-class TypeScript, middleware architecture (Koa-style), rich official plugin ecosystem, warns about anti-patterns (nested listener memory leaks), built-in "did you mean?" for unknown commands.

### Command Registration (grammY)

```typescript
import { Bot, Composer } from "grammy";
import { CommandGroup } from "@grammyjs/commands";

const bot = new Bot(process.env.BOT_TOKEN!);
const myCommands = new CommandGroup();

myCommands
  .command("create", "Create a Jira ticket",   handleCreate)
  .command("move",   "Move ticket to status",   handleMove)
  .command("comment","Add comment to ticket",   handleComment)
  .command("solve",  "Get AI solution",         handleSolve)
  .command("help",   "Show this help",          handleHelp);

await myCommands.setCommands(bot); // registers menu in Telegram UI
bot.use(myCommands);
```

Recommended file structure:
```
src/
  commands/
    index.ts      ← merges all composers
    create.ts
    move.ts
    comment.ts
    solve.ts
    help.ts
  bot.ts          ← Bot init + global middleware
```

### Argument Parsing

grammY strips the command prefix automatically; remainder is in `ctx.match`:

```typescript
bot.command("move", (ctx) => {
  const args = ctx.match.trim().split(/\s+/);
  // /move ENG-123 "In Progress" → args = ["ENG-123", "In", "Progress"]
  const [ticketKey, ...statusParts] = args;
  const status = statusParts.join(" ");
});
```

Production pattern — parse before routing:

```typescript
function parseArgs(ctx: Context): string[] {
  return ctx.match?.trim().split(/\s+/).filter(Boolean) ?? [];
}
```

### Error Handling

```typescript
// Global catch-all
bot.catch((err) => {
  const ctx = err.ctx;
  const e   = err.error;
  if (e instanceof GrammyError) ctx.reply("Telegram API error. Try again.");
  else ctx.reply("Unexpected error — check logs.");
});

// Per-command validation
bot.command("move", async (ctx) => {
  const args = parseArgs(ctx);
  if (args.length < 2) {
    return ctx.reply("Usage: /move <ticket> <status>\nExample: /move ENG-123 \"In Progress\"");
  }
  // ...
});
```

Unknown command with suggestion:
```typescript
import { commandNotFound } from "@grammyjs/commands";
bot.filter(commandNotFound(myCommands, { ignoreCase: true, similarityThreshold: 0.5 }))
   .use(async (ctx) => {
     if (ctx.commandSuggestion) {
       await ctx.reply(`Unknown command. Did you mean ${ctx.commandSuggestion}?`);
     } else {
       await ctx.reply("Unknown command. Try /help");
     }
   });
```

Sources: [grammy.dev](https://grammy.dev/) | [grammy.dev/plugins/commands](https://grammy.dev/plugins/commands) | [grammy.dev/guide/errors](https://grammy.dev/guide/errors)

---

## Topic 2: Telegram Message Splitting Strategies

### Hard Limits

| Message type | Limit |
|---|---|
| Text message | **4096 UTF-8 characters** |
| Edit message | 4096 characters |
| Photo caption | 1024 characters |

Note: Telegram measures entity offsets in UTF-16 code units (emojis = 2 units). Splitting formatted messages requires offset recalculation.

### Splitting Strategy Comparison

| Strategy | When to use | Risk |
|---|---|---|
| Naive char boundary | Never | Cuts mid-word, breaks tags |
| Newline boundary | Plain text baseline | OK if no very long lines |
| Word boundary | Most plain text | Good default |
| Sentence boundary | Natural prose | More complex |
| Entity-aware (library) | Formatted text | Correct but needs dep |

### Recommended Plain Text Splitter

```typescript
function splitForTelegram(raw: string, limit = 4096): string[] {
  const text = raw.replace(/[ \t]+/g, " ").replace(/\n{3,}/g, "\n\n").trim();
  if (text.length <= limit) return [text];

  const chunks: string[] = [];
  let current = "";

  for (const para of text.split(/\n\n+/)) {
    const candidate = current ? `${current}\n\n${para}` : para;
    if (candidate.length <= limit) {
      current = candidate;
    } else {
      if (current) chunks.push(current);
      if (para.length <= limit) {
        current = para;
      } else {
        // word-split oversized paragraph
        current = "";
        for (const word of para.split(" ")) {
          const sep = current ? " " : "";
          if ((current + sep + word).length <= limit) {
            current += sep + word;
          } else {
            if (current) chunks.push(current);
            current = word.substring(0, limit);
          }
        }
      }
    }
  }
  if (current) chunks.push(current);
  return chunks;
}
```

For formatted text: use `@gramio/split` which handles entity offset recalculation across boundaries. Or avoid entirely by sending as file attachment for very long outputs.

### Rate Limiting

| Scenario | Limit |
|---|---|
| Same chat | ~1 message/second |
| Broadcast to different chats | ~30 messages/second |
| Same group | ~20 messages/minute |

On 429 error, Telegram returns `retry_after` seconds. During that wait, **all bot users are blocked** — not just the triggering user.

Best practice: do NOT add `sleep` between chunks. Use grammY plugins:
- `@grammyjs/transformer-throttler` — queues requests via Bottleneck
- `@grammyjs/auto-retry` — retries on 429 with `retry_after` delay

```typescript
import { apiThrottler } from "@grammyjs/transformer-throttler";
import { autoRetry }    from "@grammyjs/auto-retry";

bot.api.config.use(apiThrottler());
bot.api.config.use(autoRetry());
```

Sources: [grammy.dev/advanced/flood](https://grammy.dev/advanced/flood) | [gramio.dev/plugins/official/split](https://gramio.dev/plugins/official/split) | [limits.tginfo.me](https://limits.tginfo.me/en)

---

## Testing Preferences (New Project)

No existing test setup. Recommendations for TypeScript bot:
- **Unit tests:** Vitest (or Jest) — fast, ESM-native, good TS support
- **Integration tests:** Grammytest (`@grammyjs/grammytest`) — in-memory bot testing without real Telegram connection
- Pattern: test command handlers as pure functions (extract handler logic from bot framework)

---

## Key Decisions for Implementation

1. **Use grammY** as the Telegram bot framework (best TypeScript support, actively maintained)
2. **`CommandGroup` from `@grammyjs/commands`** for command registration and Telegram menu
3. **`ctx.match` + whitespace split** for positional argument parsing
4. **Plain text responses** from Claude (as per spec prompts) — use newline/word boundary splitter
5. **`@grammyjs/transformer-throttler` + `@grammyjs/auto-retry`** for rate limit handling when sending split messages
6. **Part numbering** optional — add `[1/3]` prefix on multi-part Claude responses for UX
