import type { Context } from "grammy"

export function keepTyping(ctx: Context): () => void {
  ctx.replyWithChatAction("typing").catch(() => {})
  const interval = setInterval(() => {
    ctx.replyWithChatAction("typing").catch(() => {})
  }, 4000)
  return () => clearInterval(interval)
}
