# Design: `cox-protocol` (T0.6)

## Problem

D11 commits to four surfaces (`cox`, `cox run -p`, `cox acp`, `cox mcp`) over one event
stream, each capped at ≤ 300 LOC. `cox-protocol`'s `Event` enum has **19 variants** and
`Submission` has **9** (verified against `crates/cox-protocol/src/types.rs`) — that is
what every surface adapter must translate without touching the network, filesystem or a
process directly (D2). The measurable question: can 4 surfaces × ~28 variants stay inside
four 300-LOC adapters, or does the enum outgrow what JSON-RPC/stream-json/ACP can carry?

## The field

**Codex (R§1.2).** A submission queue / event queue: the core takes `Submission`s and
emits `Event`s; `Thread → Turn → Item` is the hierarchy, item deltas stream, queues are
bounded with an explicit overload error (`-32001`), submissions carry stable ids so they
auto-start when idle. Four consumers — TUI, `codex exec`, an app-server (JSON-RPC 2.0
over stdio/WebSocket/Unix socket for IDEs), and an MCP server — read the same queue. This
is the direct ancestor of D2/D11.

**Claude Code (R§2.1).** `--output-format stream-json` emits one JSON object per line;
headless mode (`-p`) and the Agent SDK both wrap that same line stream, not a separate
protocol. No published app-server RPC — the CLI is the integration surface.

**Copilot CLI (R§2.1).** `--headless --port` starts a server with a TypeScript SDK on
top; turn-based with preview-before-execute approval gates, unlike Codex's queue.

**ACP (R§3).** JSON-RPC over stdio; `session/*` and `*/update` notifications carry the
same shape as a streamed event log. Zed, JetBrains and neovim speak it (crate
`agent-client-protocol` 2.0.0). Cost to implement over an existing event stream: ~300 LOC
(R§3) — exactly D11's budget for `cox-acp`.

## cox

`Submission` in, `Event` out (D2) — `cox-core` is the only thing that mutates session
state; every surface, including the JSONL rollout, folds the same `Event` sequence.
`cox` (TUI) renders events into ratatui cells; `cox run -p --output-format stream-json`
serializes them one-per-line, matching Claude Code's line protocol (R§2.1) without a new
wire format; `cox acp` maps `Event` to ACP `session/update` and `Submission::Approve` to
ACP permission responses — the ~300 LOC Codex/ACP precedent (R§1.2, R§3) is why D11
budgets each surface the same; `cox mcp` exposes built-in tools, not the turn loop, so it
barely touches `Event`. `Store::rollout_append` is a fifth consumer for free (§1.2,
§1.3).

Borrowed: Codex's SQ/EQ split and `Thread → Turn → Item` (renamed `Session → Turn →
Item`); Claude Code's newline-delimited JSON as the headless format; ACP's notification
shape for the editor surface. Dropped for v0.1: Codex's JSON-RPC app-server — no IDE
socket protocol of our own, ACP covers that role (R§1.3: "no app-server in v0.1");
Copilot's turn-based preview-gate, since D2's `ApprovalRequired`/`Submission::Approve`
already blocks per-call. Everything above `cox-core` speaks the same serde-tagged JSON
(§1.2: `#[serde(tag = "type")]`), so `stream-json` and the rollout are shape-identical to
what ACP and MCP marshal.

## Falsifiers

This design is wrong if any of the following turns out true:

1. A surface needs session state `Event` doesn't carry — e.g. ACP's `session/load`
   needs full transcript replay and folding `ItemStarted`/`ItemDone` pairs (§1.3 rule 6)
   can't reconstruct it byte-identically from the rollout.
2. An `Event` cannot be replayed deterministically — two runs of `cox-core` against the
   same `Scripted` provider and `Submission` sequence produce different `Event`
   sequences (breaks golden-JSONL testing, R§5.2, and the
   `resume_builds_identical_request` invariant).
3. `cox-acp` exceeds 300 LOC to cover `session/new`, `session/prompt`, `session/update`,
   permission requests and client fs/terminal calls — the R§3 estimate was optimistic
   for cox's specific `Event` shape, not just ACP in the abstract.
4. `cox run -p --output-format stream-json` diverges from Claude Code's stream-json
   (field names, line framing) enough that tooling built for one can't read the other —
   undermining the "no new wire format" claim.

## Review

(think-tier review pending)
