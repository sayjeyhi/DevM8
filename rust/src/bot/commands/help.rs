use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

use crate::bot::AppState;

pub const HELP_TEXT: &str = "\
<b>DevM8 Commands</b>

/create &lt;title&gt; [-- &lt;description&gt;]
  Create a Jira issue. Claude enriches the description if provided.

/move &lt;issue-key&gt; &lt;status&gt;
  Transition an issue to a new status (e.g. \"In Progress\").

/comment &lt;issue-key&gt; &lt;text&gt;
  Add a comment to an existing issue.

/solve &lt;issue-key&gt;
  Analyze an issue with Claude and post a solution as a comment.

/my_tickets
  List your last 10 assigned Jira tickets.

/ask [question]
  Ask Claude a question about a repository.

/logs [n]
  Show last n daemon log lines (default 50, max 200).

/help
  Show this reference.";

pub async fn handle_help(
    bot: Bot,
    msg: Message,
    _state: Arc<AppState>,
) -> Result<()> {
    bot.send_message(msg.chat.id, HELP_TEXT)
        .parse_mode(ParseMode::Html)
        .await?;
    Ok(())
}
