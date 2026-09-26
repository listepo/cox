# Design: external agents from plugins, Cursor first (A54, phase P35)

## Problem

Today a plugin can bring in-process code, a model provider, or an MCP
stdio server (`docs/design/plugins.md` PL§7) — none of those fits "drive
another vendor's CLI agent over its own official headless protocol".
Cursor has no chat/completions API (research.md §4.3.8): its Cloud Agents
API only creates durable server-side runs, so it cannot be a `Provider` the
way T30.15 wired LM Studio. Creator decision (A54): Cursor is a plugin that
drives the official `agent` CLI, never the desktop session, never a
reverse-engineered proxy. This is the ≤1-page doc D15 requires before
`[[external_agents]]` or any code lands (T35.1–T35.9).

## 1. Manifest capability

One new `plugin.toml` table, validated like `[[mcp]]` (PL§2):

```toml
[[external_agents]]
name = "cursor"        # fits the tool-name rule after prefixing
command = "agent"      # in-package path, or a PATH program
args = ["acp"]         # the mode's own invocation (T35.13, below)
mode = "acp"           # "acp" | "stream-json"
key_env = "CURSOR_API_KEY"
```

`name` is the `agent(preset: "<name>")` dispatch name (§3). An in-package
`command` is covered by the whole-tree digest (PL§1); a PATH program is
shown verbatim at approval. `key_env` resolves with
`resolve_key(key_env, <section>)` (D12/A49) — never a keychain in tests,
never a hand-typed key. `deny_unknown_fields` applies.

## 2. Spawn, sandbox, grant

Host only — a WASM guest cannot open a process. `cox-plugin` resolves
`command`/`args` the same way T33.19 resolves `[[mcp]]`; `crates/cox` wraps
it with `sandbox::Policy` before spawning, reusing PL§7c's existing wrap
for a plugin's MCP stdio server (T33.42's per-server opt-out applies here
too) — no second sandbox path. The capability is one more line
`grant::check`'s `Verdict` lists in words ("Can run the external agent
'cursor' via 'agent', key from `CURSOR_API_KEY`"); `Granted`/
`NeedsApproval`/`Disabled` exactly as any other capability (PL§3).

**What cox does not judge (T35.13).** The external agent's own tool calls
— its reads, edits and shell commands inside its own process — are not
judged per call by `cox_permission::Engine` or `PreToolUse` hooks: cox
never sees them before they run. The guard is the process sandbox the wrap
above put the CLI under, the same profile `bash` runs in. Only what the
agent asks cox for goes through cox's guards: ACP
`session/request_permission` through the Engine, `fs/*` through
`path::confine` (§4). stream-json `tool_call` lines are shown after the
fact (§5), never decided.

**Host drivers (T35.13, `crates/cox/src/external_agents.rs`).** At session
open, for each granted entry, the host builds one driver behind
`ExternalAgent` and keeps it for the session (`Session::set_external_agents`,
called next to `set_agent_defs`). A bare `command` found on no `PATH`
directory, or a `key_env` that `resolve_key(key_env, <name>)` cannot
resolve, leaves that entry out with one `Notice(Warn)` (§7). Each turn
spawns the wrapped argv — manifest `args` first, so they carry the mode's
own invocation (`["acp"]`, or `["-p", "--output-format", "stream-json"]`),
then, for stream-json only, the prompt as the last argument — with the
child env allowlist (`CHILD_ENV_ALLOWLIST`) plus `key_env` set to the key,
nothing else of cox's environment, and cwd the session's. The CLI leads
its own process group; however the turn ends (done, error, cancel), that
group is SIGKILLed with `cox_tools::bash::kill_group`, the kill a cancelled
`bash` ends with.

## 3. Reaching the model

A granted entry registers as one more name `AgentTool::preset()` resolves
(T34.1), alongside built-ins and discovered `.cox/agents/*.md` definitions
— `agent(preset: "cursor")` dispatches it like a custom `AgentDef`, picking
the ACP (§4) or stream-json (§5) driver from `mode`. A follow-up or a
sibling message addressed to it routes through T34.5's existing
parent-routing — no second messaging path.

## 4. ACP mode

cox is the ACP **client** here, not the server `cox-acp` already is.
**Crate check:** the workspace pins `agent-client-protocol = "2.0"`;
`Cargo.lock` resolves **2.1.0**, and its source
(`~/.cargo/registry/…/agent-client-protocol-2.1.0`) does provide the client
side: `AcpAgent`/`AcpAgentConfig` (`src/acp_agent.rs`) spawn the subprocess
and implement `ConnectTo`; `Client.builder()` registers typed handlers
(`.on_receive_request::<RequestPermissionRequest>()`, `…<WriteTextFileRequest>()`,
…) before `.connect_with(agent, |cx: ConnectionTo<Agent>| …)` — mirroring
the `ConnectionTo<Client>` shape `server.rs` already drives in the other
role (`examples/yolo_one_shot_client.rs` is the reference). No new
dependency: T35.3 adds `crates/cox-acp/src/client.rs` beside `server.rs`.

`session/request_permission` is decided by `cox_permission::Engine::decide`
— the one guard every tool call goes through: its options map onto a
`ToolCall`-shaped decision, `Allow` selects the first "allow" option, `Ask`
surfaces the normal `ApprovalRequired`/relay path (labelled with the
subagent's `Source`, T34.3) and answers with the picked option, `Deny`
selects a reject option or answers `Cancelled`. `fs/read_text_file`/
`fs/write_text_file` pass their path through `path::confine` against the
process's workspace roots before touching disk; `terminal/*` runs only
under the same `sandbox::Policy` already governing the spawned process, or
is refused with the reason named — never a path the external agent picks
for itself.

The host's ACP driver (T35.13) runs one `initialize` → `session/new` →
`session/prompt` per turn over the process's stdio and makes the agent's
`agent_message_chunk` text the turn's one `AssistantMessage`. Its
`ClientHost` gets the writable roots, the session cwd, the `[sandbox]`
policy (`None` under `danger-full-access`), an Engine compiled from the
session's `[permissions]`, its mode and approval policy, and no session
grants. `ExternalAgent::turn` has no route to the session's
`ApprovalRequired` relay yet, so an `Ask` verdict is refused with the reason
named (fail closed) rather than prompted; a rule the Engine allows or denies
is decided as usual.

## 5. stream-json mode

A pure, host-side line mapper, not a WASM export (PL§7a defers a streaming
ABI provider for the same reason: boilerplate parsing, not plugin logic; a
host mapper also lets T35.7 test it with no wasm toolchain). Documented
shapes (research.md §4.3.8):

| stream-json line | cox `Event`/`Item` |
|---|---|
| `system`/`init` | `Notice(Info)` naming model/permissionMode; no transcript item |
| `user` | absorbed — cox already recorded the outbound turn |
| `assistant` | `ItemStarted`/`ItemDone(AssistantMessage { text })` |
| `tool_call`/`started` | `ItemStarted(ToolCall { name, input })` |
| `tool_call`/`completed` | `ToolCallOutput` + `ToolCallDone` + `ItemDone(ToolResult)` |
| `result` (`is_error: false`) | `TurnDone`, then the usage row (§6) |
| `result` (`is_error: true`) | `Error`, then the usage row (§6) |
| unrecognised line | sanitized `Notice(Warn)` of the raw line, never a hard error (D14) |

## 6. Cost

`cox_model_call`'s rule ("every request has a `usage` row") still holds.
Cursor's shapes carry no token counts. Write the row at **$0** with
`billed_externally: true`, `job = plugin:<id>` (PL§7d), and only use
reported tokens if a future event ever carries them — never estimate spend
that lands on the user's own Cursor plan, not cox's ledger.

## 7. Trust, fail-open, doctor

Every rendered line (stream-json text, ACP updates, stderr) passes
`cox_sanitize::sanitize` (D14) — as untrusted as any MCP result. A missing
CLI or unset `key_env` is one `Notice(Warn)` at session open, and the
preset is left out of `agent`'s names, matching
`broken_hook_is_skipped_not_fatal`. `cox doctor` gets one row per granted
entry: CLI found on `PATH` (+ `--version`, best-effort), `key_env` set or
missing, sandboxed or opted out (T33.42) — missing CLI/key is a warning,
never a hard failure.

## 8. Hard rule (creator decision, not a preference)

Only the dashboard-issued key via `key_env`, resolved like any other
provider key, and only `agent -p --output-format stream-json`/`agent acp`.
Never the desktop session, never a reverse-engineered proxy (research.md
§4.3.8 catalogs several).

## Falsifiers

- A Cursor CLI release drops `--output-format stream-json` or `acp` from
  `agent`'s documented surface.
- An ACP permission/`fs`/`terminal` request needs a richer consent shape
  than `cox_permission::Engine` offers — would force a second permission
  path, which PL§10 ("no bypass") forbids without a design amendment.
- A future `agent-client-protocol` major version drops the client-side
  `AcpAgent`/`Client.builder()` API this doc relies on.

Out of scope: the Cloud Agents API as a background-task backend
(`ideas.md`, unapproved — a durable server-side run, not a local
subprocess); a second, agent-specific permission path.
