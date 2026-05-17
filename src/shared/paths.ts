import { homedir } from "os"
import { join } from "path"

const home = homedir()
const configDir = join(home, ".config/devm8")
const logsDir = join(configDir, "logs")
const launchAgentsDir = join(home, "Library/LaunchAgents")

export const PATHS = {
  configDir,
  configFile:      join(configDir, "config.toml"),
  restartsFile:    join(configDir, "restarts.json"),
  logsDir,
  logFile:         join(logsDir, "app.log"),
  pidFile:         join(configDir, "daemon.pid"),
  plistFile:       join(launchAgentsDir, "net.devm8.plist"),
  launchAgentsDir,
}
