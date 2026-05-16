# Project Requirements

## Overview

Build a CLI Bun-based application that:
- Produces OS-native executable binaries (macOS, Linux, Windows)
- Is installable via a single bash one-liner from GitHub
- Runs as a background daemon/service
- Integrates with Telegram Bot API
- Integrates with Jira API
- Integrates with local Claude (claude CLI / Claude Code)

## Core Automations

The app should enable the following workflows, triggered via Telegram messages:

- **Create Jira ticket** — user describes issue via Telegram, app creates ticket
- **Move Jira ticket** — change ticket status/column via Telegram command
- **Comment on Jira ticket** — add comment to ticket via Telegram
- **Get AI solutions** — ask Claude to analyze a Jira ticket and suggest solutions, returned via Telegram

## User Interaction Model

- User talks to a personal Telegram bot
- Messages are parsed and routed to the appropriate integration (Jira, Claude)
- Responses come back through Telegram

## Technical Constraints

- Built with Bun (TypeScript runtime)
- Single executable per OS (bun build --compile)
- Install via: `curl -fsSL https://raw.githubusercontent.com/.../install.sh | bash`
- Runs as background process (daemon) — survives terminal close
- Config stored locally (API tokens, Jira base URL, Claude path, etc.)
- Telegram bot token and Jira credentials configured on first run or via config file
