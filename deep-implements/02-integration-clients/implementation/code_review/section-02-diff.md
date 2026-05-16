diff --git a/02-integration-clients/src/jira/adf.ts b/02-integration-clients/src/jira/adf.ts
new file mode 100644
index 0000000..16999ca
--- /dev/null
+++ b/02-integration-clients/src/jira/adf.ts
@@ -0,0 +1,56 @@
+export interface AdfNode {
+  type: string
+  text?: string
+  content?: AdfNode[]
+  attrs?: Record<string, unknown>
+  version?: number
+}
+
+export function toADF(text: string): AdfNode {
+  const paragraphs = text
+    .split('\n')
+    .filter((line) => line.length > 0)
+    .map((line) => ({
+      type: 'paragraph',
+      content: [{ type: 'text', text: line }],
+    }))
+
+  return { version: 1, type: 'doc', content: paragraphs }
+}
+
+export function adfToText(node: AdfNode | null | undefined): string {
+  if (node == null) return ''
+
+  switch (node.type) {
+    case 'text':
+      return node.text ?? ''
+
+    case 'hardBreak':
+      return '\n'
+
+    case 'mention':
+      return `@${node.attrs?.text || 'user'}`
+
+    case 'paragraph':
+    case 'heading':
+    case 'blockquote':
+    case 'listItem':
+      return (node.content ?? []).map(adfToText).join('') + '\n'
+
+    case 'bulletList':
+    case 'orderedList':
+      return (node.content ?? []).map(adfToText).join('')
+
+    case 'codeBlock':
+      return (node.content ?? []).map(adfToText).join('') + '\n'
+
+    case 'doc':
+      return (node.content ?? []).map(adfToText).join('\n').trim()
+
+    default:
+      if (node.content) {
+        return (node.content).map(adfToText).join('')
+      }
+      return ''
+  }
+}
diff --git a/02-integration-clients/tests/adf.test.ts b/02-integration-clients/tests/adf.test.ts
new file mode 100644
index 0000000..faf9170
--- /dev/null
+++ b/02-integration-clients/tests/adf.test.ts
@@ -0,0 +1,175 @@
+import { describe, it, expect } from 'vitest'
+import { toADF, adfToText, type AdfNode } from '../src/jira/adf'
+
+describe('toADF', () => {
+  it('returns doc with version 1 and type doc', () => {
+    const doc = toADF('hello')
+    expect(doc.type).toBe('doc')
+    expect((doc as AdfNode & { version: number }).version).toBe(1)
+  })
+
+  it('single line → one paragraph with one text node', () => {
+    const doc = toADF('hello')
+    expect(doc.content).toHaveLength(1)
+    expect(doc.content![0].type).toBe('paragraph')
+    expect(doc.content![0].content![0]).toEqual({ type: 'text', text: 'hello' })
+  })
+
+  it('two lines → two paragraph nodes', () => {
+    const doc = toADF('a\nline2')
+    expect(doc.content).toHaveLength(2)
+    expect(doc.content![0].content![0]).toEqual({ type: 'text', text: 'a' })
+    expect(doc.content![1].content![0]).toEqual({ type: 'text', text: 'line2' })
+  })
+
+  it('empty line between two lines → two paragraphs (empty line skipped)', () => {
+    const doc = toADF('a\n\nb')
+    expect(doc.content).toHaveLength(2)
+  })
+
+  it('empty string → doc with empty content array', () => {
+    const doc = toADF('')
+    expect(doc.type).toBe('doc')
+    expect(doc.content).toHaveLength(0)
+  })
+
+  it('each paragraph has correct structure', () => {
+    const doc = toADF('test')
+    const para = doc.content![0]
+    expect(para.type).toBe('paragraph')
+    expect(Array.isArray(para.content)).toBe(true)
+  })
+})
+
+describe('adfToText', () => {
+  it('null input → returns ""', () => {
+    expect(adfToText(null)).toBe('')
+  })
+
+  it('undefined input → returns ""', () => {
+    expect(adfToText(undefined)).toBe('')
+  })
+
+  it('text node → returns its text value', () => {
+    expect(adfToText({ type: 'text', text: 'hello' })).toBe('hello')
+  })
+
+  it('hardBreak node → returns "\\n"', () => {
+    expect(adfToText({ type: 'hardBreak' })).toBe('\n')
+  })
+
+  it('paragraph containing text node → returns text + trailing newline', () => {
+    const para: AdfNode = { type: 'paragraph', content: [{ type: 'text', text: 'hi' }] }
+    expect(adfToText(para)).toBe('hi\n')
+  })
+
+  it('two paragraphs in a doc → text joined with newlines, trimmed', () => {
+    const doc: AdfNode = {
+      type: 'doc',
+      content: [
+        { type: 'paragraph', content: [{ type: 'text', text: 'line1' }] },
+        { type: 'paragraph', content: [{ type: 'text', text: 'line2' }] },
+      ],
+    }
+    expect(adfToText(doc)).toBe('line1\n\nline2')
+  })
+
+  it('hardBreak inside paragraph → \\n in output', () => {
+    const para: AdfNode = {
+      type: 'paragraph',
+      content: [
+        { type: 'text', text: 'a' },
+        { type: 'hardBreak' },
+        { type: 'text', text: 'b' },
+      ],
+    }
+    expect(adfToText(para)).toBe('a\nb\n')
+  })
+
+  it('bulletList with listItem nodes → recurses and returns item text', () => {
+    const list: AdfNode = {
+      type: 'bulletList',
+      content: [
+        { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'item1' }] }] },
+        { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'item2' }] }] },
+      ],
+    }
+    const result = adfToText(list)
+    expect(result).toContain('item1')
+    expect(result).toContain('item2')
+  })
+
+  it('orderedList with listItem nodes → recurses and returns item text', () => {
+    const list: AdfNode = {
+      type: 'orderedList',
+      content: [
+        { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'first' }] }] },
+      ],
+    }
+    expect(adfToText(list)).toContain('first')
+  })
+
+  it('codeBlock node → returns text content with trailing newline', () => {
+    const code: AdfNode = {
+      type: 'codeBlock',
+      content: [{ type: 'text', text: 'const x = 1' }],
+    }
+    const result = adfToText(code)
+    expect(result).toContain('const x = 1')
+    expect(result).toMatch(/\n$/)
+  })
+
+  it('mention with attrs.text → returns "@<attrs.text>"', () => {
+    const mention: AdfNode = { type: 'mention', attrs: { text: 'Alice' } }
+    expect(adfToText(mention)).toBe('@Alice')
+  })
+
+  it('mention without attrs.text → returns "@user"', () => {
+    expect(adfToText({ type: 'mention', attrs: {} })).toBe('@user')
+    expect(adfToText({ type: 'mention' })).toBe('@user')
+  })
+
+  it('unknown node type with content → recurses into content', () => {
+    const node: AdfNode = {
+      type: 'unknownBlock',
+      content: [{ type: 'text', text: 'inner' }],
+    }
+    expect(adfToText(node)).toBe('inner')
+  })
+
+  it('unknown node type without content → returns ""', () => {
+    expect(adfToText({ type: 'someUnknown' })).toBe('')
+  })
+
+  it('doc node → joins blocks and trims', () => {
+    const doc: AdfNode = {
+      type: 'doc',
+      content: [
+        { type: 'paragraph', content: [{ type: 'text', text: 'hello' }] },
+      ],
+    }
+    expect(adfToText(doc)).toBe('hello')
+  })
+
+  it('real-world ADF: paragraph + bulletList + heading → correct flat text', () => {
+    const doc: AdfNode = {
+      type: 'doc',
+      content: [
+        { type: 'heading', content: [{ type: 'text', text: 'Title' }] },
+        { type: 'paragraph', content: [{ type: 'text', text: 'Intro text.' }] },
+        {
+          type: 'bulletList',
+          content: [
+            { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'A' }] }] },
+            { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'B' }] }] },
+          ],
+        },
+      ],
+    }
+    const result = adfToText(doc)
+    expect(result).toContain('Title')
+    expect(result).toContain('Intro text.')
+    expect(result).toContain('A')
+    expect(result).toContain('B')
+  })
+})
