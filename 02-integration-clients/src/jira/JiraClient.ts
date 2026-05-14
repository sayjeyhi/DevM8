import { toADF, adfToText, type AdfNode } from './adf'
import type { JiraConfig, JiraIssue } from './types'
import {
  JiraAuthError,
  JiraPermissionError,
  JiraNotFoundError,
  JiraRateLimitError,
  JiraServerError,
  JiraTimeoutError,
  InvalidTransitionError,
} from '../errors'

// TODO: replace with import from 01-core-daemon when available
interface Logger {
  info(obj: object): void
  error(obj: object): void
}

export class JiraClient {
  private readonly authHeader: string
  private readonly baseUrl: string

  constructor(private readonly config: JiraConfig, private readonly logger: Logger) {
    this.authHeader = 'Basic ' + btoa(`${config.email}:${config.apiToken}`)
    this.baseUrl = `https://${config.host}/rest/api/3`
  }

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const url = `${this.baseUrl}/${path}`
    const controller = new AbortController()
    const timeoutMs = this.config.requestTimeoutMs ?? 15000
    const timer = setTimeout(() => controller.abort(), timeoutMs)
    const start = Date.now()

    try {
      const response = await fetch(url, {
        method,
        headers: {
          Authorization: this.authHeader,
          'Content-Type': 'application/json',
          Accept: 'application/json',
        },
        signal: controller.signal,
        body: body !== undefined ? JSON.stringify(body) : undefined,
      })

      const durationMs = Date.now() - start
      this.logger.info({ method, path, status: response.status, durationMs })

      if (response.ok) {
        if (response.status === 204) return undefined as T
        return response.json() as Promise<T>
      }

      if (response.status === 401) throw new JiraAuthError()
      if (response.status === 403) throw new JiraPermissionError()
      if (response.status === 404) throw new JiraNotFoundError(this.extractIssueKey(path))
      if (response.status === 429) {
        const retryAfter = parseInt(response.headers.get('Retry-After') ?? '', 10)
        throw new JiraRateLimitError(isNaN(retryAfter) ? undefined : retryAfter)
      }
      if (response.status >= 500) throw new JiraServerError(response.status)

      throw new Error(`Jira request failed: ${response.status}`)
    } catch (err) {
      if ((err as { name?: string }).name === 'AbortError' || controller.signal.aborted) {
        throw new JiraTimeoutError()
      }
      throw err
    } finally {
      clearTimeout(timer)
    }
  }

  private extractIssueKey(path: string): string {
    const match = path.match(/issue\/([^/]+)/)
    return match ? decodeURIComponent(match[1]) : path
  }

  async createIssue(title: string, description: string): Promise<JiraIssue> {
    const response = await this.request<{ key: string }>('POST', 'issue', {
      fields: {
        project: { key: this.config.projectKey },
        issuetype: { name: this.config.issueType ?? 'Task' },
        summary: title,
        description: toADF(description),
      },
    })
    return this.getIssue(response.key)
  }

  async getIssue(issueKey: string): Promise<JiraIssue> {
    const response = await this.request<{
      key: string
      fields: {
        summary: string
        status: { name: string }
        description: AdfNode | null
      }
    }>('GET', `issue/${encodeURIComponent(issueKey)}`)

    return {
      key: response.key,
      summary: response.fields.summary,
      status: response.fields.status.name,
      description: adfToText(response.fields.description),
      url: `https://${this.config.host}/browse/${response.key}`,
    }
  }

  async transitionIssue(issueKey: string, targetStatus: string): Promise<void> {
    const encoded = encodeURIComponent(issueKey)
    const response = await this.request<{ transitions: Array<{ id: string; name: string }> }>(
      'GET',
      `issue/${encoded}/transitions`
    )

    const transition = response.transitions.find(
      (t) => t.name.toLowerCase() === targetStatus.toLowerCase()
    )

    if (!transition) {
      throw new InvalidTransitionError(
        targetStatus,
        response.transitions.map((t) => t.name)
      )
    }

    await this.request('POST', `issue/${encoded}/transitions`, {
      transition: { id: transition.id },
    })
  }

  async addComment(issueKey: string, body: string): Promise<void> {
    await this.request('POST', `issue/${encodeURIComponent(issueKey)}/comment`, {
      body: toADF(body),
    })
  }
}
