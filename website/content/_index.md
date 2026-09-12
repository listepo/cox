---
title: "cox"
description: "A modular terminal coding agent in Rust — one binary, four surfaces."
---

**cox v0.1** is a modular terminal coding agent in Rust. One core state machine turns submissions into typed events; every surface consumes the same stream.

| Surface | Command | Use |
| --- | --- | --- |
| TUI | `cox` or `cox [PROMPT]` | Interactive terminal sessions |
| Headless | `cox run -p` | Scripts and CI (`text`, `json`, or `stream-json`) |
| Editors | `cox acp` | Agent Client Protocol for Zed, JetBrains, and other clients |
| MCP | `cox mcp` | Built-in tools for other agents |

Build from the repo with [mise](https://mise.jdx.dev/): `mise exec -- cargo build -p cox`. Set `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`, run `cox doctor`, then start with `cox` or `cox run -p`.

Read the [documentation](/docs/) for configuration, architecture, and observability.
