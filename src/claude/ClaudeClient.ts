import { ClaudeTimeoutError, ClaudeExitError } from '../shared/errors'
import type { ClaudeConfig, AskOptions } from './types'

interface Logger {
  info(data: Record<string, unknown>): void
}

interface BunSubprocess {
  stdin: { write(data: string): void; end(): void }
  stdout: ReadableStream<Uint8Array>
  stderr: ReadableStream<Uint8Array>
  exitCode: number | null
  exited: Promise<number>
  kill(signal: NodeJS.Signals): void
}

declare const Bun: {
  spawn(
    args: string[],
    opts: { stdin: 'pipe'; stdout: 'pipe'; stderr: 'pipe'; env: Record<string, string | undefined> },
  ): BunSubprocess
  sleep(ms: number): Promise<void>
}

// Extract generated text from a single stream-json event line.
// Handles both Anthropic API streaming format (content_block_delta) and
// Claude Code CLI format (assistant message events).
function extractStreamText(event: Record<string, unknown>): string | null {
  if (event.type === 'content_block_delta') {
    const delta = event.delta as Record<string, unknown> | undefined
    if (delta?.type === 'text_delta' && typeof delta.text === 'string') return delta.text
  }
  if (event.type === 'assistant') {
    const msg = event.message as { content?: Array<{ type: string; text?: string }> } | undefined
    if (Array.isArray(msg?.content)) {
      const text = msg.content.filter(c => c.type === 'text' && c.text).map(c => c.text).join('')
      return text || null
    }
  }
  return null
}

export class ClaudeClient {
  constructor(
    private readonly config: ClaudeConfig,
    private readonly logger: Logger,
  ) {}

  async ask(prompt: string, options?: AskOptions): Promise<string> {
    const effectiveTimeoutMs = options?.timeoutMs ?? this.config.timeoutMs ?? 120000
    const effectiveModel = options?.model ?? this.config.model

    const args = [
      this.config.binaryPath,
      '--print',
      '--dangerously-skip-permissions',
      '--output-format',
      'stream-json',
    ]
    if (effectiveModel) args.push('--model', effectiveModel)

    const clonedEnv: Record<string, string | undefined> = { ...process.env }
    delete clonedEnv.CLAUDECODE

    this.logger.info({ event: 'claude_spawn', binary: this.config.binaryPath, model: effectiveModel, home: clonedEnv.HOME })

    let timedOut = false
    const startMs = Date.now()
    const proc = Bun.spawn(args, {
      stdin: 'pipe',
      stdout: 'pipe',
      stderr: 'pipe',
      env: clonedEnv,
    })

    proc.stdin.write(prompt)
    proc.stdin.end()

    const timer = setTimeout(async () => {
      timedOut = true
      proc.kill('SIGTERM')
      await Bun.sleep(2000)
      if (proc.exitCode === null) proc.kill('SIGKILL')
    }, effectiveTimeoutMs)

    const textLines: string[] = []
    let resultEvent: { is_error: boolean; result: string } | undefined
    let stderr = ''

    try {
      await Promise.all([
        // Stream stdout line-by-line and fire progress callbacks
        (async () => {
          const reader = proc.stdout.getReader()
          const decoder = new TextDecoder()
          let buffer = ''
          let lastProgressAt = Date.now()

          while (true) {
            const { done, value } = await reader.read()
            if (done) break
            buffer += decoder.decode(value, { stream: true })
            const lines = buffer.split('\n')
            buffer = lines.pop() ?? ''

            for (const line of lines) {
              const trimmed = line.trim()
              if (!trimmed) continue
              try {
                const event = JSON.parse(trimmed) as Record<string, unknown>
                if (event.type === 'result') {
                  resultEvent = {
                    is_error: !!event.is_error || typeof event.result !== 'string',
                    result: typeof event.result === 'string'
                      ? event.result
                      : `unexpected result type: ${JSON.stringify(event.result)}`,
                  }
                } else {
                  const text = extractStreamText(event)
                  if (text) textLines.push(...text.split('\n'))
                }
              } catch { /* skip non-JSON lines */ }
            }

            if (options?.onProgress && Date.now() - lastProgressAt > 2000) {
              lastProgressAt = Date.now()
              await options.onProgress(textLines.slice(-10))
            }
          }
          // Final progress flush after stream ends
          if (options?.onProgress && textLines.length > 0) {
            await options.onProgress(textLines.slice(-10))
          }
        })(),
        (async () => { stderr = await new Response(proc.stderr).text() })(),
        proc.exited,
      ])
    } finally {
      clearTimeout(timer)
    }

    const durationMs = Date.now() - startMs
    const failed = !timedOut && proc.exitCode !== null && proc.exitCode !== 0

    this.logger.info({
      event: 'claude_done',
      exitCode: proc.exitCode,
      durationMs,
      ...(failed && { stderr: stderr.slice(0, 1000) || undefined }),
    })

    if (timedOut) throw new ClaudeTimeoutError(effectiveTimeoutMs)

    if (failed || resultEvent?.is_error) {
      const errorMessage =
        (resultEvent?.is_error && resultEvent.result) ||
        stderr.trim() ||
        `Claude exited with code ${proc.exitCode}`
      this.logger.info({ event: 'claude_error', exitCode: proc.exitCode, message: errorMessage })
      throw new ClaudeExitError(proc.exitCode ?? 1, errorMessage)
    }

    if (!resultEvent) {
      if (textLines.length > 0) return textLines.join('\n')
      throw new Error(`ClaudeClient: no result event in output (stderr: ${stderr.slice(0, 200)})`)
    }

    return resultEvent.result
  }
}
