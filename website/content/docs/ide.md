---
title: "Editors"
weight: 7
---

`cox acp` serves cox over the Agent Client Protocol on stdio. Editors see the same `Event` stream as the TUI—prompts, tool calls, approvals, and diffs—without a separate terminal session. It starts without an API key; model keys load from the usual places when you call a model.

## Zed

Add cox as a custom agent server in `settings.json` (`Cmd-,` → **zed: open settings**, or `~/.config/zed/settings.json`):

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

Open the Agent Panel and start a thread with **cox**. When Zed offers `fs` and `terminal` capabilities, file and shell work goes through editor buffers and terminals; approval prompts appear as Zed permission dialogs.

## JetBrains

Install an ACP-compatible plugin from the JetBrains Marketplace, add a custom agent server, and set its command to `cox acp`. Working directory defaults to the open project; with `fs`/`terminal` offered, edits land in editor buffers.

## Neovim

Use any ACP-capable Neovim plugin and set the agent command to `cox acp` (no extra flags). For scripted integrations, `cox run -p` fits better; `cox acp` is for interactive threads.

## Troubleshooting

- **`cox` not found** — the `command` must resolve on your PATH (or use an absolute path to the binary).
- **No reply on stdio** — send one JSON-RPC object per line, newline-terminated. A one-line `initialize` request to `cox acp` should print exactly one response line.
- **Protocol version** — `cox acp` speaks protocol version 1; clients that offer a newer draft fall back automatically.
- **Sessions** — `session/load` only resumes sessions the running server still holds; restarting `cox acp` drops in-memory sessions.
