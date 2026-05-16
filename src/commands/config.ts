import { defineCommand } from "citty"
import { loadConfig, writeConfig } from "../config/loader"
import { runWizard } from "../config/wizard"
import { PATHS } from "../shared/paths"

export async function configCommand(): Promise<void> {
  let existing
  try {
    existing = await loadConfig()
  } catch {
    existing = undefined
  }

  const result = await runWizard(existing)
  await writeConfig(result)
  process.stdout.write(`Config written to ${PATHS.configFile}\n`)
}

export default defineCommand({
  meta: { name: "config", description: "Configure jira-assistant" },
  async run() {
    await configCommand()
  },
})
