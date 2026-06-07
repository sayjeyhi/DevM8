# Microsoft Teams Integration

DevM8 supports Microsoft Teams alongside Telegram and Slack. All features — ask sessions, Jira, git workflows, admin panel, and interactive buttons — are fully supported through Teams **Adaptive Cards**.

---

## How It Works

Teams uses the **Bot Framework** protocol: Teams sends an HTTP `POST /api/messages` to your server for every user message or button click. DevM8 runs an embedded webhook server on a configurable port (default **3978**) that receives and dispatches these activities.

**Buttons** are rendered as Adaptive Card `Action.Submit` elements — they look and behave like Telegram's inline keyboard buttons.

---

## Prerequisites

1. **Azure account** — free tier is fine.
2. **Azure Bot Registration** — creates the App ID + password the bot uses to authenticate outbound API calls.
3. A **public HTTPS endpoint** pointing to the machine running DevM8 (e.g. reverse proxy with nginx/Caddy, or a tunnel with [ngrok](https://ngrok.com) or [Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/) during development).

---

## Step 1 — Create a Bot in Azure

1. Go to the [Azure Portal](https://portal.azure.com).
2. Search for **Azure Bot** and click **Create**.
3. Fill in:
   - **Bot handle** — any name (e.g. `devm8-bot`)
   - **Subscription / Resource group** — choose or create
   - **Pricing tier** — **F0** (free)
   - **Type of App** — **Single Tenant** (recommended for internal company bots)
4. Click **Review + Create** → **Create**.
5. Once deployed, go to the resource → **Configuration** tab:
   - Copy the **Microsoft App ID** — this is your `app_id`.
   - Click **Manage Password** → **New client secret** → copy the value — this is your `app_password`.
6. Go to **Microsoft Entra ID** → **Overview** and copy the **Tenant ID** — you will need this too.

---

## Step 2 — Add the Teams Channel

1. In the Azure Bot resource, go to **Channels** → **Microsoft Teams** → click the Teams icon.
2. Accept the terms and click **Save**.
3. The bot is now enabled for Teams.

---

## Step 3 — Configure DevM8

Add a `[teams]` section to your DevM8 config file (`~/.config/devm8/config.toml`):

```toml
[teams]
# From Azure Portal → Bot resource → Configuration tab
app_id       = "YOUR_APP_ID"
app_password = "YOUR_APP_PASSWORD"

# Required for Single Tenant bots (created with "Single Tenant" app type).
# Azure Portal → Microsoft Entra ID → Overview → Tenant ID
tenant_id    = "YOUR_TENANT_ID"

# Port the webhook server listens on (default 3978)
# port = 3978

# Restrict access (Teams AAD user object IDs).
# Leave empty to allow everyone in your org.
# allowed_user_ids = ["aad-object-id-1", "aad-object-id-2"]

# Admin user (can run /admin)
# admin_user_id = "aad-object-id-of-admin"

# Per-project access (project key → list of AAD object IDs)
# [teams.project_access]
# MYAPP = ["aad-object-id-1"]
```

> **How to find a user's AAD Object ID:**
> Azure Portal → Microsoft Entra ID → Users → click the user → copy **Object ID**.

---

## Step 4 — Expose the Webhook Publicly

Teams requires HTTPS. Options:

### Production — Reverse Proxy

Point your domain to port 3978 and add a TLS certificate:

**nginx example:**
```nginx
server {
    listen 443 ssl;
    server_name bot.yourcompany.com;

    ssl_certificate     /etc/ssl/certs/your-cert.pem;
    ssl_certificate_key /etc/ssl/private/your-key.pem;

    location /api/messages {
        proxy_pass http://127.0.0.1:3978;
    }
}
```

### Development — ngrok Tunnel

```bash
ngrok http 3978
# Copy the https://xxxx.ngrok.io URL
```

### Development — Cloudflare Tunnel

```bash
cloudflared tunnel --url http://localhost:3978
```

---

## Step 5 — Set the Messaging Endpoint

1. In the Azure Bot resource → **Configuration**.
2. Set **Messaging endpoint** to:
   ```
   https://YOUR_PUBLIC_DOMAIN/api/messages
   ```
3. Click **Apply**.

---

## Step 6 — Start DevM8

```bash
devm8 start
```

The Teams webhook server starts automatically alongside the Telegram bot. Check the log:

```bash
devm8 logs
# Look for: "teams webhook listening" {"addr":"0.0.0.0:3978"}
```

---

## Step 7 — Install the Bot in Teams

1. In the Azure Bot resource → **Channels** → **Open in Teams** — this opens a 1:1 chat with the bot directly.
2. Alternatively, package and install the bot as a Teams App:
   - Download the **App manifest** from Azure Bot → **Channels** → Teams → **App manifest**.
   - In Teams → **Apps** → **Manage your apps** → **Upload an app** → select the manifest zip.
   - The bot will appear under **Built for your org**.

---

## Commands

All commands are sent as chat messages or by pressing buttons:

| Command | Description |
|---------|-------------|
| `/ask [question]` | Start an AI ask session (with git repo selection) |
| `/start [question]` | Alias for `/ask` |
| `/jira` | Open Jira panel (browse tickets, create issues) |
| `/help` | Show available commands |
| `/admin` | Admin panel — projects, permissions, logs (admin only) |

Free-form text (without a `/` prefix) is treated as a message to the active ask session.

---

## Buttons and Adaptive Cards

All interactive buttons work the same as in Telegram. Example flow:

1. User types `/ask`
2. Bot shows a card with repo selection buttons
3. User clicks a repo — bot shows the session keyboard
4. User clicks **Pull latest**, **Commit**, **Push**, **Open PR**, etc.

Buttons appear as clickable cards directly inside the Teams chat.

---

## Access Control

### Allow-list

Only users listed in `allowed_user_ids` can interact with the bot. If the list is empty, everyone in the tenant can use it.

### Per-project Access

```toml
[teams.project_access]
MYAPP  = ["aad-id-of-alice", "aad-id-of-bob"]
BACKEND = ["aad-id-of-carol"]
```

A user not listed in any project's entry can still use the bot for non-project commands (/help, /jira with permitted projects).

### Admin

Only the `admin_user_id` can run `/admin`. If not set, the first user is treated as admin.

---

## Security Note

The webhook currently accepts any POST to `/api/messages` without verifying the Bot Framework JWT signature. This is safe if your endpoint is not publicly guessable, but for production you should add signature validation.

To enable proper JWT validation, Microsoft publishes its signing keys at:
```
https://login.botframework.com/v1/.well-known/openidconfiguration
```

Validation can be added to the `handle_activity` function in `src/teams_bot/webhook.rs` by checking the `Authorization: Bearer <token>` header against the JWKS endpoint.

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Bot doesn't respond | Check `devm8 logs` for `teams webhook listening`. Verify the messaging endpoint URL in Azure. |
| `Teams API 401` in logs | App password is wrong or expired — rotate it in Azure Portal. |
| `Teams API 403` in logs | The bot token lacks permissions — check the bot's channel configuration. |
| Buttons show but clicks do nothing | Verify the public endpoint is reachable from the internet (try `curl https://your-domain/api/messages`). |
| `invalid Teams chat_id` in logs | A message arrived with a missing `serviceUrl` or `conversation.id` — this is a Teams SDK edge case; safe to ignore. |
| User sees no bot in Teams | Ensure the Teams channel is enabled in Azure and the app manifest is installed. |

---

## Architecture Overview

```
Teams Client
    │  HTTPS POST /api/messages (Bot Framework Activity JSON)
    ▼
DevM8 webhook server (axum, port 3978)
    │
    ├── Activity type: "message" + value.devm8_action present → button click
    │       └─► dispatch_action() → same handlers as Telegram/Slack
    │
    └── Activity type: "message" + text present → user typed
            ├── starts with "/" → slash command routing
            └── otherwise → pending-state routing → ask session
```

The Teams bot shares the same `AppState` as the Telegram and Slack bots, so all session state (git worktrees, Jira context, conversation history) is unified across channels.
