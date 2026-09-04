# How cox works, with examples

One idea explains cox: a `Submission` goes into a pure core state machine,
a sequence of `Event`s comes out, and every surface renders that same
sequence. The TUI, headless `cox run -p`, the ACP editor server, the MCP
server, the JSONL rollout on disk, and the test suite are all consumers of
one event stream (`cox-protocol::types::{Submission, Event}`).

```text
                ┌────────────────────────────────────────┐
  you ──Submission──▶│ cox-core: Session state machine    │──▶ Event stream ──▶ TUI
script ──UserTurn───▶│  assemble → route → stream → tools │──▶ stream-json      ACP
editor ──Approve────▶│  (no network / fs / process here)  │──▶ rollout.jsonl    tests
                └────────────────────────────────────────┘
```

Everything the core needs from the outside world (models, files, shells,
stored sessions) arrives through traits in `cox-protocol`
(`Provider`, `Tool`, `Store`, `Hook`, `Archive`). That is what makes the
loop testable without a model: a scripted provider plus a golden event log.

For the full contract see `plan.md` §1.2–§1.3; for per-component rationale
see `docs/design/`. This file is the walkthrough.

## Example 1: one turn in the TUI

```bash
./target/debug/cox
# > create hello.txt containing hi
```

What happens inside `cox-core` (`plan.md` §1.3), simplified:

1. `Submission::UserTurn { text: "create hello.txt containing hi", .. }`
   enters the session. `UserPromptSubmit` hooks may rewrite or block it.
2. The core assembles a provider-neutral `Request` (system prompt, tool
   schemas, instruction files, history), picks the `code` tier
   (`claude-sonnet-5`), and streams the model.
3. The model emits a `write` tool use. The core emits, in order:

```json
{"type":"tool_call_requested","call":{"name":"write","input":{"path":"hello.txt","content":"hi\n"}}}
{"type":"tool_call_done","call_id":"…","result":{"ok":true,"visible":"wrote 3 bytes to hello.txt","bytes":3}}
{"type":"text_delta","text":"Created hello.txt."}
{"type":"turn_done","stop":"end_turn"}
```

4. All tool results for that assistant message go back to the model in
   **one** user message, in emission order — even when the calls ran in
   parallel. No `Event` is ever emitted after `TurnDone` for that turn.

Interrupt (`Esc`) cancels the provider stream and every running tool
through one shared token, then emits the partial assistant text and
`TurnDone{Interrupted}`.

## Example 2: the same turn, headless

```bash
export ANTHROPIC_API_KEY=sk-...
./target/debug/cox run -p "create hello.txt containing hi"
./target/debug/cox run -p "summarise the diff" --output-format stream-json | head -n 5
```

`stream-json` prints the *same* `Event` JSON the TUI renders, one object
per line (Claude Code-compatible framing). Exit codes are scriptable:
`0` ok · `1` error · `2` denied · `3` budget · `4` interrupted.

Approval from a script: a `Write`/`Exec` call the policy would ask about
is denied instead under the headless default (`--approve never`), with
the reason in the tool result so the model can try another approach:

```bash
./target/debug/cox run -p "commit this" --approve never; echo "exit=$?"
# exit=2 when a call was denied
```

Answer an interactive approval in the TUI with `y` (allow once),
`s` (allow for this session), `n` (deny), `e` (edit the call's input).
Programmatically that is `Submission::Approve { call_id, decision }`:

```rust
use cox_protocol::types::{Decision, Submission};
// `call_id` is the pending call from the `ApprovalRequired` event.
// Allow the pending call the engine escalated:
let answer = Submission::Approve { call_id, decision: Decision::Allow };
// …or let this tool+subject-prefix through for the rest of the session:
let answer = Submission::Approve { call_id, decision: Decision::AllowForSession };
```

## Example 3: permissions — deny beats allow

Every tool call carries a `risk` (`ReadOnly` | `Write` | `Exec` |
`Destructive`) and a `subject` (the confined path, command line, URL, or
`mcp__<server>__<tool>` name). `cox_core::permission::Engine` decides
each call, in order: `deny` rules → `bypass`/`plan` modes → `allow`
rules → `ask` rules → session grants → risk default → approval policy
(`plan.md` §1.8). Adding a `deny` rule can never turn a `Deny` into
anything else.

```toml
# ~/.cox/config.toml
[permissions]
allow = ["Bash(cargo test:*)"]   # this command runs without asking
ask   = ["Bash(git commit:*)"]   # this one always asks
deny  = ["Read(~/.ssh/**)"]      # this one never runs — even with an allow rule
```

```rust
use std::path::Path;
use cox_core::permission::{Engine, Outcome};
use cox_protocol::{CallId, config::PermissionsConfig, types::*};
use serde_json::json;

let home = Path::new("/home/alice");
let engine = Engine::compile(&PermissionsConfig::default(), Some(home), Path::new("/repo"))
    .expect("default rules compile");
let ssh = ToolCall {
    id: CallId::new(), name: "read".into(),
    input: json!({"path": "/home/alice/.ssh/id_ed25519"}),
    risk: Risk::ReadOnly, subject: "/home/alice/.ssh/id_ed25519".into(),
};
// The default config denies this, despite the ReadOnly risk:
assert!(matches!(
    engine.decide(&ssh, PermissionMode::Default, ApprovalPolicy::OnRequest, SandboxMode::WorkspaceWrite, &[]),
    Outcome::Deny { .. }
));
```

(The same snippet runs as a doctest on `Engine::decide`, so `cargo test`
keeps it compiling. A matching doctest on
`cox_protocol::types::Submission` covers the `UserTurn` JSON shape.)

`plan` permission mode (`Tab` in the TUI) denies every non-`ReadOnly`
call without prompting, so the model learns to describe the change
instead of making it.

## Example 4: big output is lossless, not lost

Tools return their **full** output; the core archives it *before* the
model sees anything. The model sees head + tail lines plus a pointer:

```text
line 1
line 2
[… 84 KiB archived; expand #01J9… lines 3–8210]
line 8211
```

Read the rest any time — you see exactly what the model saw, plus more:

```bash
./target/debug/cox expand 01J9…              # full archived output
./target/debug/cox expand 01J9… --lines 60-90
```

Two refinements keep the window small without losing evidence:

- **Dedup:** an identical read-only call within
  `context.dedup_window_turns` (default 8) with no write to its subject
  since returns `"unchanged since #<id>"` instead of the bytes again.
- **Microcompaction:** tool results older than
  `context.microcompact_after_turns` (default 6) become
  `Pointer { archive, summary }` in new requests. The rollout on disk is
  untouched — only what the model is (re)sent shrinks.

## What the model sees: the cache-stable prefix

Every request is laid out so the stable bytes come first and the
volatile bytes last (`plan.md` §1.9). Anthropic caches the stable
prefix; OpenAI-compatible providers get the same order for free via
automatic prefix caching:

```text
system[0]  tool schemas, sorted by name ............ byte-stable ┐
system[1]  cox system prompt (versioned, no date) ... byte-stable │ breakpoint 1
system[2]  AGENTS.md / skills index ................ byte-stable ┘
system[3]  volatile: date, cwd, branch, memory ..... never cached
messages   summary (if compacted) + history ........ breakpoint 2 (end of last turn)
           this turn's messages ................... breakpoint 3 (moves)
```

The rule: touch `system[0..=2]` and you invalidate the cache for every
later call. Discovering a deferred tool via `tool_search` does exactly
that, once — the core emits a `Notice` explaining it. `cox stats --cache`
shows whether the prefix is actually hitting.

## When context runs out: compaction

After a turn, when the last call's context tokens reach
`context.compact_at` (default 75%) of the model's window — or on
`/compact [focus]`, or on a context-length error — the core summarises
every turn but the last `keep_turns` (default 2) with the cheap tier,
appends one `Summary` item, and emits:

```json
{"type":"compacted","summary":"…","dropped":["…"],"before_tokens":150000,"after_tokens":9000}
```

Append-only: the rollout keeps every original line; `dropped` ids are
just skipped when building future requests. Early turns keep their
verbatim text right up until they are summarised.

## Where it lands on disk

Under `~/.cox/` (`COX_HOME` overrides; never touch the real home in
tests — use `COX_HOME=/tmp/cox-scratch`):

```text
~/.cox/
  config.toml                 effective config (see cox config show --sources)
  cox.db                      sessions, per-request usage ledger, archive index, memory FTS
  sessions/<ulid>.jsonl       the rollout: one Event per line, resume + replay source
  archive/<ulid>              tool outputs over 16 KiB (smaller ones inline in the db)
  logs/cox.log                tracing log
```

Every provider call writes one `usage` row (model, input/output, cache
read/write, cost). `cox stats --day`, `cox stats --month`, and
`cox stats --cache` read the ledger; session/monthly caps in
`[budget]` stop the turn with `TurnDone{Budget}` instead of a surprise.

## The four surfaces (one stream each)

| Surface | Command | What it does with `Event`s |
|---|---|---|
| TUI | `cox [PROMPT]` | renders transcript, diffs, approval modal, status line |
| Headless | `cox run -p … --output-format text\|json\|stream-json` | prints the stream for scripts |
| Editor | `cox acp` | maps `Event` → ACP `session/update` (see `docs/ide.md`) |
| Other agents | `cox mcp [--allow-write] [--tools a,b]` | serves built-in tools, not the loop (see `docs/compat.md`) |

Useful companions: `cox sessions --grep <q>` (find a rollout),
`cox doctor` (keys, sandbox, stale price rows), `cox config show
--sources` (which file each key came from), `cox ext list` (which
instruction files, skills, commands, agents, hooks, MCP servers are in
effect).

## Trust boundaries in one paragraph

Model output, tool results, MCP responses, hook stdout, skill files, and
repository instruction files are all untrusted. Four guards from
`AGENTS.md` cover them, and this doc's examples each touched one: the
**permission engine** authorises every call (Example 3); **path
confinement** (`cox_tools::path::confine`) rejects workspace escapes
before a file tool runs; the **sandbox** confines shell commands unless
the session chose `danger-full-access`; terminal **sanitisation**
strips escape sequences and bidi overrides before anything the model or
a tool wrote is displayed. A broken hook, skill, or MCP server is a
warning and an absence, never a fatal error.
