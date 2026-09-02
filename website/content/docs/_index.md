---
title: "Documentation"
weight: 1
---

## Welcome aboard

**cox is currently under active development.** Its goal is a dependable coding agent that stays useful across an interactive terminal, CI scripts, editors, and other agents—without changing the core behavior underneath.

### What cox is building

- **A terminal-first experience** with an inline, scrollback-friendly TUI.
- **Automation surfaces** for headless prompts, stream-JSON output, ACP editor clients, and MCP consumers.
- **A pure core**: submissions go in and typed events come out, making agent behavior replayable and testable.
- **Visible economics**: request usage and cost are recorded; tool output is archived before it is shortened for context.
- **Practical safety**: permission decisions live in one engine, paths are confined to the workspace, and shell execution is sandboxed by default.

## Project status

The Rust workspace and foundational protocol, provider, tool, storage, and configuration work are in progress. The public API and installation flow are not yet stable; treat this site as design documentation rather than a release manual.

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
- [Configuration]({{< relref "configuration" >}}) describes the planned precedence and safety defaults.

## Design principles

cox favors small, explicit components over opaque automation. It does not let a tool decide its own permissions, does not hide model routing, and does not discard long output merely to fit it into context. These constraints keep the agent inspectable when it matters most.
