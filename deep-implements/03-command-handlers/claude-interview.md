# Interview Transcript: 03-command-handlers

## Round 1

### Q1: For /move — status matching strategy
**Answer:** Fuzzy/prefix match — case-insensitive substring contains.
e.g. `prog` matches `In Progress` via `"in progress".includes("prog")`.

### Q2: For /create with no description
**Answer:** Claude should expand the title into a short 3-5 line paragraph and bullet points (not just create with blank description). This is the fallback enrichment path, not a multi-turn flow.

### Q3: Are TelegramClient, JiraClient, ClaudeClient interfaces already defined?
**Answer:** Yes, already defined in `02-integration-clients`. Treat as stable contracts.

### Q4: Authorization model
**Answer:** Allowlist of specific Telegram user IDs. Unauthorized requests: log and silently ignore (no reply to the user).

---

## Round 2

### Q5: For /create title-expand — should it ask for component/priority?
**Answer:** No. Create with defaults only. User edits in Jira afterwards if needed.

### Q6: For /solve — handling Claude responses >4096 chars
**Answer:** Split at paragraph/word boundaries and send as multiple messages.

### Q7: Unauthorized user reply behavior
**Answer:** Log the attempt (user ID, command) and silently ignore — no reply sent.

### Q8: Claude retry strategy on timeout/error
**Answer:** Fail immediately. No auto-retry. Report error to user so they can re-run.

---

## Round 3

### Q9: Fuzzy match algorithm for /move
**Answer:** Case-insensitive substring contains. `"in progress".toLowerCase().includes(input.toLowerCase())`. Check each available transition name.

### Q10: Allowlist storage
**Answer:** Config file (JSON or .env). Simple structure, read at startup.

### Q11: Typing indicator during slow ops
**Answer:** Yes. Send `sendChatAction("typing")` while waiting for Claude API calls and Jira calls that may take a moment.

---

## Key Decisions Summary

| Decision | Choice |
|---|---|
| Bot framework | grammY (TypeScript, actively maintained) |
| /move matching | Case-insensitive substring contains |
| /create no-description | Claude expands title to 3-5 line paragraph + bullets |
| /create title-expand scope | Defaults only — no component/priority |
| /solve long response | Split at paragraph/word boundary, multiple messages |
| Unauthorized users | Log + silently ignore |
| Claude error handling | Fail immediately, report to user |
| Allowlist storage | Config file (JSON/.env), read at startup |
| Typing indicators | Yes — sendChatAction during Claude + Jira calls |
