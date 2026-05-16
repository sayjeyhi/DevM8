import { describe, it, expect } from "bun:test"
import {
  JiraAuthError,
  JiraPermissionError,
  JiraNotFoundError,
  JiraRateLimitError,
  JiraServerError,
  JiraTimeoutError,
  InvalidTransitionError,
  ClaudeTimeoutError,
  ClaudeExitError,
} from "../../src/shared/errors"

describe("JiraAuthError", () => {
  it("is instanceof Error with correct type", () => {
    const err = new JiraAuthError()
    expect(err).toBeInstanceOf(Error)
    expect(err.type).toBe("JIRA_AUTH")
    expect(err.message).toBe("Jira authentication failed")
    expect(err.name).toBe("JiraAuthError")
  })

  it("accepts custom message", () => {
    const err = new JiraAuthError("custom")
    expect(err.message).toBe("custom")
  })
})

describe("JiraPermissionError", () => {
  it("is instanceof Error with correct type", () => {
    const err = new JiraPermissionError()
    expect(err).toBeInstanceOf(Error)
    expect(err.type).toBe("JIRA_PERMISSION")
    expect(err.name).toBe("JiraPermissionError")
    expect(err.message).toBe("Jira permission denied")
  })
})

describe("JiraNotFoundError", () => {
  it("carries issueKey", () => {
    const err = new JiraNotFoundError("PROJ-123")
    expect(err).toBeInstanceOf(Error)
    expect(err.type).toBe("JIRA_NOT_FOUND")
    expect(err.issueKey).toBe("PROJ-123")
    expect(err.message).toBe("Issue PROJ-123 not found")
    expect(err.name).toBe("JiraNotFoundError")
  })

  it("accepts custom message", () => {
    const err = new JiraNotFoundError("PROJ-1", "gone")
    expect(err.message).toBe("gone")
  })
})

describe("JiraRateLimitError", () => {
  it("carries optional retryAfter", () => {
    const withRetry = new JiraRateLimitError(30)
    expect(withRetry).toBeInstanceOf(Error)
    expect(withRetry.type).toBe("JIRA_RATE_LIMIT")
    expect(withRetry.retryAfter).toBe(30)

    const withoutRetry = new JiraRateLimitError()
    expect(withoutRetry.retryAfter).toBeUndefined()
  })
})

describe("JiraServerError", () => {
  it("carries status code", () => {
    const err = new JiraServerError(500)
    expect(err).toBeInstanceOf(Error)
    expect(err.type).toBe("JIRA_SERVER")
    expect(err.status).toBe(500)
    expect(err.name).toBe("JiraServerError")
  })
})

describe("JiraTimeoutError", () => {
  it("is instanceof Error with correct type", () => {
    const err = new JiraTimeoutError()
    expect(err).toBeInstanceOf(Error)
    expect(err.type).toBe("JIRA_TIMEOUT")
    expect(err.message).toBe("Jira request timed out")
    expect(err.name).toBe("JiraTimeoutError")
  })
})

describe("InvalidTransitionError", () => {
  it("carries attempted and available[]", () => {
    const err = new InvalidTransitionError("Close", ["Resolve", "Reopen"])
    expect(err).toBeInstanceOf(Error)
    expect(err.type).toBe("INVALID_TRANSITION")
    expect(err.attempted).toBe("Close")
    expect(err.available).toEqual(["Resolve", "Reopen"])
    expect(err.name).toBe("InvalidTransitionError")
  })
})

describe("ClaudeTimeoutError", () => {
  it("carries timeoutMs", () => {
    const err = new ClaudeTimeoutError(5000)
    expect(err).toBeInstanceOf(Error)
    expect(err.type).toBe("CLAUDE_TIMEOUT")
    expect(err.timeoutMs).toBe(5000)
    expect(err.name).toBe("ClaudeTimeoutError")
  })
})

describe("ClaudeExitError", () => {
  it("carries exitCode and stderr", () => {
    const err = new ClaudeExitError(1, "error output")
    expect(err).toBeInstanceOf(Error)
    expect(err.type).toBe("CLAUDE_EXIT")
    expect(err.exitCode).toBe(1)
    expect(err.stderr).toBe("error output")
    expect(err.name).toBe("ClaudeExitError")
  })
})
