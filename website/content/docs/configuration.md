---
title: "Configuration"
weight: 3
---

## Precedence

cox uses one configuration vocabulary: every CLI flag maps to a configuration key. Values resolve in this order; **later sources win**:

1. Built-in defaults
2. `~/.cox/config.toml`
3. `.cox/config.toml` at the Git root
4. `COX_<SECTION>_<KEY>` environment variables
5. Command-line flags

`cox config show --sources` prints the effective value and where it came from.

## Environment files

`.env` and `.env.local` supply otherwise-unset process environment variables before configuration loads. They are not another configuration layer and never replace variables already set by the shell or CI.

## Permission and sandbox defaults

The sandbox is **on** by default. Default mode is **workspace-write**; `.git` and `.cox` stay read-only even in that mode. Other modes:

- **read-only** — no writes under workspace roots
- **workspace-write** — writes inside the workspace except `.git` and `.cox`
- **danger-full-access** — no sandbox (explicit opt-in only)

Permission rules can be imported from an existing `.claude/settings.json` setup; cox-native configuration stays in `.cox/config.toml`.

## Model routing

Jobs are grouped into cheap, code, and think tiers. Background tasks such as compaction and summaries stay in the cheap tier. The think tier requires explicit user confirmation—cox does not silently escalate a request to a more expensive model.
