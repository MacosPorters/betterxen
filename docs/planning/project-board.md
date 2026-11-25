# Project board setup

Status: **not configured, and not an engineering blocker.** The token used so far lacks the GitHub `project` scope, and GitHub's API cannot create views. Tracked by [#346](https://github.com/MacosPorters/betterxen/issues/346).

One simple board for active work only:

1. `gh auth refresh -s project`, then create a project named "BetterXen Development".
2. Status (single select): Backlog, Ready, In Progress, In Review, Blocked, Done.
3. Fields: Priority (p1, p2, p3), Area (text), and the built-in Milestone, Assignees and Status. No other custom fields.
4. Add the open issues and pull requests (`gh project item-add`). Closed native issues stay out; they are indexed in [the native issue archive](../research/native-issue-archive.md).
5. Optional view in the web UI: a table filtered to the current milestone and sorted by Priority. No phase views and no native board.
