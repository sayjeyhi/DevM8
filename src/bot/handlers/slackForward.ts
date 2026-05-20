import type { Bot, Context } from "grammy"
import type { SlackClient } from "../../slack/SlackClient"
import type { ClaudeClient } from "../../claude/ClaudeClient"
import type { SlackNewMessage } from "../../slack/types"
import { escapeHtml } from "../commands/my-tickets"

interface CallbackButton {
  text: string
  callback_data: string
}

interface PendingSlackAction {
  channelId: string
  ts: string
  aiDraft?: string
}

export const pendingSlackReplies = new Map<number, PendingSlackAction>()

function formatForTelegram(msg: SlackNewMessage): string {
  const sender = escapeHtml(msg.senderName)
  const text = escapeHtml(msg.message.text)
  const type = msg.channel.is_mpim ? "Group DM" : "DM"
  return `📨 <b>Slack ${type} from @${sender}</b>\n<code>─────────────────</code>\n${text}`
}

function slackKeyboard(channelId: string, ts: string): { inline_keyboard: CallbackButton[][] } {
  return {
    inline_keyboard: [[
      { text: "↩ Reply", callback_data: `slack:reply:${channelId}:${ts}` },
      { text: "🤖 Answer with AI", callback_data: `slack:ai:${channelId}:${ts}` },
    ]],
  }
}

export function createSlackForwardHandler(
  allowedUserIds: number[],
  sendMessage: (chatId: number, text: string, options: object) => Promise<unknown>,
) {
  return async (msg: SlackNewMessage): Promise<void> => {
    const text = formatForTelegram(msg)
    const reply_markup = slackKeyboard(msg.channel.id, msg.message.ts)
    for (const chatId of allowedUserIds) {
      try {
        await sendMessage(chatId, text, { parse_mode: "HTML", reply_markup })
      } catch {
        // non-fatal — user may not have started bot yet
      }
    }
  }
}

export function registerSlackHandlers(bot: Bot, slack: SlackClient, claude: ClaudeClient): void {
  // Must be registered before registerCommands so it intercepts text first
  bot.on("message:text", async (ctx: Context, next: () => Promise<void>) => {
    const chatId = ctx.chat?.id
    if (!chatId) return next()

    const pending = pendingSlackReplies.get(chatId)
    if (!pending) return next()

    pendingSlackReplies.delete(chatId)
    const text = (ctx.message as { text?: string })?.text ?? ""

    try {
      await ctx.replyWithChatAction("typing")
      await slack.postMessage(pending.channelId, text, pending.ts)
      await ctx.reply("✅ Sent to Slack")
    } catch (err) {
      await ctx.reply(`❌ Failed to send: ${(err as Error).message}`)
    }
  })

  bot.callbackQuery(/^slack:reply:([^:]+):(\d+\.\d+)$/, async ctx => {
    const [, channelId, ts] = ctx.match as RegExpMatchArray
    const chatId = ctx.chat?.id
    if (!chatId) return ctx.answerCallbackQuery()

    pendingSlackReplies.set(chatId, { channelId, ts })
    await ctx.answerCallbackQuery("Type your reply below")
    await ctx.reply("✏️ Type your reply to Slack:")
  })

  bot.callbackQuery(/^slack:ai:([^:]+):(\d+\.\d+)$/, async ctx => {
    const [, channelId, ts] = ctx.match as RegExpMatchArray
    const chatId = ctx.chat?.id
    if (!chatId) return ctx.answerCallbackQuery()

    await ctx.answerCallbackQuery("Generating AI response…")
    await ctx.replyWithChatAction("typing")

    const originalMsg = await slack.getMessageByTs(channelId, ts)
    const msgText = originalMsg?.text ?? "(message unavailable)"
    const prompt = `Draft a concise, professional reply to this Slack message:\n\n${msgText}`

    let draft: string
    try {
      draft = await claude.ask(prompt)
    } catch (err) {
      await ctx.reply(`❌ AI failed: ${(err as Error).message}`)
      return
    }

    const aiKeyboard: { inline_keyboard: CallbackButton[][] } = {
      inline_keyboard: [[
        { text: "✅ Send", callback_data: `slack:send:${channelId}:${ts}` },
        { text: "✏️ Edit", callback_data: `slack:edit:${channelId}:${ts}` },
        { text: "❌ Cancel", callback_data: "slack:cancel" },
      ]],
    }

    await ctx.reply(`🤖 <b>AI Draft:</b>\n\n${escapeHtml(draft)}`, {
      parse_mode: "HTML",
      reply_markup: aiKeyboard,
    })

    pendingSlackReplies.set(chatId, { channelId, ts, aiDraft: draft })
  })

  bot.callbackQuery(/^slack:send:([^:]+):(\d+\.\d+)$/, async ctx => {
    const chatId = ctx.chat?.id
    if (!chatId) return ctx.answerCallbackQuery("Expired")

    const pending = pendingSlackReplies.get(chatId)
    if (!pending?.aiDraft) return ctx.answerCallbackQuery("Draft expired — try again")

    pendingSlackReplies.delete(chatId)

    try {
      await slack.postMessage(pending.channelId, pending.aiDraft, pending.ts)
      await ctx.answerCallbackQuery("Sent!")
      await ctx.editMessageReplyMarkup()
      await ctx.reply("✅ AI reply sent to Slack")
    } catch (err) {
      await ctx.answerCallbackQuery("Send failed")
      await ctx.reply(`❌ Failed: ${(err as Error).message}`)
    }
  })

  bot.callbackQuery(/^slack:edit:([^:]+):(\d+\.\d+)$/, async ctx => {
    const [, channelId, ts] = ctx.match as RegExpMatchArray
    const chatId = ctx.chat?.id
    if (!chatId) return ctx.answerCallbackQuery()

    pendingSlackReplies.set(chatId, { channelId, ts })
    await ctx.answerCallbackQuery("Send your edited version below")
    await ctx.reply("✏️ Send your edited reply:")
  })

  bot.callbackQuery("slack:cancel", async ctx => {
    const chatId = ctx.chat?.id
    if (chatId) pendingSlackReplies.delete(chatId)
    await ctx.answerCallbackQuery("Cancelled")
    await ctx.editMessageReplyMarkup()
  })
}
