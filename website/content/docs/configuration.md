---
title: "Configuration"
weight: 3
---

## Planned precedence

cox is designed around one configuration vocabulary: every CLI flag maps to a configuration key. Values resolve in this order, with later sources taking priority:

1. Built-in defaults
2. `~/.cox/config.toml`
3. `.cox/config.toml` at the Git root
4. `COX_<SECTION>_<KEY>` environment variables
5. Command-line flags

`cox config show --sources` is intended to show both the effective value and where it came from.

## Environment files

`.env` and `.env.local` may supply otherwise-unset process environment variables before configuration loads. They are not another configuration layer and never replace environment variables already provided by CI or the shell.

## Permission and sandbox defaults

cox is planned to start with a sandbox enabled and an approval policy that is explicit about risk. Supported sandbox modes are read-only, workspace-write, and danger-full-access. The workspace-write mode keeps `.git` and `.cox` read-only.

Permission rules can be imported from an existing `.claude/settings.json` setup, while cox-native configuration stays in `.cox/config.toml`.

## Model routing

Jobs are grouped into cheap, code, and think tiers. Background tasks such as compaction and summaries stay in the cheap tier. A higher-cost thinking tier requires an explicit user confirmation—cox does not silently escalate a request to a more expensive model.
