# Interview Transcript

## Q1: What should `jira-assistant start` do if the daemon is already running?

**Answer:** Restart it — stop the running instance, then start fresh.

---

## Q2: What should the `status` command output?

**Answer:** Human-readable summary — running state, PID, uptime, config path, key config values (URL, project key).

---

## Q3: Binary name — should both `jira-assistant` and `ja` work?

**Answer:** Both — `ja` is a short alias. Install both names (symlink or second compiled binary).

---

## Q4: Should the `daemon` subcommand be hidden from help output or shown explicitly?

**Answer:** Explicit public command — listed in `--help` as an advanced option for foreground/dev mode.

---

## Q5: If config is missing when user runs `start`, what should happen?

**Answer:** Auto-trigger the wizard — detect missing config and run first-run setup before starting.

---

## Q6: When `jira-assistant config` runs on an existing config, how should it behave?

**Answer:** Full re-run — go through all prompts; pre-fill with current values from existing config.

---

## Q7: How will the compiled binary be distributed to users?

**Answer:** Direct download / GitHub Releases.

---

## Q8: What should happen if the daemon crashes repeatedly?

**Answer:** Max retries then give up — custom logic: daemon tracks restart count, exits 0 after N failures to stop launchd loop.

---

## Q9: Should `stop` also remove the plist file, or just unload the service?

**Answer:** Unload only — plist stays; `start` re-uses it next time.

---

## Q10: Error handling for failed launchctl commands?

**Answer:** Show raw launchctl stderr + friendly hint (suggested fix).

---

## Q11: Should `daemon` behave differently in terminal vs launchd?

**Answer:** Same code, different log output — TTY check: human-readable logs in terminal, JSON logs in launchd mode.

---

## Q12: Max restart count before giving up?

**Answer:** 10 restarts.

---

## Q13: Should log files be rotated automatically?

**Answer:** Size-based rotation (10MB max) — roll log file when it hits size limit; keep last N files.
