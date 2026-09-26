# Design: subagent messaging (T34.0)

## Problem

Today 0 of N running or finished subagents can receive a follow-up: `agent`
is one-shot (`crates/cox-core/src/subagent.rs`), the only "child speaks
first" channel is the one-shot `relay_approval`, and a finished child's
history is reachable only by starting an unrelated new `agent` call.
Measurable question: can a parent and its children exchange more than one
message each, addressed only by the `TaskId` the model already holds
(`TaskCreated`/`TaskCompleted`), through the existing pure
`Submission`-in/`Event`-out stream (D2) — no side channel?

## The field (research.md §4.3.7)

**Claude Code**: `SendMessage`+`SubagentHandoff` resumes a *finished*
subagent with full history, but gated off by default behind
`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` (M3–M5), plus a sibling roster
injected into every subagent's prompt once teams are on (M5, rejected
below). **Codex CLI**: one-directional only, no resumption, no progress
streaming (M9–M10). **OpenCode**: messaging undocumented; only its
visibility gate (`permission: deny`, M11) is confirmed, reused by T34.10.
cox already beats Claude Code's parallelism (M2) and Codex's one-shot
result; the gap is strictly the two-way follow-up.

## Design

1. **Wire shape** (T34.4, `cox-protocol/src/types.rs`):
   `Submission::TaskMessage { task: TaskId, from: Option<TaskId>, hop: u32, text: String }`
   and a matching `Event::TaskMessage` with the same fields. `task` is
   always the addressee; `from: None` is the parent/user, `Some(id)` a
   sibling. Both are only ever submitted to / emitted by the **parent**
   session (D2). `hop` is set only by the parent, never trusted from a tool
   call (D14; see point 5).
2. **Running vs. finished** (T34.5): `Session::inner.tasks` gains, per
   `TaskId`, a live child `Session` handle while running, downgraded on
   `complete_task` to `SessionId`+`job`+`tier`+`parent_id` (kept, not
   dropped). Delivery to a running task queues `Submission::UserTurn {
   text: "[message from {sender}] {text}", .. }` behind its current turn,
   never mid-turn. Delivery to a finished task loads its `History` and
   resumes with the same `job`/`tier`/`parent_id`/budget slice. **Note**:
   today's `Session::resume` hardcodes `parent_id: None, Job::Main,
   Tier::Code` (top-level only); it needs parametrizing to carry those
   three through — touches `cox-core/src/session.rs` beyond the card's
   listed files.
3. **Child → parent / sibling routing**: `to: "parent"` never queues a
   `Submission` — it appends a pointer line to the parent's history after
   the last cache breakpoint (D6e), exactly like `publish_task_result`,
   and emits `Event::TaskMessage`. A child never gets a handle to another
   child: its call reaches its own session's `emit`; the parent's
   `run_task` loop (already switching on child `Event`s for
   `relay_approval`) turns the new variant into the parent's own
   `Submission::TaskMessage` — one more match arm, no second relay path.
   `ask_user` (T34.3) stays the only channel that can block a child.
4. **The tool** (T34.6, `cox-tools/src/send_message.rs`): `send_message {
   to: string, text: string }`. `to` is `"parent"`, a sibling's registry
   name (`explore-2`, the string `Source.agent` already carries) or its
   `TaskId`; the parent resolves either form. Available to a subagent whose
   preset/def grants it (none does yet, same content decision as T34.3
   point 4) and to the parent itself (`to: <child name/id>`). The child
   learns its own `TaskId` via a new `Session::self_task: Option<TaskId>`
   set in `spawn_child`, so its call can stamp `from`.
5. **Flood/loop guards** (T34.6). There are two named constants in
   `subagent.rs`, not config keys (YAGNI; promote them only if a user asks):
   - `MAX_MESSAGES_PER_TASK = 16`: a per-`TaskId` received-message counter
     denies the 17th delivery (`ToolError::Denied`, shaped like T34.2's
     denial).
   - `MAX_HOPS = 4`: the hop limit is causal, not a session-wide counter.
     A message from the parent has `hop = 0`. When a message with hop `h`
     starts or queues a turn in child B, the parent records `h` as B's
     current hop. Any message B sends during that turn gets `h + 1`, and the
     parent denies it once `h + 1 > MAX_HOPS`. An A→B→A→B ping-pong stops
     after four relays, while independent sibling messages (each at hop 1)
     are never throttled by one another.
   T34.2's concurrency cap still bounds how many children can talk at all.
6. **Trust and surfaces**: `send_message` grants no tool access itself —
   the queued/resumed turn it starts still goes through
   `cox_permission::Engine`; every rendered line runs through
   `cox_sanitize::sanitize` (D14). `Event::TaskMessage` renders as one
   transcript line (T34.7, `insta`) labelled like `ApprovalRequired`'s
   `Source` already labels a relayed approval, and updates the `/agents`
   card (A29, no new progress stream); `stream-json` needs no special case
   (D2 passthrough) — T34.7 adds the test proving it. ACP (T34.8) gives
   `TaskCreated`/`TaskCompleted`/`TaskMessage` their own arms in
   `drive_prompt` instead of today's `Ok(_) => {}`.

## Out of scope

Persistent teammates outliving their task, split-pane processes, a shared
task board, a sibling roster injected into every subagent's prompt (breaks
the cache-stable prefix, D6e) — a child addresses a sibling only by the
name/`TaskId` its parent put in its task text. Protocol scope is the one
`Submission`/`Event` pair above, nothing wider.

## Falsifier

If, after T34.9's e2e (two children messaging through the parent, plus a
scripted ping-pong), the ping-pong gets more than
`MAX_HOPS` relays before denial, an independent hop-1 message is denied, or a delivered
message ever lands mid-turn instead of after a cache breakpoint (a
cache-write spike in the ledger for that turn), the design is wrong and
this doc is amended before T34.7–T34.9 continue.

*Review: `think`-tier review pending, per D15.*
