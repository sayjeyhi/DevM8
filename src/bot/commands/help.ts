import type { Context } from "grammy"

export const HELP_TEXT = `DevM8 Commands:

/create <title> [-- <description>]
  Create a Jira issue. Claude enriches the description if provided.

/move <issue-key> <status>
  Transition an issue to a new status (e.g. "In Progress").

/comment <issue-key> <text>
  Add a comment to an existing issue.

/solve <issue-key>
  Analyze an issue with Claude and post a solution as a comment.

/my_tickets
  List your last 10 assigned Jira tickets.

/logs [n]
  Show last n daemon log lines (default 50, max 200).

/help
  Show this reference.`

export async function handleHelp(ctx: Context): Promise<void> {
  await ctx.reply(HELP_TEXT)
}
