---
title: "Documentation"
weight: 1
---

## What cox is

**cox** is a modular Rust terminal coding agent. Submissions enter a pure core state machine; typed events leave it. The same stream powers the TUI, headless runs, ACP editors, and MCP consumers.

- **TUI first** — inline, scrollback-friendly terminal UI.
- **Four surfaces** — `cox`, `cox run -p`, `cox acp`, `cox mcp`.
- **Pure core** — replayable, testable turn loop.
- **Visible costs** — usage recorded per request; long tool output archived before it is shortened.
- **Safe by default** — one permission engine, workspace path confinement, sandboxed shell unless you opt out.

## Project status

cox **v0.1** ships the full CLI surface, configuration layers, sandbox, and all four entry points (`cox`, `cox run -p`, `cox acp`, `cox mcp`). Treat these pages as the manual for the current tree.

## Develop cox

The repository pins Rust with [mise](https://mise.jdx.dev/). After cloning, run the quality gates through mise:

```bash
mise exec -- cargo fmt --check
mise exec -- cargo clippy --workspace --all-targets -- -D warnings
mise exec -- cargo test --workspace
```

The documentation site itself lives in `website/`. Install its Node dependencies with `npm ci`, run `npm run build:css`, then build with Hugo. GitHub Actions publishes the `main` branch build to this site.

## Documentation map

- [Getting started]({{< relref "getting-started" >}}) is the 60-second path from clone to first prompt.
- [Architecture]({{< relref "architecture" >}}) explains the core event model and crate boundaries.
- [Configuration]({{< relref "configuration" >}}) describes precedence, sandbox defaults, and model routing.
- [Observability]({{< relref "observability" >}}) covers traces, metrics, and the usage ledger.
- [Tools]({{< relref "tools" >}}) lists built-in tools, risks, and how permission decisions are centralized.
- [Compat]({{< relref "compat" >}}) describes what cox reads from `.claude/` and `.codex/` setups.
- [IDE]({{< relref "ide" >}}) covers connecting Zed, JetBrains, and other editors via ACP.
- [How it works]({{< relref "how-it-works" >}}) walks through one user turn on the event stream.

## Design principles

cox favors small, explicit components over opaque automation. It does not let a tool decide its own permissions, does not hide model routing, and does not discard long output merely to fit it into context. These constraints keep the agent inspectable when it matters most.
