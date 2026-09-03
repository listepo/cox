# Compatibility notes

Manual smoke checks that no automated test covers. Each entry says what was
run, on what, and when.

## `cox mcp` as an MCP server (T6.2)

Claude Code / any MCP client `.mcp.json` entry:

```json
{ "mcpServers": { "cox": { "command": "cox", "args": ["mcp"] } } }
```

Add `"--allow-write"` to also serve `edit`, `write` and `apply_patch`, or
`"--tools", "bash,read"` to name the exact set (the only way to get `bash`).
Calls go through the permission engine with the approval policy forced to
`never`: anything that would ask in the TUI is denied with the reason in the
tool result. Paths are confined to the workspace roots.

Smoke 2026-09-03, macOS, stdio JSON-RPC by hand: `initialize` reports
`cox 0.1.0` with the tools capability; `tools/list` returns `read`, `grep`,
`glob` by default; `read note.txt` returns the numbered file; `write` is
"unknown tool" without `--allow-write` and a denied result with it under the
default permission mode; `read ../note.txt` is rejected by path confinement.
