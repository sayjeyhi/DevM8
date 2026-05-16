export interface JiraConfig {
  host: string
  email: string
  apiToken: string
  projectKey: string
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
