import { describe, it, expect } from "bun:test"
import { FriendlyError, LaunchctlError, launchctlHint } from "../../src/shared/errors"

describe("FriendlyError", () => {
  it("is an instance of Error", () => {
    const err = new FriendlyError("test message")
    expect(err).toBeInstanceOf(Error)
  })

  it("exposes hint property", () => {
    const err = new FriendlyError("test message", "try this")
    expect(err.hint).toBe("try this")
  })

  it("hint is undefined when not provided", () => {
    const err = new FriendlyError("test message")
    expect(err.hint).toBeUndefined()
  })

  it("message is accessible via .message", () => {
    const err = new FriendlyError("hello world")
    expect(err.message).toBe("hello world")
  })
})

describe("LaunchctlError", () => {
  it("is an instance of FriendlyError", () => {
    const err = new LaunchctlError("some stderr", "some hint")
    expect(err).toBeInstanceOf(FriendlyError)
  })

  it("exposes rawOutput property", () => {
    const err = new LaunchctlError("stderr output here", "hint text")
    expect(err.rawOutput).toBe("stderr output here")
  })

  it("hint is accessible", () => {
    const err = new LaunchctlError("stderr", "my hint")
    expect(err.hint).toBe("my hint")
  })
})

describe("launchctlHint", () => {
  it("maps 'No such file or directory' to jira-assistant start hint", () => {
    const hint = launchctlHint("error: No such file or directory")
    expect(hint).toContain("jira-assistant start")
  })

  it("maps 'Operation already in progress' to jira-assistant status hint", () => {
    const hint = launchctlHint("error: Operation already in progress")
    expect(hint).toContain("jira-assistant status")
  })

  it("maps 'Permission denied' to file permissions hint", () => {
    const hint = launchctlHint("error: Permission denied")
    expect(hint).toContain("plist")
  })

  it("returns fallback hint for unknown errors", () => {
    const hint = launchctlHint("some unknown error")
    expect(hint).toBe("launchctl exited with a non-zero status")
  })
})
