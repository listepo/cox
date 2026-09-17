---
title: "Getting started"
weight: 0
---

cox is a modular terminal coding agent in Rust. One core state machine turns submissions into typed events; the same stream powers the TUI, headless runs, editor clients (ACP), and MCP.

## Build and first run

Rust is pinned with [mise](https://mise.jdx.dev/). Prefer `mise exec -- cargo …` over a global toolchain.

```bash
git clone https://github.com/listepo/cox && cd cox
mise exec -- cargo build -p cox
export ANTHROPIC_API_KEY=sk-...   # or OPENAI_API_KEY
./target/debug/cox doctor         # green except prices? you are good
./target/debug/cox -p "create hello.txt containing hi"
./target/debug/cox                # interactive TUI: Enter sends, Esc interrupts
```

In the TUI: `y` / `s` / `n` answer approval prompts, `/model` switches tiers, `/compact` compacts context now. Headless scripts use `cox run -p`; editors use `cox acp`; other agents can call `cox mcp`.

## What to read next

- [Configuration]({{< relref "configuration" >}}) — every key and precedence
- [Tools]({{< relref "tools" >}}) — built-in tools, risk, subjects
- [Observability]({{< relref "observability" >}}) — traces, metrics, OTLP
- [Editors]({{< relref "ide" >}}) — Zed, JetBrains, Neovim via ACP
- [How it works]({{< relref "how-it-works" >}}) — one user turn on the event stream
- [Compat]({{< relref "compat" >}}) — what cox reads from `.claude/` / Codex setups

Costs land in `cox stats`.

## Status

cox is under active development. APIs, configuration, and install paths are not yet stable. Treat these pages as the manual for the current tree.
