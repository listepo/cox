---
title: "How it works"
weight: 8
---

## One event stream

One idea explains cox: a `Submission` goes into a pure core state machine, a sequence of `Event`s comes out, and every surface renders that same sequence. The TUI, headless `cox run -p`, the ACP editor server, the MCP server, the JSONL rollout on disk, and the test suite are all consumers of one event stream.

```text
                ┌────────────────────────────────────────┐
  you ──Submission──▶│ cox-core: Session state machine    │──▶ Event stream ──▶ TUI
script ──UserTurn───▶│  assemble → route → stream → tools │──▶ stream-json      ACP
editor ──Approve────▶│  (no network / fs / process here)  │──▶ rollout.jsonl    tests
                └────────────────────────────────────────┘
```

Everything the core needs from the outside world — models, files, shells, stored sessions — arrives through traits in `cox-protocol`. That is what makes the loop testable without a model: a scripted provider plus a golden event log.

## Example: one turn with a tool

In the TUI, you type:

```bash
cox
# > create hello.txt containing hi
```

Inside `cox-core`, the turn proceeds like this:

1. `Submission::UserTurn { text: "create hello.txt containing hi", .. }` enters the session.
2. The core assembles a provider request, picks the model tier, and streams the model.
3. The model emits a `write` tool use. The core emits events in order:

```json
{"type":"tool_call_requested","call":{"name":"write","input":{"path":"hello.txt","content":"hi\n"}}}
{"type":"tool_call_done","call_id":"…","result":{"ok":true,"visible":"wrote 3 bytes to hello.txt","bytes":3}}
{"type":"text_delta","text":"Created hello.txt."}
{"type":"turn_done","stop":"end_turn"}
```

4. Tool results go back to the model in one user message, in emission order. No `Event` is emitted after `TurnDone` for that turn.

The same turn in headless mode prints the identical JSON events, one per line:

```bash
cox run -p "create hello.txt containing hi" --output-format stream-json
```

Interrupts, permissions, compaction, archival, and trust boundaries all operate on this same stream.
