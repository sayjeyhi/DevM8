diff --git a/src/bot/utils/parseArgs.ts b/src/bot/utils/parseArgs.ts
index 336ce12..de9b1d8 100644
--- a/src/bot/utils/parseArgs.ts
+++ b/src/bot/utils/parseArgs.ts
@@ -1 +1,26 @@
-export {}
+import type { Context } from "grammy"
+
+/**
+ * Extracts positional arguments from ctx.match.
+ * Trims the match string, splits on whitespace, and filters empty strings.
+ * Returns [] if match is empty or undefined.
+ */
+export function parseArgs(ctx: Context): string[] {
+  const match = ctx.match
+  if (!match || typeof match !== "string") return []
+  return match
+    .trim()
+    .split(/\s+/)
+    .filter(s => s !== "")
+}
+
+/**
+ * Splits input into the first whitespace-delimited token and the raw remainder.
+ * Uses regex /^(\S+)\s+([\s\S]*)$/ — preserves all whitespace within the remainder.
+ * Returns null if input has only one token or is empty.
+ */
+export function parseFirstAndRest(input: string): { first: string; rest: string } | null {
+  const match = /^(\S+)\s+([\s\S]*)$/.exec(input)
+  if (!match) return null
+  return { first: match[1], rest: match[2] }
+}
diff --git a/src/bot/utils/splitMessage.ts b/src/bot/utils/splitMessage.ts
index 336ce12..a5a8dbf 100644
--- a/src/bot/utils/splitMessage.ts
+++ b/src/bot/utils/splitMessage.ts
@@ -1 +1,64 @@
-export {}
+const PREFIX_RESERVE = 8 // "[99/99] "
+
+/**
+ * Splits text into chunks that fit within `limit` characters (default 4096).
+ * Splits preferentially at \n\n paragraph boundaries, then at word boundaries,
+ * then hard-cuts as a last resort.
+ * When more than one chunk is produced, each chunk is prefixed [N/M].
+ * Reserves 8 characters per chunk for the prefix to ensure prefixed chunks
+ * never exceed `limit`.
+ */
+export function splitMessage(text: string, limit = 4096): string[] {
+  if (text.length <= limit) return [text]
+
+  const effectiveLimit = limit - PREFIX_RESERVE
+  const chunks: string[] = []
+
+  function pushLongText(str: string) {
+    let remaining = str
+    while (remaining.length > effectiveLimit) {
+      const slice = remaining.slice(0, effectiveLimit)
+      const lastSpace = slice.lastIndexOf(" ")
+      if (lastSpace > 0) {
+        chunks.push(remaining.slice(0, lastSpace))
+        remaining = remaining.slice(lastSpace + 1)
+      } else {
+        // Hard cut — prevents infinite loop on text with no spaces
+        chunks.push(remaining.slice(0, effectiveLimit))
+        remaining = remaining.slice(effectiveLimit)
+      }
+    }
+    if (remaining.length > 0) return remaining
+    return ""
+  }
+
+  const paragraphs = text.split("\n\n")
+  let current = ""
+
+  for (const para of paragraphs) {
+    if (para.length > effectiveLimit) {
+      if (current) {
+        chunks.push(current)
+        current = ""
+      }
+      current = pushLongText(para)
+    } else {
+      const candidate = current ? `${current}\n\n${para}` : para
+      if (candidate.length > effectiveLimit) {
+        if (current) chunks.push(current)
+        current = para
+      } else {
+        current = candidate
+      }
+    }
+  }
+
+  if (current) chunks.push(current)
+
+  if (chunks.length > 1) {
+    const total = chunks.length
+    return chunks.map((chunk, i) => `[${i + 1}/${total}] ${chunk}`)
+  }
+
+  return chunks
+}
diff --git a/tests/bot/utils/parseArgs.test.ts b/tests/bot/utils/parseArgs.test.ts
new file mode 100644
index 0000000..0d2757c
--- /dev/null
+++ b/tests/bot/utils/parseArgs.test.ts
@@ -0,0 +1,46 @@
+import { describe, it, expect } from "bun:test"
+import { parseArgs, parseFirstAndRest } from "../../../src/bot/utils/parseArgs"
+
+describe("parseArgs", () => {
+  function ctx(match: string) {
+    return { match } as never
+  }
+
+  it("empty match string → []", () => {
+    expect(parseArgs(ctx(""))).toEqual([])
+  })
+
+  it("single token", () => {
+    expect(parseArgs(ctx("ENG-1"))).toEqual(["ENG-1"])
+  })
+
+  it("multiple tokens split on whitespace", () => {
+    expect(parseArgs(ctx("ENG-1 In Progress"))).toEqual(["ENG-1", "In", "Progress"])
+  })
+
+  it("extra surrounding and internal whitespace trimmed and filtered", () => {
+    expect(parseArgs(ctx("  ENG-1   In  Progress  "))).toEqual(["ENG-1", "In", "Progress"])
+  })
+})
+
+describe("parseFirstAndRest", () => {
+  it("preserves multiple spaces in remainder", () => {
+    expect(parseFirstAndRest("ENG-1 Hello   world")).toEqual({
+      first: "ENG-1",
+      rest: "Hello   world",
+    })
+  })
+
+  it("single token → null", () => {
+    expect(parseFirstAndRest("ENG-1")).toBeNull()
+  })
+
+  it("empty string → null", () => {
+    expect(parseFirstAndRest("")).toBeNull()
+  })
+
+  it("trailing space after first token → { first, rest: '' }", () => {
+    // Regex /^(\S+)\s+([\s\S]*)$/ matches: rest is empty string, not null
+    expect(parseFirstAndRest("ENG-1 ")).toEqual({ first: "ENG-1", rest: "" })
+  })
+})
diff --git a/tests/bot/utils/splitMessage.test.ts b/tests/bot/utils/splitMessage.test.ts
new file mode 100644
index 0000000..e2dc45e
--- /dev/null
+++ b/tests/bot/utils/splitMessage.test.ts
@@ -0,0 +1,86 @@
+import { describe, it, expect } from "bun:test"
+import { splitMessage } from "../../../src/bot/utils/splitMessage"
+
+const LIMIT = 4096
+
+describe("splitMessage", () => {
+  it("short text returned as single element without prefix", () => {
+    const result = splitMessage("hello world")
+    expect(result).toHaveLength(1)
+    expect(result[0]).toBe("hello world")
+  })
+
+  it("text at exact limit returned as single element without prefix", () => {
+    const text = "a".repeat(LIMIT)
+    const result = splitMessage(text)
+    expect(result).toHaveLength(1)
+    expect(result[0]).toBe(text)
+  })
+
+  it("text one char over limit → two prefixed chunks", () => {
+    const text = "a".repeat(LIMIT + 1)
+    const result = splitMessage(text)
+    expect(result).toHaveLength(2)
+    expect(result[0]).toMatch(/^\[1\/2\]/)
+    expect(result[1]).toMatch(/^\[2\/2\]/)
+    for (const chunk of result) {
+      expect(chunk.length).toBeLessThanOrEqual(LIMIT)
+    }
+  })
+
+  it("splits at \\n\\n boundaries preserving double newlines in output", () => {
+    const para1 = "a".repeat(100)
+    const para2 = "b".repeat(100)
+    const para3 = "c".repeat(100)
+    const text = `${para1}\n\n${para2}\n\n${para3}`
+    const result = splitMessage(text, 250)
+    expect(result.length).toBeGreaterThan(1)
+    const joined = result.map(c => c.replace(/^\[\d+\/\d+\] /, "")).join("\n\n")
+    expect(joined).toContain("aaa")
+    expect(joined).toContain("bbb")
+    expect(joined).toContain("ccc")
+  })
+
+  it("splits at last space when no paragraph boundary available", () => {
+    const words = Array.from({ length: 20 }, (_, i) => `word${i}`.padEnd(10, "x"))
+    const text = words.join(" ")
+    const result = splitMessage(text, 100)
+    expect(result.length).toBeGreaterThan(1)
+    for (const chunk of result) {
+      expect(chunk.length).toBeLessThanOrEqual(100)
+    }
+  })
+
+  it("hard cuts a word with no spaces (no infinite loop)", () => {
+    const bigWord = "x".repeat(300)
+    const result = splitMessage(bigWord, 100)
+    expect(result.length).toBeGreaterThan(1)
+    for (const chunk of result) {
+      expect(chunk.length).toBeLessThanOrEqual(100)
+    }
+  })
+
+  it("10-part split has correct [N/10] prefixes", () => {
+    // Create text that forces 10+ splits at limit=100
+    const text = Array.from({ length: 10 }, () => "a".repeat(90)).join(" ")
+    const result = splitMessage(text, 100)
+    expect(result.length).toBeGreaterThanOrEqual(10)
+    const total = result.length
+    result.forEach((chunk, i) => {
+      expect(chunk.startsWith(`[${i + 1}/${total}]`)).toBe(true)
+    })
+  })
+
+  it("prefixed chunks never exceed LIMIT characters", () => {
+    const text = "word ".repeat(1500)
+    const result = splitMessage(text)
+    for (const chunk of result) {
+      expect(chunk.length).toBeLessThanOrEqual(LIMIT)
+    }
+  })
+
+  it("empty string returns ['']", () => {
+    // Defined behavior: empty input → single empty element (no split needed)
+    expect(splitMessage("")).toEqual([""])
+  })
+})
