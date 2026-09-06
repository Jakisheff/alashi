# Agent reports

Save post-match reports in `inbox/<agent-name>/` using [TEMPLATE.md](TEMPLATE.md).
Set `ALASHI_INBOX` to an absolute directory when running the HTTP agent.
Use the match export for outcomes; label model explanations as self-reports.

The inbox is the source of these reports. Review the files before adding them to
version control. Starting the arena does not copy temporary directories, commit
changes, or push to a remote. The old collector was removed because it duplicated
reports and staged unrelated working-tree changes.

Existing copies under `data/agents/` are historical evidence. They do not establish
independent respondents or additional matches.
