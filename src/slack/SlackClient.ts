import type { SlackAuthTestResult, SlackChannel, SlackMessage, SlackUser } from "./types"

const BASE = "https://slack.com/api"

export class SlackClient {
  constructor(private readonly token: string) {}

  private async get<T>(method: string, params: Record<string, string> = {}): Promise<T> {
    const url = new URL(`${BASE}/${method}`)
    for (const [k, v] of Object.entries(params)) url.searchParams.set(k, v)
    const res = await fetch(url.toString(), {
      headers: { Authorization: `Bearer ${this.token}` },
    })
    if (!res.ok) throw new Error(`Slack HTTP ${res.status} on ${method}`)
    const data = await res.json() as { ok: boolean; error?: string } & T
    if (!data.ok) throw new Error(`Slack API error on ${method}: ${data.error}`)
    return data
  }

  private async post<T>(method: string, body: Record<string, string>): Promise<T> {
    const res = await fetch(`${BASE}/${method}`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${this.token}`,
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: new URLSearchParams(body).toString(),
    })
    if (!res.ok) throw new Error(`Slack HTTP ${res.status} on ${method}`)
    const data = await res.json() as { ok: boolean; error?: string } & T
    if (!data.ok) throw new Error(`Slack API error on ${method}: ${data.error}`)
    return data
  }

  async authTest(): Promise<SlackAuthTestResult> {
    return this.get<SlackAuthTestResult>("auth.test")
  }

  async listImChannels(): Promise<SlackChannel[]> {
    type Resp = { channels: SlackChannel[]; response_metadata?: { next_cursor?: string } }
    const channels: SlackChannel[] = []
    let cursor = ""
    do {
      const params: Record<string, string> = { types: "im,mpim", limit: "200", exclude_archived: "true" }
      if (cursor) params.cursor = cursor
      const data = await this.get<Resp>("conversations.list", params)
      channels.push(...data.channels)
      cursor = data.response_metadata?.next_cursor ?? ""
    } while (cursor)
    return channels
  }

  async getHistory(channelId: string, oldest?: string, limit = 50): Promise<SlackMessage[]> {
    type Resp = { messages: SlackMessage[] }
    const params: Record<string, string> = { channel: channelId, limit: String(limit) }
    if (oldest) params.oldest = oldest
    const data = await this.get<Resp>("conversations.history", params)
    return data.messages.filter(m => !m.subtype && !m.bot_id && !m.app_id)
  }

  async getMessageByTs(channelId: string, ts: string): Promise<SlackMessage | null> {
    type Resp = { messages: SlackMessage[] }
    try {
      const params: Record<string, string> = { channel: channelId, latest: ts, limit: "1", inclusive: "true" }
      const data = await this.get<Resp>("conversations.history", params)
      return data.messages[0] ?? null
    } catch {
      return null
    }
  }

  async getUserInfo(userId: string): Promise<SlackUser> {
    type Resp = { user: SlackUser }
    const data = await this.get<Resp>("users.info", { user: userId })
    return data.user
  }

  async postMessage(channelId: string, text: string, threadTs?: string): Promise<void> {
    const body: Record<string, string> = { channel: channelId, text }
    if (threadTs) body.thread_ts = threadTs
    await this.post("chat.postMessage", body)
  }
}
