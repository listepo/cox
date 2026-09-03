# Design: tier routing (T9.4)

## Problem

Routing picks the price of every provider call, and tier prices differ by an
order of magnitude (Haiku 4.5 $1/$5 vs Sonnet 5 $2/$10 vs Fable 5.1 $10/$50
per MTok, plan.md §1.4). One miscategorised job — a summary on the think
tier, a coding turn silently downgraded — moves the session bill by 5–10×
with nothing in the transcript to explain it. The measurable question: can
every request carry a job tag into the ledger (D5) whose tier a user could
have predicted from the config file alone?

## The field

**Copilot (R§4.4).** Auto-routing across models, praised because it is
explicit, priced (10 % discount for auto) and switchable — the user sees the
choice and the bill agrees with it.

**Cursor / aider / OpenCode (R§4.4).** Cursor auto-picks; aider pins
`--weak-model` to commits and summaries; OpenCode pins a small model to
titles. Same pattern in three shapes: cheap work is named in config, not
detected per call.

**Claude Code (R§2.1).** Silent Haiku delegation for titles, compaction and
search — the most-cited complaint in the survey, precisely because the tier
is invisible and the user cannot audit or override it.

## cox

Ten jobs pinned to three tiers in `[jobs]` (`Router::pick`, pure over config
+ session overrides): main→code, plan→think, everything mechanical→cheap.
Three rules keep it auditable: **never up** — a retry re-resolves the same
route, escalation needs `/model` or a flag; **think gates** — the think tier
refuses without `confirm_think` and shows the price in the refusal notice;
**every request is tagged** — the ledger row carries job and tier, so `cox
stats` shows what each tier cost. `/model <tier> [model]`, `--tier
TIER=MODEL` and `--provider local` are the only overrides, all visible in
the session (`ModelSwitched`, config provenance).

Borrowed from aider/OpenCode: cheap work named in config. Borrowed from
Copilot: explicit and priced. Rejected from Claude Code: silent delegation —
there is no path in the code where the tier changes without an event or a
flag. Anthropic's own guidance (measure the capable model at lower effort
first, caches are model-scoped) is why effort, not a second model, is the
first dial; the router carries effort per tier for the same reason.

## Falsifier

A job where the cheap tier's quality measurably costs more in retries than
it saves: same task set on cheap vs code, total ledger cost (including the
extra turns) lower on code. `just bench` already replays sessions per
mechanism; the same harness with the tier swapped is the experiment. If a
pinned-cheap job fails it twice, the pin moves up in `[jobs]` — one config
line, no code.

*Review: `think`-tier review pending (same standing as T0.6).*
