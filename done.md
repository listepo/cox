

#### T30.8 Tests for the eval package

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: T30.7 · Size: ~150 · Priority: P1 · Complexity: 2
Goal: `cox_evals` has pytest tests that fail if the harness breaks, run by `just test-evals` with no network and no key.
Files: `evals/tests/test_harness.py`, `evals/tests/test_tbench.py`, `justfile`.
Plan: a fake `cox` (a script printing a canned `cox run` JSON payload) drives `run_task`/`main` with no binary and no key; tests: task loading and `--only` matching; the scripted scenario TOML round-trips through `tomllib` into the shape `Scripted` reads; the verify preset writes `AGENTS.md` and a `PostToolUse` hook config; result/token accounting from a `cox run` JSON payload (tokens, cost, exit code 2 → fail); an end-to-end dry run of one task against the built `cox` binary (skipped when none is built); the tbench adapter's self-test as a test.
Check:
```bash
just test-evals
```
Done when: the suite is green and each listed behaviour has a test that fails when it breaks.
What landed (`1ae2f87`): `evals/tests/conftest.py` (a fake `cox` shell script that prints a canned `cox run` payload, exits with a chosen code and records its argv; a `real_cox` fixture that skips when no binary is built), `test_harness.py` (17 tests: task loading and `--only`, scenario TOML round-trip incl. quotes/newlines, every task's dry-run TOML parses, verify preset files, pass/exit-2/failed-check/failed-setup/unparseable rows, hermetic flags, `--preset verify` re-enabling hooks, `main` totals and exit code, `token_line`, `find_cox_bin` precedence, a real-binary dry run), `test_tbench.py` (7 tests: pane JSON parsing, missing binary, missing key, command quoting and token mapping, non-zero payload exit, no JSON, the self-test without any key). `just test-evals` runs them.
Deviations: `conftest.py` is a fourth file, for the fixtures both test modules share. Mutation check: dropping token parsing fails 3 tests, always passing `--no-hooks` fails 1, dropping the final scripted turn fails 2.
Check:
```text
$ just test-evals
24 passed in 1.35s
```

#### T30.10 Generated Anthropic wire types (typify)

Model: claude-sonnet-5 · Status: done 2026-09-25 · Depends: - · Size: ~200 (+ schema data) · Priority: P2 · Complexity: 3
Goal: the Anthropic stream frames are deserialized into Rust types generated from a JSON Schema instead of `serde_json::Value` `.get()` chains, so a field rename or a new required field is a compile-time or schema diff, not a silent `None`. Only the types are generated; the SSE → `ProviderEvent` state machine stays hand-written (D3: own thin provider layer, no SDK). Why not a whole SDK: no Rust generator handles Anthropic's spec end to end (SSE streaming, discriminated unions, beta headers); typify (oxidecomputer, used by progenitor) is the one maintained JSON Schema → serde types generator.
Plan:
1. `crates/cox-provider/schema/anthropic-stream.json` (new): a curated JSON Schema subset — the stream events (`message_start`, `content_block_start`/`delta`/`stop`, `message_delta`, `message_stop`, `ping`, `error`), content blocks (`text`, `thinking`, `redacted_thinking`, `tool_use`), deltas (`text_delta`, `input_json_delta`, `thinking_delta`, `signature_delta`) and `usage`. Field names and shapes copied from Anthropic's published OpenAPI spec (source URL in a `$comment`); only the fields cox reads are required, everything else optional, so an additive API change never breaks parsing.
2. `crates/cox-provider/src/anthropic/wire.rs` (new): `typify::import_types!` over that file, nothing hand-written but the `//!` header.
3. `stream.rs`: keep the dispatch on the frame's `type` string (unknown events still ignored); each known event body deserializes into its generated type; unknown block/delta types are still ignored exactly as today. Error mapping to `ProviderError::Parse { line }` unchanged.
4. Deps: `typify` in `[workspace.dependencies]` and `cox-provider`; rows in plan §1.1, `toolchain.md` and the workspace `rust.md`.
Files: `Cargo.toml`, `crates/cox-provider/Cargo.toml`, the schema, `wire.rs`, `stream.rs`, `anthropic/mod.rs` (one `mod` line). Deviation: 6 files, because the macro needs a manifest entry and a data file; the logic change is only in `stream.rs`.
Check:
```bash
mise exec -- cargo nextest run -p cox-provider
```
Plus the three workspace commands. Every Anthropic fixture (including `live_tool_use.sse`) replays with unchanged snapshots; a new test proves a frame with an extra unknown field and an unknown block type still parses.
Done when: `stream.rs` has no `Value::get` chains for the known events, all snapshots are byte-identical, and the dependency is recorded.
Out of scope: request types (the request builder stays as is), OpenAI providers, fetching the spec at build time.
What landed (`89ab1ab`): `schema/anthropic-stream.json` (draft-07, 105 lines; events, content block, delta, usage, stop details, error; `additionalProperties` left open so typify emits no `deny_unknown_fields`); `anthropic/wire.rs` (`import_types!` + 19 tests on the generated types: every known event from real fixture frames, unknown fields on event/block/usage, missing and null usage fields → `None`, missing `message`/`content_block`/`delta` fails, unknown block and delta `type` still deserialize, and a schema sanity test that every `required` entry exists in `properties`); `stream.rs` deserializes each known event with `serde_json::from_value::<wire::*Event>`, dispatch on `type` unchanged; new test `unknown_fields_and_block_types_are_ignored`. All Anthropic snapshots byte-identical.
Deviations: 7 files and ~530 LOC including the schema and the wire tests (the creator asked for tests on the generated code). Block and delta `type` are a plain string, not a tagged enum, so unknown types parse at the wire layer and `stream.rs` still ignores them by hand; `tool_use.id` is not `required` because one `ContentBlock` type covers text and thinking too. typify 0.8 pulls schemars 0.8 as a build-time proc-macro dependency beside the workspace's schemars 1. Mutation checks: a bogus `required` entry fails `schema_required_fields_exist_in_properties`; dropping `required: ["content_block"]` breaks compilation.
Check:
```text
$ mise exec -- cargo nextest run -p cox-provider
Summary [ 3.284s] 120 tests run: 120 passed, 0 skipped
$ mise exec -- cargo nextest run --workspace
Summary [ 48.530s] 886 tests run: 886 passed, 3 skipped
clippy -D warnings: clean; fmt --check: clean
```

#### T30.11 OpenAI Responses wire types from async-openai

Model: claude-sonnet-5 · Status: done 2026-09-25 · Depends: - · Size: ~200 · Priority: P1 · Complexity: 3
Goal: `openai/responses.rs` (the Codex-replacement backend) builds requests and parses stream events with `async-openai`'s Responses types instead of hand-written structs and `Value` walks (A40 step 1). Transport, retry, SSE framing, the `ProviderEvent` mapping and usage/ledger stay ours.
Plan: step 0 is a gate: add `async-openai` with `default-features = false` and only the Responses types feature; run `cargo tree -p cox-provider -e normal` and confirm it pulls no second HTTP stack (reqwest/hyper/tokio versions other than ours). If it does, stop and ask the creator (fallback: typify over a vendored `openai-openapi` subset, as in T30.12). Then: a `openai/wire.rs` module re-exporting the used types; `responses.rs` builds the request with them (raw-JSON extras only where the type lacks a field cox sends) and deserializes each stream event by its `type` into them, unknown events ignored; tests for unknown events/fields; snapshots unchanged.
Check:
```bash
mise exec -- cargo nextest run -p cox-provider
```
Done when: every OpenAI Responses fixture replays with byte-identical snapshots, unknown events and fields are still ignored (test), and `async-openai` has rows in plan §1.1, `toolchain.md` and `rust.md`.
Out of scope: OpenAI Chat (`chat.rs`), ChatGPT-account login.
What landed (`a04fe69`): `async-openai` 0.42 with `default-features = false, features = ["response-types"]`; `openai/wire.rs` (new) re-exports the request, item, tool and per-event payload types, with 7 tests on real `fixtures/openai-responses/*.sse` frames; `responses.rs` builds a typed `CreateResponse` and deserializes each known event into its payload struct; unknown event types still fall through, unknown fields are ignored (new test `responses_stream_unknown_field_on_known_event_is_ignored`). All 6 Responses snapshots byte-identical.
Deviations: the SDK's `ResponseStreamEvent` enum is not used — it has no catch-all variant, so an unknown `type` would be an error; `Response`/`ResponseCompletedEvent` are not used either (non-optional fields cox never reads). Typed structs serialize in declaration order, so a small `reorder_body` step restores the pinned key order in three places. Gate: `cargo tree` shows async-openai pulls only `derive_builder`, `serde`, `serde_json`; one reqwest/hyper/tokio each.
Check:
```text
$ mise exec -- cargo nextest run -p cox-provider   # on HEAD db85e5f, after T30.12
Summary [ 19.923s] 128 tests run: 128 passed, 0 skipped
workspace (agent run): 894 passed, 3 skipped; clippy -D warnings clean; fmt --check clean
```

#### T30.12 Anthropic wire types generated from the vendored OpenAPI spec

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: T30.10 · Size: ~300 (+ vendored spec) · Priority: P1 · Complexity: 4
Goal: the Anthropic backend (the Claude Code replacement) gets request *and* stream types generated with typify from Anthropic's own OpenAPI 3.1 spec, vendored in the repo, replacing T30.10's hand-curated schema subset (A40 step 2).
Plan: vendor the last published Stainless snapshot (URL in R§4.3.1; record its sha256 beside it) under `crates/cox-provider/schema/`; a small extraction script turns `components.schemas` reachable from the Messages request and `MessageStreamEvent` into one JSON Schema (`$defs`, refs rewritten) that `typify::import_types!` consumes; the extracted file is committed and a test fails when it is stale against the vendored spec; `request.rs` serializes through the generated request types where they express what cox sends (`cache_control`, thinking, `effort`, tools) and keeps a raw-JSON escape hatch for anything the snapshot lacks.
Check:
```bash
mise exec -- cargo nextest run -p cox-provider
```
Done when: all Anthropic request and stream snapshots are byte-identical, the T30.10 wire tests pass against the generated types, and re-vendoring is one documented command.
Out of scope: subscription OAuth (forbidden, R§4.3.1), Bedrock/Vertex.
What landed (`2e02798`): `schema/anthropic-openapi.json` (the Stainless snapshot, 3.5 MB, 1318 component schemas, sha256 `1bb7c7a0…1ad2`, provenance and re-vendor command in `schema/README.md`); `build.rs` collects the 222 schemas reachable from `CreateMessageParams`, `MessageStreamEvent` and `ErrorResponse`, preprocesses them and runs typify's `TypeSpace` into `$OUT_DIR/anthropic_wire.rs` (283 types), which `wire.rs` includes; `request.rs` serializes a typed `CreateMessageParams`; `stream.rs` parses typed events. `schema/anthropic-stream.json` deleted. All 8 Anthropic request and stream snapshots byte-identical.
Deviations: typify moved to `[build-dependencies]`, plus `schemars 0.8` (typify's `TypeSpace` API takes it). Nine preprocessing rewrites in `build.rs`, each with a why-comment: `Model` → string; tool `InputSchema` → raw JSON; strip `title`, `pattern`/length, `format`; response side drops string enums and narrows `required` so a new stop reason or service tier cannot fail a frame; hoist union-member property types and inline `$ref` variants so typify emits tagged enums. Block and delta enums are closed, so `stream.rs` peeks `type` and skips unknown kinds before the typed parse (the wire test for unknown types now asserts rejection at the wire layer; tolerance is proven in `stream.rs`). Raw JSON kept for `fallbacks`, a non-object tool input, image media types outside the spec, and `cache_control` placement (it also lands on thinking blocks, which the spec does not model). Behaviour now stricter: `message_delta` without `usage` and `message_start` without `message.model` are parse errors (the spec requires both). Not run against a `COX_HOME` scratch tree: the change is wire serialization only, covered by the snapshots.
Check:
```text
$ mise exec -- cargo nextest run -p cox-provider   # on HEAD db85e5f
Summary [ 19.923s] 128 tests run: 128 passed, 0 skipped
workspace (agent run): 894 passed, 3 skipped; clippy -D warnings clean; fmt --check clean
```

#### T30.9 Terminal-Bench run through Harbor

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: T30.7 · Size: ~150 · Priority: P1 · Complexity: 3
Goal: one real Terminal-Bench 2.x subset run with cox, inside a $1 budget the creator set, on colima (the creator's choice of Docker runtime). The current adapter never worked for real: it runs `cox` inside the task container, where nothing installs it and no key is passed, and it targets `terminal-bench` 0.2.x while TB 2.0 runs through Harbor.
Plan:
1. Read Harbor's custom-agent contract and the TB 2.0 dataset (image arch, task list, how an agent is installed into the task container) from primary sources; record them with URLs in R§5.3.
2. Build a static Linux `cox` for the task containers' arch (musl target; `cargo zigbuild` with mise's zig if a cross toolchain is needed — a new tool gets a `toolchain.md` row).
3. Rewrite `cox_evals.tbench` as a Harbor agent: upload the binary into the container, run `cox run -p … --output-format json --budget <cap> --max-turns <cap>` there with `ANTHROPIC_API_KEY` passed from the host env, read usage and cost from the JSON; update `evals/tests/test_tbench.py` to the new contract.
4. Start colima with a capped disk; check free host disk before and after pulling images; pick 3–5 small tasks so the whole run stays under $1 (per-task `--budget`).
5. Run it, record pass rate, tokens and ledger cost in R§5.3 with sources, stop colima.
Done when: `research.md` §5.3 has the TB subset's pass rate, tokens and ledger cost; T30.3 then closes.
What landed: `b3d1791` (Harbor agent `evals/src/cox_evals/tbench.py` replacing the terminal-bench 0.2.x adapter, its tests, the `tbench` extra with `harbor>=0.23.0`, zig and cargo-zigbuild in `mise.toml`, `toolchain.md` rows) and `db85e5f` (cox runs with `--permission-mode bypass --sandbox danger-full-access` inside the task container; the jobs dir goes under `$HOME`). Result recorded in `research.md` §5.3: 3/3 tasks (`fix-git`, `cobol-modernization`, `prove-plus-comm`) with `claude-sonnet-5`, 541 612 prompt tokens (516 666 cache read), 16 946 output, $0.3351 ledger cost, 2 min 56 s.
Deviations: glibc 2.31 target (`aarch64-unknown-linux-gnu.2.31`) instead of musl — cargo-zigbuild pins the glibc floor, which the TB images meet, without a musl C toolchain. A first live attempt spent about $0.51 plus at most $0.25 and scored nothing (denied tool calls under `--approve never`; reward files lost because colima shares only `$HOME`); both causes fixed in `db85e5f` and recorded in §5.3. Total spend ≈ $1.1 against the $2 the creator allowed. Docker's `compose`/`buildx` plugins come from mise (the Docker.app symlinks were broken); colima is stopped.
Check:
```text
$ harbor run -d terminal-bench@2.0 -a cox_evals.tbench:CoxAgent -m anthropic/claude-sonnet-5 --force-build -n 2 \
    -i fix-git -i cobol-modernization -i prove-plus-comm --ak budget_usd=0.25 -o ~/.cache/cox-evals/tb-jobs
job 2026-09-25__21-41-22: 3 trials, 0 errors, mean reward 1.0, cost $0.3351
$ just test-evals
27 passed in 2.24s
```

#### T30.3 Eval run with a verification step

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: a funded API key · Size: ~100 · Priority: P2 · Complexity: 2
Goal: the T12.1 harness gains a "verify before done" instruction and a `PostToolUse` test-runner hook preset; one Terminal-Bench 2.x run is recorded with its ledger cost.
Files: `evals/run.py`, `evals/hooks/verify.sh` (new), `research.md`.
Steps: (1) Harness system addendum: "before reporting done, run the task's tests and show the output"; (2) hook preset: after `edit`/`apply_patch`/`write` run the project's test command when one is detected (`just test`, `cargo nextest`, `npm test`, `pytest`) with a 120 s cap and feed failures back as `additionalContext`; (3) run the 10 in-repo tasks with and without the preset, then one TB 2.x run; record pass rate, cost, and tokens in `research.md` §5.3.
Check:
```bash
uv run --project evals cox-evals --provider anthropic --model claude-sonnet-5 --preset verify
```
Done when: §5.3 has the table with both configurations and the run's cost from `cox stats`.
Out of scope: leaderboard submission.
Progress: steps (1)–(2) landed in `defba68` (`--preset verify` in `evals/run.py`, `evals/hooks/verify.sh`), verified offline only. Step (3): the 10 in-repo tasks ran live with and without the preset (`4cd7bb6`, `research.md` §5.3: 9/10 both, $0.0515 vs $0.0626; `evals/run.py` now prints tokens). Left: the one Terminal-Bench 2.x run.
What landed: `defba68` (steps 1–2: `--preset verify`, `evals/hooks/verify.sh`), `4cd7bb6` (step 3, in-repo half: 9/10 both configurations, $0.0515 vs $0.0626), and the Terminal-Bench 2.0 run through T30.9 (3/3, $0.3351). `research.md` §5.3 holds both tables.
Deviations: the TB run used the baseline configuration only; the verify preset is wired for the in-repo harness, and all three TB tasks already passed without it. Costs come from the `cox run` payload, which is the ledger row `cox stats` reads.
Check:
```text
$ uv run --project evals cox-evals --provider anthropic --model claude-sonnet-5 --preset verify
9/10 passed  total cost $0.0628
$ harbor run -d terminal-bench@2.0 … (see T30.9)
3/3, mean reward 1.0, $0.3351
```
