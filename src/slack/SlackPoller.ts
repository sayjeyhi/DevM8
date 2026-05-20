import type { SlackClient } from "./SlackClient"
import type { SlackNewMessage } from "./types"
import { loadSlackState, saveSlackState } from "./state"

export type MessageHandler = (msg: SlackNewMessage) => Promise<void>

export class SlackPoller {
  private handler?: MessageHandler
  private readonly userCache = new Map<string, string>()

  constructor(
    private readonly client: SlackClient,
    private readonly intervalMs: number,
  ) {}

  setMessageHandler(handler: MessageHandler): void {
    this.handler = handler
  }

  async start(signal: AbortSignal): Promise<void> {
    while (!signal.aborted) {
      try {
        await this.poll()
      } catch {
        // retry on next tick
      }
      await Bun.sleep(this.intervalMs)
    }
  }

  private async resolveUsername(userId: string): Promise<string> {
    const cached = this.userCache.get(userId)
    if (cached) return cached
    try {
      const user = await this.client.getUserInfo(userId)
      const name = user.profile?.display_name || user.real_name || user.name
      this.userCache.set(userId, name)
      return name
    } catch {
      return userId
    }
  }

  private async poll(): Promise<void> {
    if (!this.handler) return

    const state = await loadSlackState()
    const channels = await this.client.listImChannels()
    const nowTs = (Date.now() / 1000).toFixed(6)
    let stateChanged = false

    for (const channel of channels) {
      const lastTs = state.lastTs[channel.id]

      if (!lastTs) {
        state.lastTs[channel.id] = nowTs
        stateChanged = true
        continue
      }

      const messages = await this.client.getHistory(channel.id, lastTs)
      if (messages.length === 0) continue

      // getHistory returns newest-first; process oldest-first
      const sorted = messages.slice().reverse()

      for (const msg of sorted) {
        if (!msg.user) continue
        const senderName = await this.resolveUsername(msg.user)
        await this.handler({ channel, message: msg, senderName })
      }

      // advance cursor to newest message (first in original array)
      state.lastTs[channel.id] = messages[0].ts
      stateChanged = true
    }

    if (stateChanged) await saveSlackState(state)
  }
}
