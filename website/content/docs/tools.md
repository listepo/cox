---
title: "Tools"
weight: 5
---

cox exposes built-in tools to the model. Core tools stay in context on every turn; deferred tools appear only after `tool_search` discovers them. Untruncated output is archived first—the model sees a capped view and can call `expand` to retrieve the full result by id.

## Core tools

| Tool | Role |
| --- | --- |
| `read` | Read whole files, line ranges, or structural outlines |
| `edit` | Apply exact string-replace edits |
| `write` | Create new files |
| `bash` | Run shell commands under the sandbox |
| `grep` | Search file contents (respects `.gitignore`) |
| `glob` | Find paths by pattern |
| `outline` | Tree-sitter structural summary (via `read` with `mode="outline"`) |

## Deferred tools

These tools are not in the initial schema. The model finds them through `tool_search` when needed:

| Tool | Role |
| --- | --- |
| `agent` | Spawn a subagent with its own budget and tool set |
| `web_fetch` | Fetch and extract readable content from a URL |
| `memory_save` / `memory_search` | Persist and recall session facts |
| `tool_search` | Reveal deferred tool schemas |
| `ask_user` | Block the turn until the user answers |
| `todo` | Track tasks in the TUI |
| `expand` | Retrieve archived tool output by id |

## Safety

Permission checks live in the core permission engine—individual tools do not decide whether a call is allowed. Every path from the model passes `path::confine` so requests cannot escape workspace roots. Shell commands run under the platform sandbox unless the session explicitly opts into `danger-full-access`.
