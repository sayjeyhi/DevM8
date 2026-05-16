import { describe, it, expect, mock, beforeEach, afterEach } from "bun:test"
import { JiraClient } from "../../src/jira/JiraClient"
import {
  JiraAuthError,
  JiraPermissionError,
  JiraNotFoundError,
  JiraRateLimitError,
  JiraServerError,
  JiraTimeoutError,
  InvalidTransitionError,
} from "../../src/shared/errors"

const config = {
  host: "test.atlassian.net",
  email: "user@example.com",
  apiToken: "secret-token",
  projectKey: "PROJ",
  issueType: "Task",
  requestTimeoutMs: 5000,
}

const mockLogger = { info: mock(), error: mock() }

function makeResponse(status: number, body: unknown, headers: Record<string, string> = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: (key: string) => headers[key] ?? null },
    json: () => Promise.resolve(body),
    text: () => Promise.resolve(JSON.stringify(body)),
  }
}

let savedFetch: typeof globalThis.fetch

beforeEach(() => {
  savedFetch = globalThis.fetch
  mockLogger.info.mockClear()
  mockLogger.error.mockClear()
})

afterEach(() => {
  globalThis.fetch = savedFetch
})

function stubFetch(response: ReturnType<typeof makeResponse>) {
  globalThis.fetch = mock().mockResolvedValue(response) as unknown as typeof fetch
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function getLastFetchCall(): [string, RequestInit] {
  return (globalThis.fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls[0]
}

// ─── Error mapping ──────────────────────────────────────────────────────────

describe("error mapping", () => {
  it("HTTP 401 → JiraAuthError", async () => {
    stubFetch(makeResponse(401, {}))
    const client = new JiraClient(config, mockLogger)
    await expect(client.getIssue("PROJ-1")).rejects.toBeInstanceOf(JiraAuthError)
  })

  it("HTTP 403 → JiraPermissionError", async () => {
    stubFetch(makeResponse(403, {}))
    const client = new JiraClient(config, mockLogger)
    await expect(client.getIssue("PROJ-1")).rejects.toBeInstanceOf(JiraPermissionError)
  })

  it("HTTP 404 → JiraNotFoundError", async () => {
    stubFetch(makeResponse(404, {}))
    const client = new JiraClient(config, mockLogger)
    await expect(client.getIssue("PROJ-1")).rejects.toBeInstanceOf(JiraNotFoundError)
  })

  it("HTTP 429 with Retry-After header → JiraRateLimitError with retryAfter", async () => {
    stubFetch(makeResponse(429, {}, { "Retry-After": "30" }))
    const client = new JiraClient(config, mockLogger)
    const err = await client.getIssue("PROJ-1").catch(e => e)
    expect(err).toBeInstanceOf(JiraRateLimitError)
    expect((err as JiraRateLimitError).retryAfter).toBe(30)
  })

  it("HTTP 503 → JiraServerError with status 503", async () => {
    stubFetch(makeResponse(503, {}))
    const client = new JiraClient(config, mockLogger)
    const err = await client.getIssue("PROJ-1").catch(e => e)
    expect(err).toBeInstanceOf(JiraServerError)
    expect((err as JiraServerError).status).toBe(503)
  })

  it("AbortError → JiraTimeoutError", async () => {
    const abortErr = new DOMException("AbortError", "AbortError")
    globalThis.fetch = mock().mockRejectedValue(abortErr) as unknown as typeof fetch
    const client = new JiraClient({ ...config, requestTimeoutMs: 1 }, mockLogger)
    await expect(client.getIssue("PROJ-1")).rejects.toBeInstanceOf(JiraTimeoutError)
  })
})

// ─── createIssue ───────────────────────────────────────────────────────────

describe("createIssue", () => {
  const createdKey = "PROJ-99"
  const getIssueBody = {
    key: createdKey,
    fields: {
      summary: "My title",
      status: { name: "To Do" },
      description: null,
    },
  }

  function setupCreateFetch(issueType?: string) {
    const cfg = { ...config, issueType: issueType ?? "Task" }
    globalThis.fetch = mock()
      .mockResolvedValueOnce(makeResponse(201, { id: "1", key: createdKey, self: "url" }))
      .mockResolvedValueOnce(makeResponse(200, getIssueBody)) as unknown as typeof fetch
    return new JiraClient(cfg, mockLogger)
  }

  it("POSTs to /rest/api/3/issue with correct fields", async () => {
    const client = setupCreateFetch()
    await client.createIssue("My title", "desc")

    const [url, opts] = (globalThis.fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls[0]
    expect(url).toBe("https://test.atlassian.net/rest/api/3/issue")
    expect(opts.method).toBe("POST")
    const body = JSON.parse(opts.body as string)
    expect(body.fields.project.key).toBe("PROJ")
    expect(body.fields.issuetype.name).toBe("Task")
    expect(body.fields.summary).toBe("My title")
    expect(body.fields.description.type).toBe("doc")
  })

  it("uses config.issueType when set", async () => {
    const client = setupCreateFetch("Bug")
    await client.createIssue("Bug report", "details")
    const body = JSON.parse(
      (globalThis.fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls[0][1].body as string
    )
    expect(body.fields.issuetype.name).toBe("Bug")
  })

  it("makes follow-up GET and returns full JiraIssue", async () => {
    const client = setupCreateFetch()
    const issue = await client.createIssue("My title", "desc")
    expect(issue.key).toBe(createdKey)
    expect(issue.summary).toBe("My title")
  })
})

// ─── getIssue ──────────────────────────────────────────────────────────────

describe("getIssue", () => {
  const issueBody = {
    key: "PROJ-5",
    fields: {
      summary: "The summary",
      status: { name: "In Progress" },
      description: {
        type: "doc",
        content: [{ type: "paragraph", content: [{ type: "text", text: "Hello" }] }],
      },
    },
  }

  it("URL-encodes issueKey in the path", async () => {
    stubFetch(makeResponse(200, { ...issueBody, key: "PROJ-5 weird" }))
    const client = new JiraClient(config, mockLogger)
    await client.getIssue("PROJ-5 weird").catch(() => {})
    const [url] = getLastFetchCall()
    expect(url).toContain("PROJ-5%20weird")
  })

  it("maps summary, status, description, and url", async () => {
    stubFetch(makeResponse(200, issueBody))
    const client = new JiraClient(config, mockLogger)
    const issue = await client.getIssue("PROJ-5")
    expect(issue.summary).toBe("The summary")
    expect(issue.status).toBe("In Progress")
    expect(issue.description).toContain("Hello")
    expect(issue.url).toBe("https://test.atlassian.net/browse/PROJ-5")
  })

  it("null description → empty string", async () => {
    stubFetch(makeResponse(200, { ...issueBody, fields: { ...issueBody.fields, description: null } }))
    const client = new JiraClient(config, mockLogger)
    const issue = await client.getIssue("PROJ-5")
    expect(issue.description).toBe("")
  })
})

// ─── transitionIssue ───────────────────────────────────────────────────────

describe("transitionIssue", () => {
  const transitions = {
    transitions: [
      { id: "11", name: "To Do" },
      { id: "21", name: "In Progress" },
      { id: "31", name: "Done" },
    ],
  }

  function setupTransitionFetch() {
    globalThis.fetch = mock()
      .mockResolvedValueOnce(makeResponse(200, transitions))
      .mockResolvedValueOnce(makeResponse(204, {})) as unknown as typeof fetch
    return new JiraClient(config, mockLogger)
  }

  it("GETs transitions then POSTs with matched transition id", async () => {
    const client = setupTransitionFetch()
    await client.transitionIssue("PROJ-1", "Done")

    const calls = (globalThis.fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls
    expect(calls[0][0]).toContain("/transitions")
    expect(calls[0][1].method).toBe("GET")
    const postBody = JSON.parse(calls[1][1].body as string)
    expect(postBody.transition.id).toBe("31")
  })

  it("transition name matching is case-insensitive", async () => {
    const client = setupTransitionFetch()
    await client.transitionIssue("PROJ-1", "in progress")
    const calls = (globalThis.fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls
    const postBody = JSON.parse(calls[1][1].body as string)
    expect(postBody.transition.id).toBe("21")
  })

  it("no matching transition → throws InvalidTransitionError", async () => {
    const client = setupTransitionFetch()
    const err = await client.transitionIssue("PROJ-1", "Nonexistent").catch(e => e)
    expect(err).toBeInstanceOf(InvalidTransitionError)
    expect((err as InvalidTransitionError).attempted).toBe("Nonexistent")
    expect((err as InvalidTransitionError).available).toEqual(["To Do", "In Progress", "Done"])
  })

  it("URL-encodes issueKey in all path segments", async () => {
    globalThis.fetch = mock()
      .mockResolvedValueOnce(makeResponse(200, transitions))
      .mockResolvedValueOnce(makeResponse(204, {})) as unknown as typeof fetch
    const client = new JiraClient(config, mockLogger)
    await client.transitionIssue("PROJ 1", "Done")
    const calls = (globalThis.fetch as unknown as { mock: { calls: [string, RequestInit][] } }).mock.calls
    expect(calls[0][0]).toContain("PROJ%201")
    expect(calls[1][0]).toContain("PROJ%201")
  })
})

// ─── addComment ────────────────────────────────────────────────────────────

describe("addComment", () => {
  it("POSTs ADF body to /issue/{key}/comment", async () => {
    stubFetch(makeResponse(201, {}))
    const client = new JiraClient(config, mockLogger)
    await client.addComment("PROJ-1", "Hello comment")

    const [url, opts] = getLastFetchCall()
    expect(url).toContain("/comment")
    expect(opts.method).toBe("POST")
    const body = JSON.parse(opts.body as string)
    expect(body.body.type).toBe("doc")
    expect(JSON.stringify(body.body)).toContain("Hello comment")
  })
})

describe("204 No Content handling", () => {
  it("transitionIssue does not throw on 204 response body", async () => {
    globalThis.fetch = mock()
      .mockResolvedValueOnce(makeResponse(200, { transitions: [{ id: "1", name: "Done" }] }))
      .mockResolvedValueOnce({ ok: true, status: 204, headers: { get: () => null }, json: () => Promise.reject(new SyntaxError("no body")) }) as unknown as typeof fetch
    const client = new JiraClient(config, mockLogger)
    await expect(client.transitionIssue("PROJ-1", "Done")).resolves.toBeUndefined()
  })
})

// ─── Logging ───────────────────────────────────────────────────────────────

describe("logging", () => {
  it("logs method, path, status, durationMs after each request", async () => {
    stubFetch(makeResponse(200, {
      key: "PROJ-1",
      fields: { summary: "s", status: { name: "Done" }, description: null },
    }))
    const client = new JiraClient(config, mockLogger)
    await client.getIssue("PROJ-1")
    expect(mockLogger.info).toHaveBeenCalled()
    const logCall = (mockLogger.info.mock.calls[0] as [Record<string, unknown>])[0]
    expect(logCall).toHaveProperty("method")
    expect(logCall).toHaveProperty("status")
    expect(logCall).toHaveProperty("durationMs")
  })

  it("apiToken never appears in any log calls", async () => {
    stubFetch(makeResponse(200, {
      key: "PROJ-1",
      fields: { summary: "s", status: { name: "Done" }, description: null },
    }))
    const client = new JiraClient(config, mockLogger)
    await client.getIssue("PROJ-1")
    const allCalls = [...mockLogger.info.mock.calls, ...mockLogger.error.mock.calls]
    for (const call of allCalls) {
      expect(JSON.stringify(call)).not.toContain("secret-token")
    }
  })
})
