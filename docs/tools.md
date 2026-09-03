# cox tools

Every tool implements one contract (`Tool::spec` / `subject` / `call`):
untruncated output goes to the archive first, the model sees the capped
visible form plus an `expand` pointer. Permission rules match on
`subject()` (path, command line, URL, or namespaced MCP name).

Core tools are always in context; deferred tools join through `tool_search`
(D6d). `agent` is never available to itself.

| Tool | Risk | Deferred | Subject | Notes |
|---|---|---|---|---|
| `read` | ReadOnly | no | path | whole / `lines="a-b"` / `mode="outline"` |
| `grep` | ReadOnly | no | pattern | ripgrep libs, respects `.gitignore` |
| `glob` | ReadOnly | no | pattern | mtime order, fuzzy with `query` |
| `edit` | Write | no | path | exact `str_replace`, ambiguity errors |
| `apply_patch` | Write (Destructive past 5 deleted files) | no | patch summary | Codex V4A grammar |
| `write` | Write | no | path | new files; rewrites over 200 lines refused |
| `bash` | Exec (Destructive as classified) | no | command line | sandboxed, streamed, `background: true` archives |
| `todo` | ReadOnly | no | — | drives the TUI todo panel |
| `expand` | ReadOnly | no | archive id | reads back archived output, capped |
| `ask_user` | ReadOnly | yes | question | blocks the turn; `--answer` headless |
| `tool_search` | ReadOnly | no | query | reveals up to 5 deferred schemas |
| `web_fetch` | ReadOnly | yes | URL | readability fallback; domain rules apply |
| `agent` | max of its tools | yes | preset | `explore` / `shell` presets, own budget |
| `memory_save` | Write | yes | name | one fact file + index + FTS row |
| `memory_search` | ReadOnly | yes | query | FTS first, then files; top 5 capped |
| `mcp__<server>__<tool>` | from server annotations (default Write) | yes | namespaced name | fail-open servers |

Edits are diff-shaped (`edit`, `apply_patch`); `write` is for new files.
Every path from the model passes `path::confine`; every shell command runs
under the platform sandbox unless the session chose `danger-full-access`.
