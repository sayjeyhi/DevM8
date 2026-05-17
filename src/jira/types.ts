export interface JiraConfig {
  host: string
  email: string
  apiToken: string
  projectKeys: string[]
  issueType?: string
  requestTimeoutMs?: number
}

export interface JiraIssue {
  key: string
  summary: string
  status: string
  description: string
  url: string
}
