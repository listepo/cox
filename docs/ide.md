# cox in editors (T11.2)

`cox acp` serves cox over the Agent Client Protocol on stdio: the same
`Event` stream as the TUI, so prompts, tool calls, approvals and diffs show
up in the editor instead of the terminal. It needs no API key to start (only
to call a model), and answers `initialize` / `session/new` with no config.

## Zed

Add cox as a custom agent server in `settings.json`
(`Cmd-,` → `zed: open settings`, or `~/.config/zed/settings.json`):

```json
{
  "agent_servers": {
    "cox": {
      "type": "custom",
      "command": "cox",
      "args": ["acp"],
      "env": {}
    }
  }
}
```

Then open the Agent Panel and start a thread with `cox`. Model keys come
from the usual places (`ANTHROPIC_API_KEY` in `env` above, `.env`, or the
system keyring) — see `cox doctor`. File reads/writes and shell commands
run through Zed's own buffers and terminals automatically, because Zed
offers the `fs` and `terminal` capabilities and `cox acp` prefers them over
its local tools. Approval prompts (`y`/`s`/`n` in the TUI) arrive as Zed
permission dialogs with allow / allow-always / reject options.

## JetBrains

Install an ACP-compatible plugin from the JetBrains Marketplace, add a
custom agent server, and point its command at:

```sh
cox acp
```

Working directory defaults to the open project. The same capability rule
applies: with `fs`/`terminal` offered, edits land in editor buffers.

## Neovim

Use any ACP-capable neovim plugin and configure its agent command as
`cox acp` (no extra flags). Headless `cox run -p` remains the better fit
for scripted editor integrations; `cox acp` is for interactive threads.

## Troubleshooting

- **No reply on stdio**: send one JSON-RPC line at a time, newline
  terminated. `echo '{"jsonrpc":"2.0","id":1,"method":"initialize",
  "params":{"protocolVersion":1}}' | cox acp` must print exactly one
  response line with `protocolVersion: 1`.
- **v1 only**: `cox acp` answers protocol version 1 even when the client
  offers the v2 draft; clients fall back automatically.
- **Sessions die with the server**: `session/load` resumes sessions the
  running server still holds; restarting `cox acp` drops them.
EOF
echo written