import { PATHS } from "../shared/paths"

interface SlackState {
  lastTs: Record<string, string>
}

export async function loadSlackState(): Promise<SlackState> {
  try {
    const text = await Bun.file(PATHS.slackStateFile).text()
    return JSON.parse(text) as SlackState
  } catch {
    return { lastTs: {} }
  }
}

export async function saveSlackState(state: SlackState): Promise<void> {
  await Bun.write(PATHS.slackStateFile, JSON.stringify(state))
}
