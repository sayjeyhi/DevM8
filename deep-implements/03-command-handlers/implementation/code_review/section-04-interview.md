# Interview Transcript: section-04-create-handler

## Auto-fixes applied

1. **Prompt injection defense**: replace {description} before {title} to prevent cross-substitution attack where title = "{description}".
2. **Move initial replyWithChatAction inside try**: prevents interval leak if that call throws.
3. **Trim title and rawDescription after -- split**: removes leading/trailing whitespace from both parts.
4. **Add JiraNotFoundError test**.
5. **Add TODO comment on local Clients interface** (replace with ./index import after section-07).
6. **Add EXPAND_PROMPT_TEMPLATE content assertion** (3-5 sentences and acceptance criteria).

## Let go

- console.log vs structured logger (daemon logger has different interface; fine for now)
- mock API compatibility concern (bun:test already supports .mockResolvedValue)
