import { parse, stringify } from "smol-toml"
import { dirname } from "node:path"
import { writeFile, rename, mkdir, chmod } from "node:fs/promises"
import { AppConfigSchema, type AppConfig } from "./schema"
import { FriendlyError } from "../shared/errors"
import { PATHS } from "../shared/paths"

export async function loadConfig(configPath?: string): Promise<AppConfig> {
  const resolvedPath = configPath ?? PATHS.configFile

  let rawText: string
  try {
    rawText = await Bun.file(resolvedPath).text()
  } catch (e: unknown) {
    const code = (e as NodeJS.ErrnoException).code
    if (code === "ENOENT") {
      throw new FriendlyError(
        `Config file not found at ${resolvedPath}. Run \`devm8 config\` to create it.`,
        "Run `devm8 config` to set up your configuration."
      )
    }
    if (code === "EACCES") {
      throw new FriendlyError(`Permission denied reading config at ${resolvedPath}.`)
    }
    throw e
  }

  let parsed: unknown
  try {
    parsed = parse(rawText)
  } catch (e: unknown) {
    throw new FriendlyError(`Failed to parse config file: ${(e as Error).message}`)
  }

  const result = AppConfigSchema.safeParse(parsed)
  if (!result.success) {
    const lines = result.error.issues.map((issue) => {
      const field = issue.path.join(".") || "unknown"
      return `${field}: ${issue.message}`
    })
    throw new FriendlyError(`Invalid config:\n${lines.join("\n")}`)
  }

  return result.data
}

export async function configExists(configPath?: string): Promise<boolean> {
  return Bun.file(configPath ?? PATHS.configFile).exists()
}

export async function writeConfig(config: AppConfig, configPath?: string): Promise<void> {
  const resolvedPath = configPath ?? PATHS.configFile
  await mkdir(dirname(resolvedPath), { recursive: true })
  const toml = stringify(config as Record<string, unknown>)
  const tmpPath = resolvedPath + ".tmp"
  await writeFile(tmpPath, toml, "utf8")
  // chmod before rename so the file is never world-readable at the final path
  await chmod(tmpPath, 0o600)
  await rename(tmpPath, resolvedPath)
}
