diff --git a/02-integration-clients/index.ts b/02-integration-clients/index.ts
index 3808ca2..1bf15c6 100644
--- a/02-integration-clients/index.ts
+++ b/02-integration-clients/index.ts
@@ -3,7 +3,7 @@ export * from './src/telegram/TelegramClient'
 export * from './src/telegram/types'
 export * from './src/telegram/splitMessage'
 
-// TODO (section-04): export * from './src/jira/JiraClient'
-// TODO (section-04): export * from './src/jira/types'
+export * from './src/jira/JiraClient'
+export * from './src/jira/types'
 // TODO (section-05): export * from './src/claude/ClaudeClient'
 // TODO (section-05): export * from './src/claude/types'
diff --git a/02-integration-clients/src/jira/JiraClient.ts b/02-integration-clients/src/jira/JiraClient.ts
new file mode 100644
index 0000000..5fd5772
--- /dev/null
+++ b/02-integration-clients/src/jira/JiraClient.ts
@@ -0,0 +1,138 @@
+import { toADF, adfToText, type AdfNode } from './adf'
+import type { JiraConfig, JiraIssue } from './types'
+import {
+  JiraAuthError,
+  JiraPermissionError,
+  JiraNotFoundError,
+  JiraRateLimitError,
+  JiraServerError,
+  JiraTimeoutError,
+  InvalidTransitionError,
+} from '../errors'
+
+// TODO: replace with import from 01-core-daemon when available
+interface Logger {
+  info(obj: object): void
+  error(obj: object): void
+}
+
+export class JiraClient {
+  private readonly authHeader: string
+  private readonly baseUrl: string
+
+  constructor(private readonly config: JiraConfig, private readonly logger: Logger) {
+    this.authHeader = 'Basic ' + btoa(`${config.email}:${config.apiToken}`)
+    this.baseUrl = `https://${config.host}/rest/api/3`
+  }
+
+  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
+    const url = `${this.baseUrl}/${path}`
+    const controller = new AbortController()
+    const timeoutMs = this.config.requestTimeoutMs ?? 15000
+    const timer = setTimeout(() => controller.abort(), timeoutMs)
+    const start = Date.now()
+
+    try {
+      const response = await fetch(url, {
+        method,
+        headers: {
+          Authorization: this.authHeader,
+          'Content-Type': 'application/json',
+          Accept: 'application/json',
+        },
+        signal: controller.signal,
+        body: body !== undefined ? JSON.stringify(body) : undefined,
+      })
+
+      const durationMs = Date.now() - start
+      this.logger.info({ method, path, status: response.status, durationMs })
+
+      if (response.ok) {
+        return response.json() as Promise<T>
+      }
+
+      if (response.status === 401) throw new JiraAuthError()
+      if (response.status === 403) throw new JiraPermissionError()
+      if (response.status === 404) throw new JiraNotFoundError(this.extractIssueKey(path))
+      if (response.status === 429) {
+        const retryAfter = parseInt(response.headers.get('Retry-After') ?? '', 10)
+        throw new JiraRateLimitError(isNaN(retryAfter) ? undefined : retryAfter)
+      }
+      if (response.status >= 500) throw new JiraServerError(response.status)
+
+      throw new Error(`Jira request failed: ${response.status}`)
+    } catch (err) {
+      if ((err as { name?: string }).name === 'AbortError') {
+        throw new JiraTimeoutError()
+      }
+      throw err
+    } finally {
+      clearTimeout(timer)
+    }
+  }
+
+  private extractIssueKey(path: string): string {
+    const match = path.match(/issue\/([^/]+)/)
+    return match ? decodeURIComponent(match[1]) : path
+  }
+
+  async createIssue(title: string, description: string): Promise<JiraIssue> {
+    const response = await this.request<{ key: string }>('POST', 'issue', {
+      fields: {
+        project: { key: this.config.projectKey },
+        issuetype: { name: this.config.issueType ?? 'Task' },
+        summary: title,
+        description: toADF(description),
+      },
+    })
+    return this.getIssue(response.key)
+  }
+
+  async getIssue(issueKey: string): Promise<JiraIssue> {
+    const response = await this.request<{
+      key: string
+      fields: {
+        summary: string
+        status: { name: string }
+        description: AdfNode | null
+      }
+    }>('GET', `issue/${encodeURIComponent(issueKey)}`)
+
+    return {
+      key: response.key,
+      summary: response.fields.summary,
+      status: response.fields.status.name,
+      description: adfToText(response.fields.description),
+      url: `https://${this.config.host}/browse/${response.key}`,
+    }
+  }
+
+  async transitionIssue(issueKey: string, targetStatus: string): Promise<void> {
+    const encoded = encodeURIComponent(issueKey)
+    const response = await this.request<{ transitions: Array<{ id: string; name: string }> }>(
+      'GET',
+      `issue/${encoded}/transitions`
+    )
+
+    const transition = response.transitions.find(
+      (t) => t.name.toLowerCase() === targetStatus.toLowerCase()
+    )
+
+    if (!transition) {
+      throw new InvalidTransitionError(
+        targetStatus,
+        response.transitions.map((t) => t.name)
+      )
+    }
+
+    await this.request('POST', `issue/${encoded}/transitions`, {
+      transition: { id: transition.id },
+    })
+  }
+
+  async addComment(issueKey: string, body: string): Promise<void> {
+    await this.request('POST', `issue/${encodeURIComponent(issueKey)}/comment`, {
+      body: toADF(body),
+    })
+  }
+}
diff --git a/02-integration-clients/src/jira/types.ts b/02-integration-clients/src/jira/types.ts
new file mode 100644
index 0000000..b39c4ee
--- /dev/null
+++ b/02-integration-clients/src/jira/types.ts
@@ -0,0 +1,16 @@
+export interface JiraConfig {
+  host: string
+  email: string
+  apiToken: string
+  projectKey: string
+  issueType?: string
+  requestTimeoutMs?: number
+}
+
+export interface JiraIssue {
+  key: string
+  summary: string
+  status: string
+  description: string
+  url: string
+}
diff --git a/02-integration-clients/tests/jira.test.ts b/02-integration-clients/tests/jira.test.ts
new file mode 100644
index 0000000..8934f08
--- /dev/null
+++ b/02-integration-clients/tests/jira.test.ts
@@ -0,0 +1,298 @@
+import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest'
+import { JiraClient } from '../src/jira/JiraClient'
+import {
+  JiraAuthError,
+  JiraPermissionError,
+  JiraNotFoundError,
+  JiraRateLimitError,
+  JiraServerError,
+  JiraTimeoutError,
+  InvalidTransitionError,
+} from '../src/errors'
+
+const config = {
+  host: 'test.atlassian.net',
+  email: 'user@example.com',
+  apiToken: 'secret-token',
+  projectKey: 'PROJ',
+  issueType: 'Task',
+  requestTimeoutMs: 5000,
+}
+
+const mockLogger = { info: vi.fn(), error: vi.fn() }
+
+function makeResponse(status: number, body: unknown, headers: Record<string, string> = {}) {
+  return {
+    ok: status >= 200 && status < 300,
+    status,
+    headers: { get: (key: string) => headers[key] ?? null },
+    json: () => Promise.resolve(body),
+    text: () => Promise.resolve(JSON.stringify(body)),
+  }
+}
+
+function stubFetch(response: ReturnType<typeof makeResponse>) {
+  return vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response))
+}
+
+function getLastFetchCall() {
+  return (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]
+}
+
+beforeEach(() => { vi.clearAllMocks() })
+afterEach(() => { vi.unstubAllGlobals() })
+
+// ─── Error mapping ──────────────────────────────────────────────────────────
+
+describe('error mapping', () => {
+  it('HTTP 401 → JiraAuthError', async () => {
+    stubFetch(makeResponse(401, {}))
+    const client = new JiraClient(config, mockLogger)
+    await expect(client.getIssue('PROJ-1')).rejects.toBeInstanceOf(JiraAuthError)
+  })
+
+  it('HTTP 403 → JiraPermissionError', async () => {
+    stubFetch(makeResponse(403, {}))
+    const client = new JiraClient(config, mockLogger)
+    await expect(client.getIssue('PROJ-1')).rejects.toBeInstanceOf(JiraPermissionError)
+  })
+
+  it('HTTP 404 → JiraNotFoundError', async () => {
+    stubFetch(makeResponse(404, {}))
+    const client = new JiraClient(config, mockLogger)
+    await expect(client.getIssue('PROJ-1')).rejects.toBeInstanceOf(JiraNotFoundError)
+  })
+
+  it('HTTP 429 with Retry-After header → JiraRateLimitError with retryAfter', async () => {
+    stubFetch(makeResponse(429, {}, { 'Retry-After': '30' }))
+    const client = new JiraClient(config, mockLogger)
+    try {
+      await client.getIssue('PROJ-1')
+      expect.fail('should have thrown')
+    } catch (err) {
+      expect(err).toBeInstanceOf(JiraRateLimitError)
+      expect((err as JiraRateLimitError).retryAfter).toBe(30)
+    }
+  })
+
+  it('HTTP 503 → JiraServerError with status 503', async () => {
+    stubFetch(makeResponse(503, {}))
+    const client = new JiraClient(config, mockLogger)
+    try {
+      await client.getIssue('PROJ-1')
+      expect.fail('should have thrown')
+    } catch (err) {
+      expect(err).toBeInstanceOf(JiraServerError)
+      expect((err as JiraServerError).status).toBe(503)
+    }
+  })
+
+  it('AbortError → JiraTimeoutError', async () => {
+    const abortErr = new DOMException('AbortError', 'AbortError')
+    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(abortErr))
+    const client = new JiraClient({ ...config, requestTimeoutMs: 1 }, mockLogger)
+    await expect(client.getIssue('PROJ-1')).rejects.toBeInstanceOf(JiraTimeoutError)
+  })
+})
+
+// ─── createIssue ───────────────────────────────────────────────────────────
+
+describe('createIssue', () => {
+  const createdKey = 'PROJ-99'
+  const getIssueBody = {
+    key: createdKey,
+    fields: {
+      summary: 'My title',
+      status: { name: 'To Do' },
+      description: null,
+    },
+  }
+
+  function setupCreateFetch(issueType?: string) {
+    const cfg = { ...config, issueType: issueType ?? 'Task' }
+    vi.stubGlobal('fetch', vi.fn()
+      .mockResolvedValueOnce(makeResponse(201, { id: '1', key: createdKey, self: 'url' }))
+      .mockResolvedValueOnce(makeResponse(200, getIssueBody))
+    )
+    return new JiraClient(cfg, mockLogger)
+  }
+
+  it('POSTs to /rest/api/3/issue with correct fields', async () => {
+    const client = setupCreateFetch()
+    await client.createIssue('My title', 'desc')
+
+    const [url, opts] = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0]
+    expect(url).toBe('https://test.atlassian.net/rest/api/3/issue')
+    expect(opts.method).toBe('POST')
+    const body = JSON.parse(opts.body)
+    expect(body.fields.project.key).toBe('PROJ')
+    expect(body.fields.issuetype.name).toBe('Task')
+    expect(body.fields.summary).toBe('My title')
+    expect(body.fields.description.type).toBe('doc')
+  })
+
+  it('uses config.issueType when set', async () => {
+    const client = setupCreateFetch('Bug')
+    await client.createIssue('Bug report', 'details')
+    const body = JSON.parse((globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls[0][1].body)
+    expect(body.fields.issuetype.name).toBe('Bug')
+  })
+
+  it('makes follow-up GET and returns full JiraIssue', async () => {
+    const client = setupCreateFetch()
+    const issue = await client.createIssue('My title', 'desc')
+    expect(issue.key).toBe(createdKey)
+    expect(issue.summary).toBe('My title')
+  })
+})
+
+// ─── getIssue ──────────────────────────────────────────────────────────────
+
+describe('getIssue', () => {
+  const issueBody = {
+    key: 'PROJ-5',
+    fields: {
+      summary: 'The summary',
+      status: { name: 'In Progress' },
+      description: {
+        type: 'doc',
+        content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Hello' }] }],
+      },
+    },
+  }
+
+  it('URL-encodes issueKey in the path', async () => {
+    stubFetch(makeResponse(200, { ...issueBody, key: 'PROJ-5 weird' }))
+    const client = new JiraClient(config, mockLogger)
+    await client.getIssue('PROJ-5 weird').catch(() => {})
+    const [url] = getLastFetchCall()
+    expect(url).toContain('PROJ-5%20weird')
+  })
+
+  it('maps summary, status, description, and url', async () => {
+    stubFetch(makeResponse(200, issueBody))
+    const client = new JiraClient(config, mockLogger)
+    const issue = await client.getIssue('PROJ-5')
+    expect(issue.summary).toBe('The summary')
+    expect(issue.status).toBe('In Progress')
+    expect(issue.description).toContain('Hello')
+    expect(issue.url).toBe('https://test.atlassian.net/browse/PROJ-5')
+  })
+
+  it('null description → empty string', async () => {
+    stubFetch(makeResponse(200, { ...issueBody, fields: { ...issueBody.fields, description: null } }))
+    const client = new JiraClient(config, mockLogger)
+    const issue = await client.getIssue('PROJ-5')
+    expect(issue.description).toBe('')
+  })
+})
+
+// ─── transitionIssue ───────────────────────────────────────────────────────
+
+describe('transitionIssue', () => {
+  const transitions = {
+    transitions: [
+      { id: '11', name: 'To Do' },
+      { id: '21', name: 'In Progress' },
+      { id: '31', name: 'Done' },
+    ],
+  }
+
+  function setupTransitionFetch() {
+    vi.stubGlobal('fetch', vi.fn()
+      .mockResolvedValueOnce(makeResponse(200, transitions))
+      .mockResolvedValueOnce(makeResponse(204, {}))
+    )
+    return new JiraClient(config, mockLogger)
+  }
+
+  it('GETs transitions then POSTs with matched transition id', async () => {
+    const client = setupTransitionFetch()
+    await client.transitionIssue('PROJ-1', 'Done')
+
+    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls
+    expect(calls[0][0]).toContain('/transitions')
+    expect(calls[0][1].method).toBe('GET')
+    const postBody = JSON.parse(calls[1][1].body)
+    expect(postBody.transition.id).toBe('31')
+  })
+
+  it('transition name matching is case-insensitive', async () => {
+    const client = setupTransitionFetch()
+    await client.transitionIssue('PROJ-1', 'in progress')
+    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls
+    const postBody = JSON.parse(calls[1][1].body)
+    expect(postBody.transition.id).toBe('21')
+  })
+
+  it('no matching transition → throws InvalidTransitionError', async () => {
+    const client = setupTransitionFetch()
+    try {
+      await client.transitionIssue('PROJ-1', 'Nonexistent')
+      expect.fail('should have thrown')
+    } catch (err) {
+      expect(err).toBeInstanceOf(InvalidTransitionError)
+      expect((err as InvalidTransitionError).attempted).toBe('Nonexistent')
+      expect((err as InvalidTransitionError).available).toEqual(['To Do', 'In Progress', 'Done'])
+    }
+  })
+
+  it('URL-encodes issueKey in all path segments', async () => {
+    vi.stubGlobal('fetch', vi.fn()
+      .mockResolvedValueOnce(makeResponse(200, transitions))
+      .mockResolvedValueOnce(makeResponse(204, {}))
+    )
+    const client = new JiraClient(config, mockLogger)
+    await client.transitionIssue('PROJ 1', 'Done')
+    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls
+    expect(calls[0][0]).toContain('PROJ%201')
+    expect(calls[1][0]).toContain('PROJ%201')
+  })
+})
+
+// ─── addComment ────────────────────────────────────────────────────────────
+
+describe('addComment', () => {
+  it('POSTs ADF body to /issue/{key}/comment', async () => {
+    stubFetch(makeResponse(201, {}))
+    const client = new JiraClient(config, mockLogger)
+    await client.addComment('PROJ-1', 'Hello comment')
+
+    const [url, opts] = getLastFetchCall()
+    expect(url).toContain('/comment')
+    expect(opts.method).toBe('POST')
+    const body = JSON.parse(opts.body)
+    expect(body.body.type).toBe('doc')
+    expect(JSON.stringify(body.body)).toContain('Hello comment')
+  })
+})
+
+// ─── Logging ───────────────────────────────────────────────────────────────
+
+describe('logging', () => {
+  it('logs method, path, status, durationMs after each request', async () => {
+    stubFetch(makeResponse(200, {
+      key: 'PROJ-1',
+      fields: { summary: 's', status: { name: 'Done' }, description: null },
+    }))
+    const client = new JiraClient(config, mockLogger)
+    await client.getIssue('PROJ-1')
+    expect(mockLogger.info).toHaveBeenCalled()
+    const logCall = mockLogger.info.mock.calls[0][0] as Record<string, unknown>
+    expect(logCall).toHaveProperty('method')
+    expect(logCall).toHaveProperty('status')
+    expect(logCall).toHaveProperty('durationMs')
+  })
+
+  it('apiToken never appears in logs', async () => {
+    stubFetch(makeResponse(200, {
+      key: 'PROJ-1',
+      fields: { summary: 's', status: { name: 'Done' }, description: null },
+    }))
+    const client = new JiraClient(config, mockLogger)
+    await client.getIssue('PROJ-1')
+    for (const call of mockLogger.info.mock.calls) {
+      expect(JSON.stringify(call)).not.toContain('secret-token')
+    }
+  })
+})
