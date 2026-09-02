# cox

> A modular terminal coding agent in Rust.

**cox** is named for the coxswain: it steers the work while models, tools, and extensions row. It is being built as one reliable, testable agent core with several ways to use it: an interactive terminal UI, headless automation, editor integration through ACP, and MCP tools for other agents.

[Documentation](https://listepo.github.io/cox/) · [Architecture](https://listepo.github.io/cox/docs/architecture/) · [Configuration](https://listepo.github.io/cox/docs/configuration/)

## Status

cox is under active development. APIs, configuration, and installation instructions are not yet stable. The Rust workspace already contains the protocol, core turn loop, provider adapters, tools, storage, extension loading, TUI, ACP, and MCP crates; features are completed incrementally against the project plan.

## Design principles

- **One event stream.** `Submission` values enter a pure core state machine and typed `Event` values leave it. Every surface consumes the same events.
- **Safe by default.** Permission decisions are centralized, model paths stay inside the workspace, and shell commands run in a sandbox unless users deliberately choose otherwise.
- **Lossless context.** Full tool output is archived before it is shortened for model context, so it remains retrievable by ID.
- **Visible costs.** Provider usage is recorded per request; routing between cheap, code, and think tiers is explicit.

## Develop

Rust is pinned through [mise](https://mise.jdx.dev/). Run Cargo through mise rather than a global toolchain:

```bash
mise exec -- cargo fmt --check
mise exec -- cargo clippy --workspace --all-targets -- -D warnings
mise exec -- cargo test --workspace
```

## Documentation site

The public documentation site is a Hugo project in [`website/`](website/), styled with Tailwind CSS.

```bash
cd website
npm ci
npm run build:css
hugo --minify --destination public
```

GitHub Actions builds and deploys the site from `main` to [GitHub Pages](https://listepo.github.io/cox/).

## Project layout

| Area | Responsibility |
| --- | --- |
| `crates/cox` | CLI surface and dispatch |
| `crates/cox-protocol` | Shared types and traits |
| `crates/cox-core` | Agent state machine, context, permissions, budgets |
| `crates/cox-provider` | Provider adapters and replay fixtures |
| `crates/cox-tools` | Built-in tools, path confinement, sandbox |
| `crates/cox-store` | SQLite sessions, archives, usage ledger |
| `crates/cox-ext` | Instructions, skills, commands, hooks |
| `crates/cox-tui` | Terminal UI |
| `crates/cox-acp` / `crates/cox-mcp` | Editor and MCP integrations |

For design decisions, milestones, and task checks, see [`plan.md`](plan.md). For evidence behind the design, see [`research.md`](research.md).
