# cox tools

Every tool implements one contract (`Tool::spec` / `subject` / `call`):
untruncated output goes to the archive first, the model sees the capped
visible form plus an `expand` pointer. Permission rules match on
`subject()` (path, command line, URL, or namespaced MCP name); `bash`
also hands over `segments()`, the simple commands of its line, so a
`Bash(prefix:*)` rule must cover each one (see how-it-works.md, Example 3).

Core tools are always in context; deferred tools join through `tool_search`
(D6d). `agent` is never available to itself.

| Tool | Risk | Deferred | Subject | Notes |
|---|---|---|---|---|
| `read` | ReadOnly | no | path | whole / `lines="a-b"` / `mode="outline"`; images returned as images, other binaries refused |
| `grep` | ReadOnly | no | pattern | ripgrep libs, respects `.gitignore` |
| `glob` | ReadOnly | no | pattern | mtime order, fuzzy with `query` |
| `edit` | Write | no | path | exact `str_replace`, ambiguity errors |
| `apply_patch` | Write (Destructive past 5 deleted files) | no | patch summary | Codex V4A grammar |
| `write` | Write | no | path | new files; rewrites over 200 lines refused |
| `bash` | Exec (Destructive as classified) | no | command line | sandboxed, streamed, `background: true` archives |
| `todo` | ReadOnly | no | — | drives the TUI todo panel |
| `expand` | ReadOnly | no | archive id | reads back archived output, capped |
| `ask_user` | ReadOnly | yes | question | blocks the turn; `--answer` headless; TUI: modal |
| `tool_search` | ReadOnly | no | query | reveals up to 5 deferred schemas |
| `web_fetch` | ReadOnly | yes | URL | readability fallback; domain rules apply |
| `agent` | max of its tools; Destructive with `isolation: "worktree"` | yes | preset | `explore` / `shell` presets, own budget; worktree isolation asks (denied in plan) |
| `memory_save` | Write | yes | name | one fact file + index + FTS row |
| `memory_search` | ReadOnly | yes | query | FTS first, then files; top 5 capped |
| `mcp__<server>__<tool>` | from server annotations (default Write) | yes | namespaced name | fail-open servers |

## MCP elicitation

An MCP server may ask the person for input during a tool call
(`elicitation/create`, or a 2026-07-28 `input_required` round). cox answers
only where a person can:

- TUI and `--plain`: the client declares `elicitation.form`. Each field of
  the requested form is one question in the question modal, labelled
  `mcp:<server> asks:` (in `--plain`, `question from mcp:<server>:`), with
  enum labels, `yes`/`no` or free text; a bad answer is asked again with
  the reason. A review step names the fields to send and offers `send`,
  `edit` (ask again; Enter keeps the last answer) or `decline`. Questions
  are events and land in the rollout, so neither the review nor an `edit`
  round repeats what was typed. Esc (in `--plain`, an empty line)
  cancels the whole request. While a question is open the call's
  `mcp.timeout_s` does not count.
- URL mode (TUI and `--plain`, `elicitation.url`): one question shows the
  message, the URL as it will be opened (normalized, so a non-ASCII host
  appears as punycode), its host, and a warning for a punycode host or a
  plain `http` URL; it offers `open` or `decline`. The browser opens only
  on `open`, and the server gets `accept`; the flow's outcome itself stays
  between the browser and the server. A URL that is not `http(s)`, names no
  host or would not survive the opener unchanged is declined without
  asking.
- `cox run -p` declares no elicitation capability and never asks; a
  server that elicits anyway gets `decline`. `--answer` does not apply.
- `cox acp` connects no MCP servers today, so there is nothing to answer.

Answers go to the server only: never into the transcript, the rollout or
the model's context. The server's text is shown sanitized, like any tool
output.

Edits are diff-shaped (`edit`, `apply_patch`); `write` is for new files.
Every path from the model passes `path::confine`; every shell command runs
under the platform sandbox unless the session chose `danger-full-access`.

`read` returns a PNG, JPEG, GIF or WebP file (sniffed by its magic bytes,
not its extension) as an image: a one-line `media type, size` text plus the
image itself, which `lines` and `mode` do not apply to. One image is capped
at 3,750,000 bytes (5,000,000 once base64-encoded, the smallest per-image
limit a supported provider documents); a larger one is refused as
`too_large`. These are the four formats Anthropic and OpenAI both accept.
