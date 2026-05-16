import { describe, it, expect } from "bun:test"
import { toADF, adfToText, type AdfNode } from "../../src/jira/adf"

describe("toADF", () => {
  it("returns doc with version 1 and type doc", () => {
    const doc = toADF("hello")
    expect(doc.type).toBe("doc")
    expect(doc.version).toBe(1)
  })

  it("single line → one paragraph with one text node", () => {
    const doc = toADF("hello")
    expect(doc.content).toHaveLength(1)
    expect(doc.content![0].type).toBe("paragraph")
    expect(doc.content![0].content![0]).toEqual({ type: "text", text: "hello" })
  })

  it("two lines → two paragraph nodes", () => {
    const doc = toADF("a\nline2")
    expect(doc.content).toHaveLength(2)
    expect(doc.content![0].content![0]).toEqual({ type: "text", text: "a" })
    expect(doc.content![1].content![0]).toEqual({ type: "text", text: "line2" })
  })

  it("empty line between two lines → two paragraphs (empty line skipped)", () => {
    const doc = toADF("a\n\nb")
    expect(doc.content).toHaveLength(2)
  })

  it("empty string → doc with empty content array", () => {
    const doc = toADF("")
    expect(doc.type).toBe("doc")
    expect(doc.content).toHaveLength(0)
  })

  it("whitespace-only lines are skipped", () => {
    const doc = toADF("a\n   \nb")
    expect(doc.content).toHaveLength(2)
  })

  it("each paragraph has correct structure", () => {
    const doc = toADF("test")
    const para = doc.content![0]
    expect(para.type).toBe("paragraph")
    expect(Array.isArray(para.content)).toBe(true)
  })
})

describe("adfToText", () => {
  it("null input → returns \"\"", () => {
    expect(adfToText(null)).toBe("")
  })

  it("undefined input → returns \"\"", () => {
    expect(adfToText(undefined)).toBe("")
  })

  it("text node → returns its text value", () => {
    expect(adfToText({ type: "text", text: "hello" })).toBe("hello")
  })

  it("hardBreak node → returns \"\\n\"", () => {
    expect(adfToText({ type: "hardBreak" })).toBe("\n")
  })

  it("paragraph containing text node → returns text + trailing newline", () => {
    const para: AdfNode = { type: "paragraph", content: [{ type: "text", text: "hi" }] }
    expect(adfToText(para)).toBe("hi\n")
  })

  it("two paragraphs in a doc → text joined with newlines, trimmed", () => {
    const doc: AdfNode = {
      type: "doc",
      content: [
        { type: "paragraph", content: [{ type: "text", text: "line1" }] },
        { type: "paragraph", content: [{ type: "text", text: "line2" }] },
      ],
    }
    expect(adfToText(doc)).toBe("line1\n\nline2")
  })

  it("hardBreak inside paragraph → \\n in output", () => {
    const para: AdfNode = {
      type: "paragraph",
      content: [
        { type: "text", text: "a" },
        { type: "hardBreak" },
        { type: "text", text: "b" },
      ],
    }
    expect(adfToText(para)).toBe("a\nb\n")
  })

  it("bulletList with listItem nodes → recurses and returns item text", () => {
    const list: AdfNode = {
      type: "bulletList",
      content: [
        { type: "listItem", content: [{ type: "paragraph", content: [{ type: "text", text: "item1" }] }] },
        { type: "listItem", content: [{ type: "paragraph", content: [{ type: "text", text: "item2" }] }] },
      ],
    }
    const result = adfToText(list)
    expect(result).toContain("item1")
    expect(result).toContain("item2")
  })

  it("orderedList with listItem nodes → recurses and returns item text", () => {
    const list: AdfNode = {
      type: "orderedList",
      content: [
        { type: "listItem", content: [{ type: "paragraph", content: [{ type: "text", text: "first" }] }] },
      ],
    }
    expect(adfToText(list)).toContain("first")
  })

  it("codeBlock node → returns text content with trailing newline", () => {
    const code: AdfNode = {
      type: "codeBlock",
      content: [{ type: "text", text: "const x = 1" }],
    }
    const result = adfToText(code)
    expect(result).toContain("const x = 1")
    expect(result).toMatch(/\n$/)
  })

  it("mention with attrs.text → returns \"@<attrs.text>\"", () => {
    const mention: AdfNode = { type: "mention", attrs: { text: "Alice" } }
    expect(adfToText(mention)).toBe("@Alice")
  })

  it("mention without attrs.text → returns \"@user\"", () => {
    expect(adfToText({ type: "mention", attrs: {} })).toBe("@user")
    expect(adfToText({ type: "mention" })).toBe("@user")
  })

  it("unknown node type with content → recurses into content", () => {
    const node: AdfNode = {
      type: "unknownBlock",
      content: [{ type: "text", text: "inner" }],
    }
    expect(adfToText(node)).toBe("inner")
  })

  it("unknown node type without content → returns \"\"", () => {
    expect(adfToText({ type: "someUnknown" })).toBe("")
  })

  it("doc node → joins blocks and trims", () => {
    const doc: AdfNode = {
      type: "doc",
      content: [
        { type: "paragraph", content: [{ type: "text", text: "hello" }] },
      ],
    }
    expect(adfToText(doc)).toBe("hello")
  })

  it("real-world ADF: paragraph + bulletList + heading → correct flat text", () => {
    const doc: AdfNode = {
      type: "doc",
      content: [
        { type: "heading", content: [{ type: "text", text: "Title" }] },
        { type: "paragraph", content: [{ type: "text", text: "Intro text." }] },
        {
          type: "bulletList",
          content: [
            { type: "listItem", content: [{ type: "paragraph", content: [{ type: "text", text: "A" }] }] },
            { type: "listItem", content: [{ type: "paragraph", content: [{ type: "text", text: "B" }] }] },
          ],
        },
      ],
    }
    const result = adfToText(doc)
    expect(result).toContain("Title")
    expect(result).toContain("Intro text.")
    expect(result).toContain("A")
    expect(result).toContain("B")
  })
})

describe("round-trip", () => {
  it("toADF(adfToText(doc)) preserves text content", () => {
    const original = toADF("hello\nworld")
    const text = adfToText(original)
    const roundTripped = toADF(text)
    expect(adfToText(roundTripped)).toBe(text)
  })
})
