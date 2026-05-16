import type { Context } from "grammy"
import type { Clients } from "./index"

export async function handleMyTickets(ctx: Context, { jira }: Clients): Promise<void> {
  const issues = await jira.getMyIssues(10)

  if (issues.length === 0) {
    await ctx.reply("No tickets assigned to you.")
    return
  }

  const lines = issues.map(
    (issue, i) => `${i + 1}. [${issue.key}] ${issue.summary}\n   Status: ${issue.status}\n   ${issue.url}`
  )

  await ctx.reply(`Your last ${issues.length} assigned tickets:\n\n${lines.join("\n\n")}`)
}
