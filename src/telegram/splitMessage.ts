export function splitMessage(text: string, limit = 4096): string[] {
  if (text.length <= limit) return [text]

  const chunks: string[] = []
  let current = ''

  for (const para of text.split('\n\n')) {
    for (const wordChunk of splitParagraph(para, limit)) {
      if (current === '') {
        current = wordChunk
      } else if (current.length + 2 + wordChunk.length <= limit) {
        current += '\n\n' + wordChunk
      } else {
        chunks.push(current)
        current = wordChunk
      }
    }
  }

  if (current) chunks.push(current)
  return chunks
}

function splitParagraph(text: string, limit: number): string[] {
  if (text.length <= limit) return [text]

  const chunks: string[] = []
  let current = ''

  for (const word of text.split(' ')) {
    if (word.length > limit) {
      if (current) { chunks.push(current); current = '' }
      let remaining = word
      while (remaining.length > limit) {
        chunks.push(remaining.slice(0, limit))
        remaining = remaining.slice(limit)
      }
      if (remaining) current = remaining
    } else if (current === '') {
      current = word
    } else if (current.length + 1 + word.length <= limit) {
      current += ' ' + word
    } else {
      chunks.push(current)
      current = word
    }
  }

  if (current) chunks.push(current)
  return chunks
}
