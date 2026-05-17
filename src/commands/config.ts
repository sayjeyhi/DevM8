import { defineCommand } from "citty"
import { loadConfig, writeConfig } from "../config/loader"
import { runWizard } from "../config/wizard"
import { PATHS } from "../shared/paths"
import { appendToLogFile } from "../logger/index"

export async function configCommand(): Promise<void> {
  let existing
  try {
    existing = await loadConfig()
  } catch {
    existing = undefined
  }

  const result = await runWizard(existing)
  await writeConfig(result)
  appendToLogFile(PATHS.logFile, "info", "config written", { file: PATHS.configFile })
  process.stdout.write(`Config written to ${PATHS.configFile}\n`)
}

export default defineCommand({
  meta: { name: "config", description: "Configure DevM8" },
  async run() {
    await configCommand()
  },
})
