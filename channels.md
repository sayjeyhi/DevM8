# Channel Configuration

DevM8 supports two channels: **Telegram** and **Slack**. Telegram is required. Slack is optional.

Config file location: `~/.config/devm8/config.toml`

Run `devm8 config` to use the interactive wizard instead of editing the file directly.

---

## Telegram

Telegram is the primary channel. The daemon will not start without a valid Telegram config.

### 1. Create a bot

1. Open [@BotFather](https://t.me/BotFather) in Telegram.
2. Send `/newbot` and follow the prompts.
3. Copy the bot token — it looks like `123456789:ABCdefGHIjklMNOpqrSTUvwxYZ`.

### 2. Find your user ID

Send any message to [@userinfobot](https://t.me/userinfobot). It replies with your numeric user ID.

### 3. Config

```toml
[telegram]
bot_token    = "123456789:ABCdefGHIjklMNOpqrSTUvwxYZ"

# Telegram user IDs allowed to use the bot.
# Empty list = anyone who messages the bot can use it.
allowed_user_ids = [123456789]

# Only this user can run admin commands (/admin, /logs, /clone, /permissions).
# Remove to make the first user who messages the bot the implicit admin.
admin_user_id = 123456789
```

### Per-project access control

Restrict individual projects to specific users. Users listed here but absent from
`allowed_user_ids` may only access the projects explicitly granted to them.

```toml
[telegram.project_access]
MYAPP = [123456789, 987654321]
BZ    = [123456789]
```

Rules:
- `admin_user_id` always has full access, regardless of `project_access`.
- If a project key is absent from `project_access`, all `allowed_user_ids` can access it.
- A user present in `project_access` but absent from `allowed_user_ids` is restricted to only their granted projects — they cannot run other commands.

### Available commands

| Command | Description |
|---|---|
| `/help` | List available commands |
| `/ask [question]` | Ask Claude about a repo or run a CLI command |
| `/jira` | Interactive Jira panel |
| `/solve` | Analyze a ticket and implement a fix |
| `/admin` | Admin panel (clone repos, add projects, logs) |
| `/status` | Show bot status and config summary |

### Managing permissions at runtime

Use `/permissions` (admin only) to add/remove users and toggle project access without editing the config file. Changes are written to `config.toml` immediately.

---

## Slack

Slack is optional. There are two independent Slack modes that can run together:

| Mode | Token needed | What it does |
|---|---|---|
| **Legacy poller** | `user_token` (xoxp-) | Polls DMs and forwards them to Telegram |
| **Interactive bot** | `bot_token` (xoxb-) + `app_token` (xapp-) | Full bot: slash commands, buttons, session state |

Enable only the interactive bot, only the poller, or both.

The interactive bot listens for messages in three cases:
- Direct messages (always).
- @-mentions of the bot, in any channel it's a member of (always).
- Plain messages (no mention needed) in channels listed in `allowed_channel_ids`.

---

### Slack App Setup

#### Step 1 — Create the app

1. Go to [api.slack.com/apps](https://api.slack.com/apps) → **Create New App** → **From scratch**.
2. Give it a name (e.g. `DevM8`) and pick your workspace.

#### Step 2 — Bot token scopes (for interactive bot)

In **OAuth & Permissions** → **Bot Token Scopes**, add:

| Scope | Required for |
|---|---|
| `chat:write` | Sending messages |
| `chat:write.public` | Sending to channels the bot hasn't joined |
| `commands` | Slash commands |
| `im:history` | Reading direct messages |
| `im:read` | Listing DM channels |
| `im:write` | Opening DM channels |
| `channels:history` | Reading channel messages (if bot is invited to channels) |
| `users:read` | Resolving user display names |

Click **Install to Workspace** and copy the **Bot User OAuth Token** (`xoxb-…`).

#### Step 3 — App-level token (for Socket Mode)

In **Basic Information** → **App-Level Tokens** → **Generate Token and Scopes**:
- Add scope: `connections:write`
- Copy the token — it starts with `xapp-`.

#### Step 4 — Enable Socket Mode

In **Socket Mode** → toggle **Enable Socket Mode** on.

#### Step 5 — Register slash commands

In **Slash Commands** → **Create New Command** for each command you want:

| Command | Description | Usage hint |
|---|---|---|
| `/ask` | Ask Claude about a repo | `[question]` |
| `/jira` | Open the Jira panel | |
| `/help` | Show available commands | |
| `/admin` | Admin panel (admin only) | |

Set **Request URL** to any placeholder (e.g. `https://placeholder.example.com`) — Socket Mode ignores it.

#### Step 6 — Enable Event Subscriptions (for DM and channel handling)

In **Event Subscriptions** → toggle **Enable Events** on.  
In **Subscribe to bot events** add:

| Event | Purpose |
|---|---|
| `message.im` | Receive direct messages to the bot |
| `app_mention` | Receive @-mentions of the bot in any channel it's in |
| `message.channels` | Receive messages in public channels listed in `allowed_channel_ids` |
| `message.groups` | Receive messages in private channels listed in `allowed_channel_ids` |

`message.channels`/`message.groups` are only needed if you want the bot to listen to
every message in specific channels (see `allowed_channel_ids` below). `app_mention`
alone is enough if you only want the bot to respond when @-mentioned. Also invite the
bot to any channel it should listen to (`/invite @YourBot`) and, for private channels,
add the `groups:history` bot token scope alongside `channels:history`.

#### Step 7 — User token (for legacy poller only)

If you want the DM-forwarding poller:

In **OAuth & Permissions** → **User Token Scopes**, add:
- `im:history`
- `im:read`

The **User OAuth Token** starts with `xoxp-`.

---

### Slack Config

#### Interactive bot (full mode)

```toml
[slack]
# xoxp- user token — required even in bot mode (legacy poller can run alongside).
# Set to a dummy value if you only want the bot and not the poller.
user_token       = "xoxp-..."
poll_interval_ms = 30000        # poller interval in ms; ignored when bot mode is active

# Bot token (xoxb-) and app-level token (xapp-) enable Socket Mode.
bot_token  = "xoxb-..."
app_token  = "xapp-..."

# Slack user IDs allowed to interact with the bot.
# Empty list = everyone in the workspace is allowed.
# Find your Slack user ID: click your name in Slack → profile → "Copy member ID".
allowed_user_ids = ["U1234567", "U9876543"]

# Channel IDs the bot proactively listens to (every message, not just mentions).
# Empty list = none — outside these channels the bot only responds in DMs and
# whenever it's @-mentioned, regardless of this list.
# Find a channel ID: open the channel in Slack → "View channel details" → bottom of the panel.
allowed_channel_ids = ["C1234567"]

# Only this user can run /admin and manage permissions.
admin_user_id = "U1234567"
```

#### Legacy poller only

```toml
[slack]
user_token       = "xoxp-..."
poll_interval_ms = 30000
```

The poller periodically checks for new DMs on the user's account and forwards them to
all Telegram `allowed_user_ids`. No bot token or app token needed.

#### Per-project access control

Same concept as Telegram — restrict projects to specific Slack user IDs:

```toml
[slack.project_access]
MYAPP = ["U1234567", "U9876543"]
BZ    = ["U1234567"]
```

Rules mirror Telegram: `admin_user_id` always has full access; absent keys mean all
`allowed_user_ids` can access that project.

---

### Available Slack commands

Once the bot is running, these slash commands are available in any Slack DM or channel
where the bot is present:

| Command | Description |
|---|---|
| `/help` | List available commands |
| `/ask [question]` | Ask Claude about a repo or run a CLI command |
| `/jira` | Open the Jira panel |
| `/admin` | Admin panel — admin only |

Direct messages to the bot (without a slash command) are routed to the active session
(e.g. a follow-up to an ongoing `/ask` conversation).

---

## Full config reference

```toml
# ── Telegram (required) ──────────────────────────────────────────────────────

[telegram]
bot_token        = "123456789:ABCdefGHIjklMNOpqrSTUvwxYZ"
allowed_user_ids = [123456789]
admin_user_id    = 123456789

[telegram.project_access]
MYAPP = [123456789, 987654321]
BZ    = [123456789]

# ── Jira (optional — users can also configure their own via /jira) ────────────

[jira]
base_url     = "https://yourcompany.atlassian.net"
email        = "you@example.com"
api_token    = "YOUR_JIRA_API_TOKEN"
project_keys = ["MYAPP", "BZ"]

# ── Claude (required) ────────────────────────────────────────────────────────

[claude]
binary_path = "/usr/local/bin/claude"
# api_key   = "sk-ant-..."  # optional if authenticated via `claude login`
# timeout_ms = 300000       # default: no timeout
# sandbox    = true         # default: true on Linux, false on macOS

# ── Git repos linked to Jira projects ────────────────────────────────────────

[repos]
MYAPP = ["/home/you/code/myapp"]
BZ    = ["/home/you/code/blaze", "/home/you/code/blaze-infra"]

# ── Slack (optional) ─────────────────────────────────────────────────────────

[slack]
user_token       = "xoxp-..."      # required (poller)
poll_interval_ms = 30000
bot_token          = "xoxb-..."      # optional (enables interactive bot)
app_token          = "xapp-..."      # optional (enables Socket Mode)
allowed_user_ids   = ["U1234567"]
allowed_channel_ids = ["C1234567"]    # channels to listen to beyond DMs/mentions
admin_user_id      = "U1234567"

[slack.project_access]
MYAPP = ["U1234567", "U9876543"]
BZ    = ["U1234567"]

# ── App settings ─────────────────────────────────────────────────────────────

[app]
log_level = "info"   # info | debug | error
```

---

## Applying changes

After editing `config.toml` directly, reload the daemon:

```bash
devm8 stop && devm8 start
```

Or send `SIGHUP` to reload config without a full restart (Telegram polling resumes
immediately; Slack reconnects within a few seconds):

```bash
kill -HUP $(cat ~/.config/devm8/daemon.pid)
```
