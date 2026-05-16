import { describe, it, expect } from "bun:test"
import { runWizard } from "../../src/config/wizard"
import { FriendlyError } from "../../src/shared/errors"

describe("runWizard", () => {
  it("throws FriendlyError in non-interactive (non-TTY) environment", async () => {
    // In test runner, process.stdin.isTTY is falsy
    let err: unknown
    try { await runWizard() } catch (e) { err = e }
    expect(err).toBeInstanceOf(FriendlyError)
  })

  it.todo("prompts for telegram.bot_token and validates format")
  it.todo("prompts for jira.base_url and validates HTTPS URL")
  it.todo("prompts for jira.api_token (non-empty)")
  it.todo("prompts for jira.email and validates format")
  it.todo("prompts for jira.project_key and validates uppercase")
  it.todo("auto-fills claude.binary_path from PATH when not in existing config")
  it.todo("preserves existing config values as initial values")
  it.todo("throws FriendlyError on Ctrl+C cancel")
  it.todo("returns AppConfig without writing to disk")
})
