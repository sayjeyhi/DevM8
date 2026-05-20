import { mock, describe, it, expect, spyOn, beforeEach, afterEach } from "bun:test"
import type { AppConfig } from "../../src/config/schema"

// Module-level state controlling mock behavior — updated per-test
type TextOpts = { message: string; initialValue?: string; validate?: (v: string) => string | undefined }
type MultiselectOpts = { message: string; options: Array<{ value: string; label: string }>; initialValues?: string[]; required?: boolean }

let _textImpl: (opts: TextOpts) => Promise<unknown> =
  ({ initialValue }) => Promise.resolve(initialValue ?? "")

let _multiselectImpl: (opts: MultiselectOpts) => Promise<unknown> =
  (opts) => Promise.resolve(opts.initialValues ?? opts.options.slice(0, 1).map(o => o.value))

let _isCancelImpl: (v: unknown) => boolean = () => false

// Hoisted before imports by Bun — wizard.ts gets the mock @clack/prompts
mock.module("@clack/prompts", () => ({
  intro: () => {},
  outro: () => {},
  spinner: () => ({ start: () => {}, stop: () => {} }),
  isCancel: (v: unknown) => _isCancelImpl(v),
  text: (o: TextOpts) => _textImpl(o),
  multiselect: (o: MultiselectOpts) => _multiselectImpl(o),
}))

import { runWizard } from "../../src/config/wizard"
import { FriendlyError } from "../../src/shared/errors"
import {
  validateBotToken,
  validateJiraBaseUrl,
  validateApiToken,
  validateEmail,
  validateProjectKeys,
  validateRepoPaths,
} from "../../src/config/validators"

let whichSpy: ReturnType<typeof spyOn<typeof Bun, "which">>

// Default fetchProjects mock: returns [] (text fallback path)
const noProjects = async () => []

beforeEach(() => {
  // Default: gh not found (skip spawn check), claude not found (use initialValue)
  whichSpy = spyOn(Bun, "which").mockReturnValue(null)
  _multiselectImpl = (opts) => Promise.resolve(opts.initialValues ?? opts.options.slice(0, 1).map(o => o.value))
  _isCancelImpl = () => false
})

afterEach(() => {
  _textImpl = ({ initialValue }) => Promise.resolve(initialValue ?? "")
  whichSpy.mockRestore()
})

// Helper: force isTTY for tests that need the wizard to proceed past the TTY check
async function withTTY<T>(value: boolean, fn: () => Promise<T>): Promise<T> {
  const saved = process.stdin.isTTY
  ;(process.stdin as any).isTTY = value
  try { return await fn() } finally { ;(process.stdin as any).isTTY = saved }
}

// Valid values that produce a well-formed AppConfig when returned by the text mock
const VALID_VALUES: Record<string, string> = {
  "Telegram bot token": "123456:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh",
  "Your Telegram user ID(s)": "12345",
  "Jira base URL": "https://myco.atlassian.net",
  "Jira API token": "mytoken",
  "Jira account email": "user@example.com",
  "Jira project keys": "MYPROJECT",
  "Path to claude binary": "/usr/bin/true",
  "Anthropic API key": "",
}

function validTextImpl({ message, initialValue }: TextOpts): Promise<string> {
  for (const [substr, val] of Object.entries(VALID_VALUES)) {
    if (message.includes(substr)) return Promise.resolve(val)
  }
  return Promise.resolve(initialValue ?? "")
}

describe("runWizard", () => {
  it("throws FriendlyError in non-interactive (non-TTY) environment", async () => {
    let err: unknown
    try { await withTTY(false, () => runWizard()) } catch (e) { err = e }
    expect(err).toBeInstanceOf(FriendlyError)
    expect((err as FriendlyError).message).toContain("non-interactive")
  })

  it("prompts for telegram.bot_token and validates format", () => {
    expect(validateBotToken("123456:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef")).toBeUndefined()
    expect(validateBotToken("")).toBeTypeOf("string")
    expect(validateBotToken("notabot")).toBeTypeOf("string")
    expect(validateBotToken("123:short")).toBeTypeOf("string")
  })

  it("prompts for jira.base_url and validates HTTPS URL", () => {
    expect(validateJiraBaseUrl("https://myco.atlassian.net")).toBeUndefined()
    expect(validateJiraBaseUrl("http://myco.atlassian.net")).toBeTypeOf("string")
    expect(validateJiraBaseUrl("not-a-url")).toBeTypeOf("string")
    expect(validateJiraBaseUrl("")).toBeTypeOf("string")
  })

  it("prompts for jira.api_token (non-empty)", () => {
    expect(validateApiToken("any-token")).toBeUndefined()
    expect(validateApiToken("")).toBeTypeOf("string")
  })

  it("prompts for jira.email and validates format", () => {
    expect(validateEmail("user@example.com")).toBeUndefined()
    expect(validateEmail("not-an-email")).toBeTypeOf("string")
    expect(validateEmail("")).toBeTypeOf("string")
    expect(validateEmail("missing@tld")).toBeTypeOf("string")
  })

  it("prompts for jira.project_keys and validates comma-separated keys", () => {
    expect(validateProjectKeys("MYPROJECT")).toBeUndefined()
    expect(validateProjectKeys("MP,BZ")).toBeUndefined()
    expect(validateProjectKeys("myproject")).toBeTypeOf("string")
    expect(validateProjectKeys("")).toBeTypeOf("string")
    expect(validateProjectKeys("123PROJ")).toBeTypeOf("string")
  })

  it("validates repo paths: blank is valid (skip), existing dirs pass, missing dirs fail", () => {
    expect(validateRepoPaths("")).toBeUndefined()
    expect(validateRepoPaths("   ")).toBeUndefined()
    expect(validateRepoPaths("/nonexistent/path/abc123")).toBeTypeOf("string")
    expect(validateRepoPaths("/tmp")).toBeUndefined()
    expect(validateRepoPaths("/tmp,/nonexistent/path")).toBeTypeOf("string")
  })

  it("auto-fills claude.binary_path from PATH when not in existing config", async () => {
    whichSpy.mockImplementation((bin: string) => bin === "claude" ? "/auto/claude" : null)
    const captured: Record<string, string | undefined> = {}
    _textImpl = ({ message, initialValue }) => {
      captured[message] = initialValue
      return Promise.resolve(initialValue ?? "")
    }

    await withTTY(true, () => runWizard(undefined, noProjects))

    expect(whichSpy).toHaveBeenCalledWith("claude")
    expect(captured["Path to claude binary"]).toBe("/auto/claude")
  })

  it("preserves existing config values as initial values", async () => {
    const captured: Record<string, string | undefined> = {}
    _textImpl = ({ message, initialValue }) => {
      captured[message] = initialValue
      return Promise.resolve(initialValue ?? "")
    }

    const existing: AppConfig = {
      telegram: { bot_token: "111111:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef", allowed_user_ids: [42, 99] },
      jira: { base_url: "https://co.atlassian.net", api_token: "tok", email: "a@b.com", project_keys: ["PROJ"] },
      claude: { binary_path: "/my/claude", api_key: "sk-ant-123" },
      repos: { PROJ: ["/tmp"] },
      app: { log_level: "info" },
    }

    await withTTY(true, () => runWizard(existing, noProjects))

    expect(captured["Telegram bot token"]).toBe("111111:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef")
    expect(captured["Jira base URL (e.g. https://mycompany.atlassian.net)"]).toBe("https://co.atlassian.net")
    expect(captured["Jira API token"]).toBe("tok")
    expect(captured["Jira account email"]).toBe("a@b.com")
    expect(captured["Jira project keys, comma-separated (e.g. MP,BZ)"]).toBe("PROJ")
    expect(captured["Path to claude binary"]).toBe("/my/claude")
    expect(captured["Your Telegram user ID(s), comma-separated (send /start to @userinfobot to get yours)"]).toBe("42, 99")
    expect(captured["Repo paths for PROJ (comma-separated, leave blank to skip)"]).toBe("/tmp")
  })

  it("saves repos per project key when paths provided", async () => {
    _textImpl = (opts) => {
      if (opts.message.includes("Repo paths for MYPROJECT")) return Promise.resolve("/tmp")
      return validTextImpl(opts)
    }

    const result = await withTTY(true, () => runWizard(undefined, noProjects))

    expect(result.repos).toEqual({ MYPROJECT: ["/tmp"] })
  })

  it("omits repos from config when all paths left blank", async () => {
    _textImpl = validTextImpl

    const result = await withTTY(true, () => runWizard(undefined, noProjects))

    expect(result.repos).toBeUndefined()
  })

  it("throws FriendlyError on Ctrl+C cancel", async () => {
    const CANCEL = Symbol("cancel")
    _textImpl = () => Promise.resolve(CANCEL)
    _isCancelImpl = (v: unknown) => v === CANCEL

    let err: unknown
    try { await withTTY(true, () => runWizard(undefined, noProjects)) } catch (e) { err = e }
    expect(err).toBeInstanceOf(FriendlyError)
  })

  it("returns AppConfig without writing to disk", async () => {
    _textImpl = validTextImpl

    const result = await withTTY(true, () => runWizard(undefined, noProjects))

    expect(result).toMatchObject({
      telegram: expect.objectContaining({ bot_token: expect.any(String), allowed_user_ids: expect.any(Array) }),
      jira: expect.objectContaining({
        base_url: expect.any(String),
        api_token: expect.any(String),
        email: expect.any(String),
        project_keys: expect.any(Array),
      }),
      claude: expect.objectContaining({ binary_path: expect.any(String) }),
      app: expect.objectContaining({ log_level: expect.any(String) }),
    })
  })

  it("uses multiselect when fetchProjectsFn returns non-empty list", async () => {
    const fakeProjects = [{ key: "MP", name: "Main Project" }, { key: "BZ", name: "Blaze" }]
    const selectedKeys: string[][] = []
    _multiselectImpl = (opts) => {
      selectedKeys.push(opts.options.map(o => o.value))
      return Promise.resolve([opts.options[0].value])
    }
    _textImpl = validTextImpl

    const result = await withTTY(true, () =>
      runWizard(undefined, async () => fakeProjects)
    )

    expect(selectedKeys.length).toBeGreaterThan(0)
    expect(result.jira.project_keys).toEqual(["MP"])
  })

  it("falls back to text prompt when fetchProjectsFn returns null", async () => {
    _textImpl = validTextImpl

    const result = await withTTY(true, () =>
      runWizard(undefined, async () => null)
    )

    expect(result.jira.project_keys).toEqual(["MYPROJECT"])
  })

  it("falls back to text prompt when fetchProjectsFn returns empty list", async () => {
    _textImpl = validTextImpl

    const result = await withTTY(true, () =>
      runWizard(undefined, noProjects)
    )

    expect(result.jira.project_keys).toEqual(["MYPROJECT"])
  })

  it("proceeds without error when gh is not installed", async () => {
    whichSpy.mockReturnValue(null)
    _textImpl = validTextImpl
    await expect(withTTY(true, () => runWizard(undefined, noProjects))).resolves.toBeDefined()
  })
})
