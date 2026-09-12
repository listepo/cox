---
title: "Documentation"
weight: 1
---

## Welcome aboard

**cox** is a modular Rust terminal coding agent: submissions enter a pure core state machine and typed events leave it. The same stream powers an interactive TUI, headless automation, editor clients, and MCP consumers—so behavior stays consistent wherever you run cox.

- **A terminal-first experience** with an inline, scrollback-friendly TUI.
- **Automation surfaces** for headless prompts, stream-JSON output, ACP editor clients, and MCP consumers.
- **A pure core**: submissions go in and typed events come out, making agent behavior replayable and testable.
- **Visible economics**: request usage and cost are recorded; tool output is archived before it is shortened for context.
- **Practical safety**: permission decisions live in one engine, paths are confined to the workspace, and shell execution is sandboxed by default.

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

- [Architecture]({{< relref "architecture" >}}) explains the core event model and crate boundaries.
- [Configuration]({{< relref "configuration" >}}) describes precedence, sandbox defaults, and model routing.
- [Observability]({{< relref "observability" >}}) covers traces, metrics, and the usage ledger.

## Design principles

cox favors small, explicit components over opaque automation. It does not let a tool decide its own permissions, does not hide model routing, and does not discard long output merely to fit it into context. These constraints keep the agent inspectable when it matters most.
