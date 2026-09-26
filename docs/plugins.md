# Writing a cox plugin in Rust

A cox plugin is a WebAssembly module plus a `plugin.toml` manifest. cox loads it with extism, gives it only the capabilities the user approved, and calls the exports it finds. This guide covers the Rust side: the `cox-plugin-sdk` crate in `plugins/sdk`. The design, including why each rule exists, is `docs/design/plugins.md` (PL); the ABI is its §4.

## Setup

The Rust toolchain and its `wasm32-unknown-unknown` target come from the repository's `mise.toml`. `mise install` adds the target if it is missing.

```toml
# Cargo.toml
[package]
name = "turn-counter"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
# Not on crates.io yet: a path inside this repository, or
# { git = "https://github.com/listepo/cox" } from outside it.
cox-plugin-sdk = { path = "../../sdk" }
```

```bash
mise exec -- cargo build --release --target wasm32-unknown-unknown
# → target/wasm32-unknown-unknown/release/turn_counter.wasm
```

Copy the `.wasm` next to `plugin.toml` under the name its `wasm` key gives. The manifest's schema is `docs/plugin.schema.json`; PL§2 shows every key.

## Exports

A plugin is a set of handler functions and one `register!` call. `init` is required and comes first. Every other export is optional and exists only if you list it, because cox probes exports by name and treats a missing one as "not provided".

```rust
use cox_plugin_sdk::*;

fn init(_: InitIn) -> Result<InitOut, SdkError> {
    Ok(InitOut {
        commands: vec![CommandDecl { name: "reset".into(), description: "Reset the counter".into() }],
        subscribe: vec!["turn_started".into()],
        ..InitOut::default()
    })
}

fn on_event(batch: EventBatch) -> Result<Effects, SdkError> {
    let turns = kv_get("turns")?.and_then(|v| v.as_u64()).unwrap_or(0);
    kv_put("turns", &Value::from(turns + batch.events.len() as u64))?;
    Ok(Effects { redraw: true, ..Effects::default() })
}

fn command(_: CommandIn) -> Result<CommandOut, SdkError> {
    kv_delete("turns")?;
    Ok(CommandOut::Nothing)
}

cox_plugin_sdk::register!(init => init, on_event => on_event, command => command);
```

The complete version is `plugins/examples/rust`, the reference plugin PL§13 asks every language to implement: it adds a failure-counting hook and a status segment, and keeps its counters in memory as well as in kv, because `cox_render` may not call `kv_get`.

A handler returns `Result<output, E>` for any error type that implements `Display`. An `Err` fails that one call: cox shows the message as a warning and carries on (extensions fail open).

| `register!` key | Export | Input → output | Called |
| --- | --- | --- | --- |
| `init` | `cox_init` | `InitIn` → `InitOut` | once per session; `InitIn.config` is your `[plugins.<id>]` table from cox's config (`{}` when absent), which you validate yourself; contributions you were not granted are dropped with a notice |
| `on_event` | `cox_on_event` | `EventBatch` → `Effects` | for the events in `InitOut.subscribe` (PL§5) |
| `hook` | `cox_hook` | `HookCall` → `HookOutcome` as `Value` | for the hooks in `capabilities.hooks` (PL§6) |
| `decide` | `cox_decide` | `Question` → `Advice` | at the decision points in `capabilities.decide` (PL§6b) |
| `tool_subject`, `tool_risk`, `tool_call` | `cox_tool_*` | `ToolCallIn` → `String`, `Risk` / `ToolOutput` as `Value` | for the tools in `InitOut.tools` (PL§7) |
| `provider_stream` | `cox_provider_stream` | `ProviderCall` → `Vec<ProviderEvent>` as `Vec<Value>` | for an `api = "plugin"` provider (PL§7a) |
| `command`, `key` | `cox_command`, `cox_key` | `CommandIn` → `CommandOut` | `/<id>:<name>` and the plugin's keys (PL§8) |
| `render` | `cox_render` | `RenderIn` → `Widget` | for a status segment, panel or overlay (PL§8) |
| `render_item` | `cox_render_item` | `RenderItemIn` → `Option<Widget>` | for the targets in `ui.render` (PL§8) |
| `shutdown` | `cox_shutdown` | `()` → `()` | at session end, best effort |

Payloads that carry a cox protocol type (`Event`, `HookOutcome`, `ToolOutput`, `ProviderEvent`, `Request`) are `serde_json::Value`, because the SDK does not depend on the host crates. Their JSON shape is in `docs/protocol.jsonschema`; everything else is in `docs/plugin-abi.schema.json`.

## Host functions

Each wrapper returns `SdkResult<T>`. `SdkError::Host(AbiError)` means cox refused or failed the call: the capability is not granted, the call is not allowed from the current export, the permission engine or budget said no, a cap was hit, or the deadline passed. Refusals are values, not traps, so a plugin can degrade instead of failing its whole call.

| Function | Capability | Not allowed from | Returns |
| --- | --- | --- | --- |
| `log(level, text)` | — | — | `()`; goes to cox's log, rate-limited |
| `notify(level, text)` | — | `render` | `()`; a transcript notice, level at most `Warn`, sanitized |
| `kv_get(key)`, `kv_put(key, value)`, `kv_delete(key)` | `kv` | `render` | `Option<Value>` / `()`; within the store quota (64 KiB per value, 1 MiB per plugin, keys up to 256 bytes) |
| `context()` | `context` | — | the event-folded session snapshot: session id, cwd, tier and model, usage totals, the last 50 items with their text, the todo list, compactions; secret-shaped text redacted |
| `invoke_tool(name, input)` | `invoke` lists `name` | everything but `on_event`, `command`, `key`, `tool_call` | a `ToolOutput`; the call passes hooks, the permission engine and the sandbox like a model's call |
| `model_call(&ModelCall)` | `model` | `render` | `Vec<ProviderEvent>`; at or below the granted tier, budget-gated, recorded in the cost ledger |
| `http(&HttpReq)` | `net` lists the host | `render` | `HttpResp`; a `[[provider]]` host only from `provider_stream`, with cox adding the auth header |
| `output(line)`, `cancelled()` | — | everything but `tool_call` | `()` / `bool`: tool progress and cancellation |
| `redraw()` | any `ui` | — | `()`; marks your slots dirty |

`invoke_tool` is refused inside `hook`, `decide` and `provider_stream` because the agent loop is waiting on those calls, so a tool run from inside them would deadlock.

## The wire, for other languages

The SDK is a thin layer over one rule, which a guest in any language can follow with its own extism PDK:

- An export reads its input as one JSON value (an empty input means `null`) and sets one JSON value as its output. A failure sets the extism error text and returns 1.
- A host function lives in the import module `cox:host/v1` and has the signature `(u64) -> u64`: it takes the offset of an extism memory block holding one JSON value and returns the offset of a block it allocated. The argument is the function's only argument, an object keyed by its argument names when it has several (`{"level": "warn", "text": "…"}`, `{"key": "turns", "value": 3}`, `{"name": "read", "input": {…}}`), or `null` when it has none. The reply is `{"Ok": value}` or `{"Err": AbiError}`.
