# Design: the turn loop (T2.8)

## Problem

cox has one `Session` that must serve a TUI, headless `cox run -p`, ACP, and a
JSONL rollout from the same `Event` stream (D2). The measurable question: can
six named rules in plan.md §1.3 stay true while tools run in parallel and the
user interrupts mid-call, without each surface growing its own agent loop?

## The field

**Claude Code (R§2.1).** A classic `while tool_use` loop: the model streams, the
CLI runs the requested tools (foreground or background subagents), feeds one
combined tool-result message back, repeats until the model stops. `/loop` and
task chips sit on top of that same cycle. Permission is mixed into the tool
runner, not a separate pure engine. There is no public submission/event queue —
the TUI *is* the loop.

**Codex (R§1.2).** Hierarchy `Thread → Turn → Item`. A submission queue feeds the
core; an event queue fans out to the TUI, `codex exec`, the app-server, and an
MCP server. Turns stream item deltas. Interrupt and approval are submissions,
not TUI-private state. This is the shape cox copies, renamed `Session → Turn →
Item`.

**Pi (R§2.2).** Deliberately minimal: four tools (read/write/edit/bash), no MCP,
no permission engine, isolation by container rather than an in-process policy.
The loop is small enough to keep in one file; cox rejects that bound because D7
(sandbox) and D2 (many surfaces) need a state machine other consumers can test
without a terminal.

## cox

`Session::submit` takes a `Submission`; `events()` yields `Event`s; `step()` is
one provider call plus its tool batch. I/O only through `Provider` / `Tool` /
`Store` / `Archive`. Parallel `JoinSet` with `Exclusive` tools serialised; all
tool results of one assistant message return in one user message, emission
order. Permission always-allow until T2.2. Compaction and resume are later
steps on the same stream.

Borrowed from Codex: SQ/EQ, `Turn`/`Item`, interrupt as a submission. Borrowed
from Claude Code: `while tool_use` until `EndTurn`, one user message of tool
results. Dropped from Pi: "the loop is the product" — cox's product is the
event stream.

## The six rules of §1.3

1. All tool results for one assistant message go back in one user message, in
   emission order. Test: `turn_all_tool_results_return_in_one_message`.
   Falsified if a parallel batch is split across two `Role::User` messages or
   reordered relative to `ToolUseStart`s.
2. An `ApprovalRequired` blocks only that call — other approved parallel calls
   proceed. Test: (T2.2) `ask_then_approve` / loop scenario. Falsified if an
   Ask stalls the whole `JoinSet`.
3. `Interrupt` cancels the provider stream and every running tool via the
   shared token, then emits the partial assistant item and
   `TurnDone{Interrupted}`. Test: `turn_interrupt_mid_tool_snapshot`. Falsified
   if a tool ignores `ToolCx.cancel` or events continue after `TurnDone`.
4. No `Event` is emitted after `TurnDone` for that turn. Test:
   `turn_no_event_after_turn_done`. Falsified if `try_recv` after `TurnDone`
   yields another event for the same turn.
5. The archive row exists before the model sees truncated text. Test: (T2.5)
   `truncate_is_lossless_via_archive`. Falsified if `visible` is shortened
   before `Archive::put` returns.
6. The request built after resume from the rollout is byte-identical to the
   one a live session would have built. Test: (T2.4)
   `resume_builds_identical_request`. Falsified if `History::from_events`
   drops a delta or reorders `system[0..=2]`.

## Falsifiers

This design is wrong if any of the following turns out true:

1. A surface needs a second loop (e.g. ACP permission RPC that cannot be
   expressed as `Submission::Approve`) — D2's "one core" claim fails.
2. Golden `Event` JSONL for the six T2.1 scenarios diverges across runs with
   the same `Scripted` fixture — the loop is not a state machine.
3. Rule 1 is abandoned to "fix" a provider that requires per-call tool-result
   messages — cache breakpoints (T2.3) and resume (T2.4) then disagree.

## Review

(think-tier review pending)
