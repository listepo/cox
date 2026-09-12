---
title: "Compatibility"
weight: 6
---

A repository already set up for Claude Code or Codex works with cox without
reconfiguration. cox reads the same instruction files, slash commands, agent
definitions, MCP servers, and hook settings those tools expect, and ignores
what they keep in their own config stores.

## What cox reads

| Source | cox reads | Notes |
| --- | --- | --- |
| `AGENTS.md` / `CLAUDE.md` hierarchy | yes | same lookup order |
| `SKILL.md` agent skills | yes | same frontmatter |
| `.claude/settings.json` | yes, read-only | permissions, hooks, and env; one config layer, never written back |
| `.claude/commands/*.md` | yes | same slash-command palette |
| `.claude/agents/*.md` | yes | same agent definitions; `explore` and `shell` presets ship embedded |
| `.mcp.json` servers | yes | stdio and Streamable HTTP; `${ENV}` expansion |
| Codex `apply_patch` (V4A) | yes | Add/Update/Delete/Move with `@@` context |
| `--output-format stream-json` | yes | same event shapes for scripts |

## What cox does not read

| Source | cox reads | Notes |
| --- | --- | --- |
| `~/.codex/config.toml` | no | Codex profiles, features, and sandbox settings are not imported |
| MCP resources, prompts, and OAuth | no | deferred to v0.2 |
| Image input on provider calls | no | deferred to v0.2 |

cox does not mirror Codex's global config file. A checkout's repo-local
`.claude/` tree and `.mcp.json` are the compatibility surface; anything
that lives only under `~/.codex/` stays outside cox's configuration model.

## `cox mcp` as an MCP server

cox also exposes its built-in tools as an MCP server. Add it to any MCP
client's `.mcp.json`:

```json
{ "mcpServers": { "cox": { "command": "cox", "args": ["mcp"] } } }
```

By default the server advertises `read`, `grep`, and `glob`. Pass
`--allow-write` to also serve `edit`, `write`, and `apply_patch`, or
`--tools` with an explicit list (the only way to include `bash`). Every
call goes through the permission engine with approval policy forced to
`never`: anything that would prompt in the TUI is denied and the reason
returns in the tool result. Paths are confined to workspace roots.
