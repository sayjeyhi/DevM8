export interface AdfNode {
  type: string
  text?: string
  content?: AdfNode[]
  attrs?: Record<string, unknown>
  version?: number
}

export function toADF(text: string): AdfNode {
  const paragraphs = text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => ({
      type: 'paragraph',
      content: [{ type: 'text', text: line }],
    }))

  return { version: 1, type: 'doc', content: paragraphs }
}

export function adfToText(node: AdfNode | null | undefined): string {
  if (node == null) return ''

  switch (node.type) {
    case 'text':
      return node.text ?? ''

    case 'hardBreak':
      return '\n'

    case 'mention':
      return `@${node.attrs?.text || 'user'}`

    case 'paragraph':
    case 'heading':
    case 'blockquote':
    case 'listItem':
      return (node.content ?? []).map(adfToText).join('') + '\n'

    case 'bulletList':
    case 'orderedList':
      return (node.content ?? []).map(adfToText).join('')

    case 'codeBlock':
      return (node.content ?? []).map(adfToText).join('') + '\n'

    case 'doc':
      return (node.content ?? []).map(adfToText).join('\n').trim()

    default:
      return (node.content ?? []).map(adfToText).join('')
  }
}
