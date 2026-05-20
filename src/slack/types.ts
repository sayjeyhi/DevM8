export interface SlackMessage {
  type: string
  user?: string
  bot_id?: string
  app_id?: string
  text: string
  ts: string
  thread_ts?: string
  subtype?: string
}

export interface SlackChannel {
  id: string
  is_im: boolean
  is_mpim: boolean
  is_archived?: boolean
  user?: string
  name?: string
}

export interface SlackUser {
  id: string
  name: string
  real_name?: string
  profile?: {
    display_name?: string
    real_name?: string
  }
}

export interface SlackAuthTestResult {
  ok: boolean
  user_id: string
  user: string
  team: string
  team_id: string
}

export interface SlackNewMessage {
  channel: SlackChannel
  message: SlackMessage
  senderName: string
}
