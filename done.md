

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

#### T30.14 Eval matrix: agents × providers × models on Terminal-Bench

Status: done 2026-09-25 · Depends: T30.9 · Size: ~250
Goal: one tested module in the `evals` package, not a one-off script, that runs any registered agent against any registered provider and model on a Terminal-Bench 2.0 task list through Harbor, and turns the job results into one per-task table. T30.13 is its first user; the repeat after the refactoring (roadmap) is the second.
Plan:
1. `evals/src/cox_evals/matrix.py`: registries in code (no config file to schema): `PROVIDERS` (LM Studio, Ollama, Anthropic, OpenAI; each with its host URL, the URL a task container uses, the API shapes it serves — OpenAI Chat, Anthropic Messages — its key env or a dummy key for local servers, and a preflight: server up, model listed); `AGENTS` (cox, Harbor's `claude-code`, `terminus-2`; each states the API shape it needs and builds its `harbor run` argv and env for a provider and model); `PRESETS` (T30.13's 12 tasks, three agents, `lmstudio` + `prism-ml/bonsai-27b`). `harbor_argv`, a sequential runner (`subprocess`, one job per agent × model), `summarize(job_dir)` over Harbor's `result.json` files, a table printer, `main` with `--preset/--agents/--provider/--model/--tasks/--dry-run/--jobs-dir`; console script `cox-bench`.
2. `cox_evals.tbench.CoxAgent`: `provider`/`base_url`/`api` kwargs that write the provider section into the container's fresh `COX_HOME/config.toml`, and a dummy key for local servers. cox reaches LM Studio through its Anthropic Messages endpoint: cox's OpenAI Chat path still drops every tool call (ideas.md), so a chat-shape run would measure that bug, not the agent.
3. `evals/tests/test_matrix.py`: argv and env per agent × provider, shape mismatch is an error before anything runs, container URL rewriting, preset contents, `summarize` over a fake job tree, `--dry-run` prints and runs nothing.
Check: `just test-evals` green; `cox-bench --preset t30.13 --dry-run` prints three `harbor run` commands.
Done when: the module and its tests land and the dry run matches the T30.13 card.
What landed: `evals/src/cox_evals/matrix.py` (console script `cox-bench`): registries `PROVIDERS` (lmstudio, ollama, anthropic, openai; host URL, container URL via `host.lima.internal`, API shapes, key env, prepare commands), `AGENTS` (cox, `claude-code`, `terminus-2`; shape preference and argv/env builder), `PRESETS` (`t30.13`); shape negotiation, preflight against the server's `/v1/models`, sequential `harbor run` per agent, `summarize` over trial `result.json`, a markdown per-task table. `CoxAgent` gained `base_url`/`context_window` kwargs that upload a `[providers.*]` section into the container's `COX_HOME/config.toml`. Keys stay in the process env; a local server gets the dummy key `local`.
Deviations: five files (matrix, its tests, `tbench.py`, `test_tbench.py`, `pyproject.toml`) plus `toolchain.md`. Live checks beyond the card: LM Studio serves `prism-ml/bonsai-27b` with tool calls on both `/v1/messages` and `/v1/chat/completions`, and the host cox (`--provider anthropic`, `base_url = "http://localhost:1234"`) finished a one-tool task at $0 — so cox's Messages path works against LM Studio. Not checked: reaching `host.lima.internal` from a task container (T30.13 step 2).
Check:
```text
$ just test-evals
39 passed in 3.13s
$ cox-bench --preset t30.13 --dry-run
3 harbor run commands (cox via anthropic/… + base_url=http://host.lima.internal:1234; claude-code with ANTHROPIC_BASE_URL; terminus-2 via openai/… + api_base=http://localhost:1234/v1), 12 tasks each
```

#### T30.17 One model of providers, models, prices and effort (design)

Depends: — · Size: design only (≤ 1-page doc + research + follow-up cards)
Goal: everything about providers, models, prices, reasoning effort, context windows, capabilities, keys and endpoints is represented once and handled by one code path per concern, so a new provider (LM Studio, T30.15) or a new model is an entry, not code. The creator's rule: this area must be as unified as possible. Per D15 the first step is the design, not code.
Plan:
1. Map the current state (config sections, provider constructors, key resolution, retries, model metadata, price tables, effort mapping, usage accounting, model-id resolution) with file:line and every divergence; record it in R§4.3.3.
2. Find what can be decomposed and where the architecture improves: one provider descriptor (endpoint, API shape, auth, retry policy), one model catalog (context window, max output, efforts, capabilities, prices) with one lookup, one effort type mapped per wire in one place, one usage/cost path; which crate owns each (D2/D3 boundaries).
3. Extend `docs/design/providers.md` (today: the two-type registry, Type 1 native / Type 2 compatible, prices in `prices.toml`) rather than start a new doc: what that design left split, the target shape, what moves where, what is deleted, migration order; the added part stays ≤ 1 page.
4. Propose the implementation as new cards (≤ 200 LOC / ≤ 3 files each) in a §6 amendment for the creator to approve; mark which existing cards (T30.15, T30.16) should wait for them.
Check: R§4.3.3 exists and `docs/design/providers.md` has the unification section; every divergence in R§4.3.3 is either addressed by a proposed card or explicitly kept with a reason.
Done when: the creator has the design and the proposed cards.
Out of scope: code changes.
What landed:
- R§4.3.3: the current state, file:line, with every divergence:
  - key paths: `providers.anthropic.api_key_env` is never read;
  - retry and timeout configurable for two of five families;
  - three context and capability sources;
  - ad hoc effort mapping.
- `docs/design/providers.md` § Target shape:
  - one `Transport` descriptor;
  - one key resolver;
  - a pure `cox-models` catalog;
  - one effort map;
  - a doctor sync row;
  - what is kept and why;
  - cards U1–U7.
- Amendment A46 (proposed): T30.15 waits for U1–U3 and T30.16 for U4–U5.

Deviations:
- Mid-task the creator set the vendored-data rule (A48). U4's embedded catalog rows are therefore generated by T30.20's script, not copied.
- U1–U7 stay proposed until the creator approves A46.

Check:
```text
R§4.3.3 table: 10 concerns, each divergence addressed by U1–U7 or kept with a reason in providers.md § Target shape (ModelId string, code-tier flags, ProviderId::Local, cache_write = 0)
```

#### T30.18 Split the workspace into as many crates as pay off (design)

Depends: — · Size: design only (research + doc + follow-up cards)
Goal: the creator's rule that the project be split into crates as far as possible. D1 fixed "ten in-tree crates"; this card measures what else can stand alone — per-wire providers, the sandbox, tool groups, core pieces (permission engine, compaction, router, hooks, budget), store, extension loaders, TUI helpers, config loading — and proposes the split. T30.17's provider/model catalog is one of the resulting crates.
Plan:
1. Measure every crate: LOC per module, internal `use crate::…` graph, external deps per module; find leaf modules and cycles that block extraction; record in R§4.3.4 with file:line.
2. For each candidate: what moves, its deps and dependants, what it gains (parallel/incremental builds, heavy deps behind their own crate, reuse from `packages/`, narrower tests) and what it costs; keep the four trust-boundary guards single (AGENTS.md) and the core pure (D2).
3. `docs/design/crates.md` (≤ 1 page): the target crate graph, what is not split and why, migration order that keeps every step green.
4. Proposed cards (≤ 200 LOC moved or ≤ 3 crates touched each) and a D1 amendment in §6 for the creator to approve.
Check: R§4.3.4 and `docs/design/crates.md` exist; every current crate is either split by a proposed card or kept with a reason.
Done when: the creator has the target graph and the proposed cards.
Out of scope: moving code before the creator approves.
What landed:
- R§4.3.4: LOC and heavy dependencies per crate, the internal graph, the `session` ↔ `turn` cycle, leaf modules, the `plain.rs` → `cox_tui::text::sanitize` coupling, and the `deps.rs` rules.
- `docs/design/crates.md`:
  - the four-reason rule for when a module becomes a crate;
  - the target graph: 17 new crates, 27 in total;
  - what is not split and why;
  - the migration order;
  - the falsifier.
- The creator approved it ("create the tasks for crates.md"). D1 is reworded (A47), and cards T32.1–T32.16 are in the new phase P32.

Deviations:
- The build-time gain is not measured yet: T32.2 records `cargo build --timings` before and after.
- `cox-models` is A46 U4, not a P32 card.

Check:
```text
every current crate is split by a P32 card or kept with a reason in crates.md "Not split": cox-core loop, cox-store, cox-ext, cox-mcp, cox-acp, cox-tui TEA core, cox commands
```

#### T30.19 Vendored data through saved scripts: the Anthropic spec

Depends: — · Size: ~150 Python + tests
Goal: the creator's rule (A48). A file no package manager fetches is produced only by a saved, tested Python script that is re-run to update it. The first such file is `crates/cox-provider/schema/anthropic-openapi.json`, which today is re-vendored with a hand `curl` from its README.
Plan:
1. A uv-managed package `scripts/vendor` (`cox_vendor`, console script `cox-vendor`, Python pinned like `evals`) with a registry of vendored files. It is the one entry point for any later data file.
2. `cox-vendor anthropic-spec`:
   - downloads the snapshot URL and checks that the result parses as JSON with an `openapi` key;
   - writes the file;
   - rewrites the README's "Downloaded" and "sha256" rows.

   The README's `curl` line becomes this command.
3. Tests with no network: a stubbed download, one idempotence check (the same bytes give no diff), and one rejection of a non-JSON body.
4. `just vendor` recipe; `toolchain.md` rows for the package and anything it pulls.

Check: the package's tests pass. Running `cox-vendor anthropic-spec` leaves `git status` clean (same snapshot). `cargo nextest` is green.
Out of scope: changing the snapshot URL.
What landed:
- `scripts/vendor/`: a uv package (`cox-vendor`, Python 3.14 as in `evals`) that uses only the stdlib.
  - `registry.py` maps command names to vendored files. T30.20 adds `models` there.
  - `anthropic_spec.py` downloads `SNAPSHOT_URL` and rejects non-JSON or a body without `openapi`. It writes the spec and the README's "Downloaded"/"sha256" rows only when the bytes change.
  - `--check` writes nothing and exits 1 on a diff.
- `just vendor` and `just vendor-test` recipes.
- Docs:
  - `scripts/vendor/README.md` covers what the package is, how to add a file and the commands.
  - `crates/cox-provider/schema/README.md` now gives the command instead of the hand `curl`.
  - `toolchain.md` has the new rows.

Deviations:
- The build backend is `uv_build`, as in `evals`, not hatchling.
- The card's `cargo nextest` step was not run for this task. It touches no Rust, and T30.21's Rust run covers the same tree.

Check:
```text
$ uv run --project scripts/vendor pytest scripts/vendor/tests -q
12 passed in 0.05s
$ uv run --project scripts/vendor cox-vendor anthropic-spec --check
anthropic-spec: up to date   (exit 0; the vendored snapshot is byte-identical)
```

#### T30.21 One key resolver for every provider section

Depends: — · Size: ~80 · Files: `cox-provider/src/http.rs`, `anthropic/mod.rs`, `crates/cox/src/session.rs`
Goal: every section resolves its key the same way (R§4.3.3, `docs/design/providers.md` § Target shape, item 2).
Plan:
1. `http::resolve_key(api_key_env, section)` reads the env var the section names, then the keyring entry `cox/<section>`.
2. A section marked local (no key required) gets `None` instead of an error.
3. The Anthropic provider uses its section's `api_key_env`, replacing the hardcoded `ANTHROPIC_API_KEY`.
4. OpenAI and compatible sections gain the keyring fallback.
Check:
- a test that a renamed `providers.anthropic.api_key_env` is honoured;
- a test that a compatible section falls back to the keyring (mock store);
- a test that a local section without a key yields no auth header;
- the existing key tests pass.
What landed:
- `cox-provider/src/http.rs`: `resolve_key(api_key_env, section)` reads the env var the section names, then the keyring `cox/<section>`. It replaces `resolve_key_env_or_keyring`.
  - The keyring lookup is injectable (`resolve_key_with`). keyring 4's v1 shim binds the platform store on first use, so a mock credential builder cannot be swapped in.
- `AnthropicProvider::new` takes the section's `api_key_env`, and `resolve_api_key()` is removed.
  - `providers.anthropic.api_key_env` is honoured now; before, it was never read.
- `session.rs::backend_for`: the OpenAI and compatible arms resolve through the same function, so they gain the keyring fallback.
  - A missing key stays `None`, meaning no auth header.
  - Anthropic and Jev still fail with `ProviderError::Auth`.
- `cox doctor`: `key_requirement` and `check_api_keys(config)` check the key of the section `tiers.code` routes to, with the same resolver.
  - Anthropic and Jev fail; OpenAI-shaped sections warn; `local` needs no key.
  - The keyring hint named the service and account backwards (`-a cox -s anthropic`); it is now `-s cox -a <section>`.
- Docs:
  - `api_key_env` doc comments in `config.rs` and `default.toml`;
  - `docs/config.md`, regenerated by its drift test;
  - `docs/design/providers.md`, which marks target-shape item 2 implemented.

Deviations:
- `doctor.rs` is a fourth file. It was folded in because a doctor that disagrees with the session about keys defeats the card.
- A chip for the same doctor fix had already been started as a separate session. That session was told the fix is in main.

Check:
```text
$ mise exec -- cargo nextest run --workspace
900 tests run: 900 passed, 3 skipped
  (new: resolve_key_prefers_the_env_var_over_the_keyring, resolve_key_falls_back_to_the_keyring_when_the_env_var_is_unset,
   resolve_key_is_auth_error_when_neither_env_nor_keyring_has_one, provider_new_honours_a_renamed_api_key_env,
   doctor_checks_the_key_the_code_tier_provider_names, doctor_warns_not_fails_when_a_keyless_section_has_no_key;
   local keyless = existing openai::chat::tests::chat_over_http_ollama_shaped)
$ mise exec -- cargo clippy --workspace --all-targets -- -D warnings   # clean
$ mise exec -- cargo fmt --check                                       # clean
$ COX_HOME=$(mktemp -d) cargo run --bin cox -- doctor                  # API keys ✓ anthropic
```

#### T30.20 Vendored data through saved scripts: prices and model lists from models.dev

Depends: T30.19 · Size: ~200 Python + tests
Goal: the rows in `crates/cox-provider/prices.toml` and the `[providers.*].models` lists in `crates/cox-protocol/default.toml` were copied by hand from models.dev (see the `prices.toml` header). They come from a script instead, so updating them is one command.
Plan:
1. `cox-vendor models` reads models.dev's public API. Check the endpoint URL and shape against models.dev's own docs and record them in R§4.3.3.
2. The command regenerates the price rows for the providers and model ids cox lists. It then rewrites the `models` arrays (id, context window, efforts mapped as in `docs/design/providers.md`) in `default.toml` with a comment-preserving TOML editor, leaving every other byte as it is.
3. The header of `prices.toml` records the source URL and the date.
4. `--check` prints the diff without writing.
5. `cox doctor`'s `PRICES_FIX` hint points at the command.
6. Tests run over a recorded models.dev subset fixture:
   - rows generated;
   - comments in `default.toml` kept;
   - an unknown id reported, not dropped;
   - idempotent.

Check: the package's tests pass. `cox-vendor models --check` against the fixture shows no diff. `usage_prices_toml_parses_and_has_all_tier_models` and the config-schema drift test pass.
Out of scope: A46's catalog crate (U4), which reads what this script writes.
What landed:
- `cox-vendor models` (`scripts/vendor/src/cox_vendor/models.py`) reads `https://models.dev/api.json`. It regenerates the `[[model]]` rows of `crates/cox-provider/prices.toml` and the `models` arrays of `crates/cox-protocol/default.toml`; the latter goes through tomlkit, so every other byte and comment stays.
  - The set of ids is whatever is already in the two files. An id models.dev lacks is reported and kept (`qwen3-coder`, `jev-latest`), and so is an unrecognised reasoning shape (`claude-haiku-4-5`, `qwen/qwen3-coder-plus`, `kimi-k2.7-code`).
  - A row's `verified_on` and the header date change only when numbers change, so a later run with unchanged data is a no-op.
  - `--check` prints a diff and exits 1.
- The first real run changed:
  - DeepSeek v4-flash prices;
  - OpenRouter `deepseek/deepseek-v4-pro` (about 40 % cheaper);
  - `gpt-5.6-sol` `cache_write` 5.0 (harmless: the OpenAI APIs report no cache-write tokens);
  - effort lists (Claude Sonnet/Opus/Fable gain `xhigh`; OpenRouter `deepseek-v4-pro` and `glm-5.2` drop `low`; OpenRouter `x-ai/grok-4.3` drops `xhigh`).
- The `prices.toml` header now names the command, and its stale "cache_read is a multiplier" line is corrected. `cox doctor`'s `PRICES_FIX` points at `just vendor models`.
- R§4.3.3 records the models.dev fact block: endpoint, shape, User-Agent, and id mismatches (`moonshot`→`moonshotai`, `z-ai`→`zai`).
- tomlkit is a new dependency (maintained comment-preserving TOML editor) with a `toolchain.md` row.

Deviations:
- `config_default_toml_carries_compatible_providers_with_models` asserted the old DeepSeek efforts and now expects models.dev's `[Low, High, Xhigh]`.
- `docs/config.md` was regenerated by its drift test.

Check:
```text
$ uv run --project scripts/vendor pytest scripts/vendor/tests -q
35 passed
$ uv run --project scripts/vendor cox-vendor models          # real run, then:
$ uv run --project scripts/vendor cox-vendor models --check  # no diff
$ mise exec -- cargo nextest run --workspace
900 tests run: 900 passed, 3 skipped
clippy -D warnings clean; fmt --check clean

#### T30.22 One `Transport` descriptor in every provider section

Depends: T30.21 · Size: ~120 · Files: `cox-protocol/src/config.rs`, the committed config schema, `crates/cox/src/config_load.rs`
Goal: every `[providers.*]` table, native or compatible, has the same `base_url`, `api_key_env`, `timeout_s` and `max_retries`, through one flattened `Transport` struct (item 1).
Plan:
1. Add `Transport` with defaults equal to today's values.
2. `#[serde(flatten)]` it into the Anthropic, OpenAI, Local, Jev and compatible sections, and drop their duplicate fields.
3. Regenerate the schema.
Check:
- the config-schema drift test passes;
- `default.toml` and a config written before this change load to the same values (a round-trip test).
What landed:
- `cox-protocol/src/config.rs`: a plain `Transport { base_url, api_key_env, timeout_s, max_retries }` struct. `impl_transport!` gives every section (Anthropic, OpenAI, Local, Jev, compatible) a `transport()` accessor.
  - The four knobs stay flat section-level fields, so the TOML does not change.
  - `deny_unknown_fields` still rejects typos. serde cannot combine `flatten` with `deny_unknown_fields`, which is why `#[serde(flatten)]` was not used.
- New fields:
  - OpenAI, Local and compatible sections gained `timeout_s = 120` and `max_retries = 4` (`retry::Policy::default()`).
  - Local gained `api_key_env = ""`, meaning no key.
  - The clients do not read these knobs yet (T30.23), so behaviour is unchanged.
- Docs:
  - `default.toml` comments;
  - `docs/config.md`, regenerated by its drift test;
  - `docs/design/providers.md` item 1: "Implemented (T30.22): config side".

Deviations:
- About 240 lines edited instead of ≈200. The excess is doc comments and the four tests.
- Test-only struct literals in `router.rs` and `session.rs` gained `..Default::default()`.
- No committed JSON schema for `Config` exists, so nothing was regenerated.

Check:
```text
$ mise exec -- cargo nextest run --workspace
905 tests run: 905 passed, 3 skipped
  (new: every_provider_section_transport_matches_documented_defaults, local_provider_api_key_env_defaults_to_empty,
   provider_sections_without_the_new_transport_keys_load_to_documented_defaults, unknown_key_in_a_provider_section_is_still_rejected)
clippy -D warnings clean; fmt --check clean
$ cox config show   # every section prints base_url / api_key_env / timeout_s / max_retries
```

#### T30.23 Provider constructors take `&Transport`

Depends: T30.22 · Size: ~150 · Files: `crates/cox/src/session.rs`, `cox-provider/src/openai/chat.rs`, `openai/responses.rs`
Goal: `backend_for` becomes one lookup from `api` shape to constructor. Chat and Responses read `timeout_s` and `max_retries` from the section instead of `Policy::default()`, and the Local provider stops taking the whole config struct (item 1).
Check:
- a wiremock test that a Chat section with `max_retries = 0` makes exactly one attempt on a 529;
- the existing provider tests pass.
Status: done 2026-09-26

What landed:
- `http::client_with_timeout(timeout_s)`: one client builder for every backend; the connect timeout moved here from `anthropic/mod.rs`.
- `AnthropicProvider::new`, `JevProvider::new`/`with_key`, `OpenAiChatProvider::new` and `OpenAiResponsesProvider::new` take `&Transport`; `timeout_s` and `max_retries` come from the section instead of `Policy::default()`.
- `backend_for`: Anthropic and Jev keep their own arm (cache TTL, fallbacks, decision model); `openai`, `local` and every compatible section go through one `openai_shaped` helper that resolves the key once and picks Chat or Responses by `api`. Local has no bespoke constructor.
- `providers.local.timeout_s` defaults to 600, because a slow local prefill can outrun 120 s; `docs/config.md` regenerated; `docs/design/providers.md` item 1 notes the change.

Deviations:
- ~400 insertions over 10 files, above the ≤200 LOC / ≤3 files limit. The bulk is the wiremock retry tests the Check asks for and doc comments; the change is one refactor that does not split cleanly.
- The new `backend_for` test sets a dedicated env var for each section, so `resolve_key` never reaches the keyring (A49). The older violators are T30.28.

Check:
- `chat_529_with_zero_max_retries_makes_one_attempt`, `chat_529_with_two_max_retries_makes_three_attempts`, `responses_529_with_zero_max_retries_makes_one_attempt`, `backend_for_builds_the_right_provider_kind_per_section` pass.
- `cargo nextest run --workspace`: 909 passed, 3 skipped; clippy and fmt clean.

#### T30.28 Tests never touch the real keychain

Depends: T30.23 (it edits `anthropic/mod.rs`) · Size: ~80 · Files: `cox-provider/src/anthropic/mod.rs`, `crates/cox/src/doctor.rs`, a new source-scan test in `crates/cox/tests/`
Goal: the AGENTS.md rule (A49). No test reads the OS keychain, so a test run never prompts for the login password and never depends on the developer's stored keys. Today:
- the Anthropic key tests call the real `resolve_key("ANTHROPIC_API_KEY", "anthropic")`, which reads the `cox/anthropic` item;
- doctor's `check_api_keys` tests reach the real store through `resolve_key`;
- doctor's MCP row calls `cox_mcp::auth::stored`, which opens a keyring entry, if a test config names an OAuth server.
Plan:
1. The Anthropic tests pass a fake lookup through `resolve_key_with`.
2. Doctor gets `check_api_keys_with(config, lookup)`; `check_api_keys` passes the real resolver, the tests pass a fake. The same for the MCP row if a test reaches it.
3. A source-scan test fails when a `#[cfg(test)]` module in any crate calls `resolve_key(`, `platform_keyring` or `keyring::Entry`.
Check:
- the scan test passes, and fails on a planted call;
- `cargo nextest run --workspace` passes with no keychain prompt on macOS.
Status: done 2026-09-26

What landed:
- Seams for injecting the key lookup:
  - `AnthropicProvider::with_key`, alongside the existing `JevProvider::with_key`;
  - `provider_for_with` and `backend_for_with` in `crates/cox/src/session.rs`, with `openai_shaped` taking the resolver;
  - `doctor::check_api_keys_with`;
  - `http::resolve_key_with` is now `pub(crate)`.

  The production paths pass `cox_provider::http::resolve_key`, so the binary behaves the same.
- The Anthropic, doctor and session tests use fake resolvers. The env-var juggling in `backend_for_builds_the_right_provider_kind_per_section` is gone.
- `crates/cox/tests/no_real_keychain_in_tests.rs` scans every `#[cfg(test)]` module and every file under a `tests/` directory. It blanks comments and strings first, then fails on `resolve_key(`, `platform_keyring` or `keyring::Entry`. Three unit tests cover the scanner itself.
- `docs/design/providers.md` item 2 documents the seams.
- Paths checked and found clean:
  - the e2e tests run the binary with `COX_PROVIDER=scripted`, which returns before any key lookup;
  - the cox-mcp auth tests use the in-memory store;
  - cox-acp and cox-tui build no provider.

Deviations:
- 6 files, about 150 edited lines plus the 252-line scan test. That is above the 3-file limit, because the session.rs path, which the card missed, was folded in.
- The scanner only sees direct calls. An indirect path through production code is caught by review and the seams, not by the scan.

Check:
- A call planted in the anthropic and session test modules made `no_test_reads_the_real_keychain` fail; the plant was then reverted.
- `cargo nextest run --workspace`: 913 passed, 3 skipped; clippy and fmt clean.

#### T30.24 `cox-models`: one model catalog

Depends: T30.20, T30.23 · Size: ~200 · Files: new crate `crates/cox-models`, `cox-provider/src/usage.rs`, `crates/cox/tests/deps.rs`
Goal: one pure catalog: model id → context window, max output, efforts, capabilities (tools, adaptive thinking, reasoning-effort parameter) and price (item 3).
Plan:
1. Built-in rows are embedded from the files T30.20's script writes.
2. `[providers.<name>].models` and a user `prices.toml` override them by id.
3. `PriceTable` moves into the catalog; `Priced` looks prices up through it.
4. `deps.rs`: `cox-models` depends only on `cox-protocol`, and `cox-core` may depend on it.
Check:
- tests for the override order, built-in < config < user file;
- `usage_prices_toml_parses_and_has_all_tier_models` passes against the catalog.
Status: done 2026-09-26

What landed:
- New pure crate `crates/cox-models`.
  - `price.rs`: `Price`, `PriceError` and `PriceTable` moved from `cox-provider/src/usage.rs`. The private `from_str` became `pub fn parse`, because clippy's `should_implement_trait` fires on a public `from_str`.
  - `catalog.rs`: `Capabilities`, `ModelRow` (id, context window, max output, efforts, capabilities, price) and `Catalog`.
    - `Catalog::builtin()` reads the embedded `default.toml` model arrays and `prices.toml`.
    - `Catalog::load(config, user_prices)` layers built-in < config < user price file. An empty `efforts` list in config means "any", so it does not clear a built-in row's efforts.
- `cox-provider::usage` re-exports the price types and keeps `Priced`, `ledger_row` and `load_price_table(path)`. That last one is the one disk read, moved out of the pure crate with the same fallback: found → parse; not found → embedded; other error → `Io`. Doctor calls it.
- `deps.rs`: `cox-models` depends only on `cox-protocol`; `cox-provider` may use `cox-models`; `cox-core` may, but does not yet.
- Docs: an AGENTS.md layout row, the plan.md §1.1 row and dependency sentence, and `docs/design/providers.md` item 3 "Implemented (T30.24)".

Deviations:
- `capabilities` and `max_output` stay `None`, because `cox-vendor models` does not emit them yet. T30.25 and T30.26 are their first readers.
- About 250 new lines in `catalog.rs`, tests included, over the ~200 estimate; `price.rs` is a move.

Check:
- The override-order tests pass: built-in < config < user file, both directions; a config row keeps the built-in efforts; a user-priced id gets a row.
- `usage_prices_toml_parses_and_has_all_tier_models` passes in `cox-models`.
- `cargo nextest run --workspace`: 920 passed, 3 skipped; clippy and fmt clean.
- `COX_HOME=<tmp> cargo run --bin cox -- doctor` shows `prices: ✓ oldest verified_on 2026-09-02`, as before.

#### T30.29 A keyring switch: no keychain prompt from any cargo run

Why: after T30.28 no test reached the keyring, but smoke runs of the rebuilt binary (`cargo run -- doctor`) still did. Every rebuild has a new code signature, so macOS asked for the login password again. The creator asked for every keyring place to use fakes (A51).

Status: done 2026-09-26

What landed:
- `cox_protocol::config::KEYRING_ENV` (`COX_KEYRING`) and a pure `keyring_enabled(value)` that is false only for `off`, `0` or `false`.
- `cox_provider::http::platform_keyring` returns nothing when the switch is off.
- `cox_mcp::auth` reads find nothing and writes fail with "keyring disabled by COX_KEYRING".
- The config env layer ignores `COX_KEYRING`.
- `.cargo/config.toml` `[env]` sets `COX_KEYRING = "off"` for `cargo run`, `cargo test` and `cargo nextest`. A value already set in the shell wins. Release builds and installed binaries are unaffected, because the switch is read at run time.
- Docs: the AGENTS.md rule and doctor command, and `docs/design/providers.md` item 2 "Implemented (T30.29)".

Check:
- `keyring_is_off_only_for_an_explicit_off_value` and `tests_run_with_the_keyring_switched_off` pass. The second proves that nextest applies the cargo `[env]`.
- `config_ignores_test_only_cox_env_vars` passes with `COX_KEYRING=off`.
- `no_test_reads_the_real_keychain` passes.
- `env -u ANTHROPIC_API_KEY COX_HOME=<tmp> cargo run --bin cox -- doctor` finishes with no keychain prompt and reports the key as missing.
- `cargo nextest run --workspace`: 920 passed, 3 skipped; clippy and fmt clean.

#### T29.3 `COX_*` env overrides for keys with an underscore

Model: claude-opus-5-5 · Status: done 2026-09-23 · Depends: - · Size: ~80 · Priority: P1 · Complexity: 2
Goal: `COX_TUI_SHOW_THINKING=full` sets `tui.show_thinking` (and `COX_HOOKS_TIMEOUT_S` sets `hooks.timeout_s`, `COX_TUI_SCREEN_READER` sets T29.1's `tui.screen_reader`). Today the env layer splits the name on every `_`, so any key that itself contains `_` becomes `tui.show.thinking` and fails `deny_unknown_fields` or is lost.
Files: `crates/cox/src/config_load.rs`.
Steps: (1) Replace `Env::split("_")` with a `map` that walks the key tree of `DEFAULT_CONFIG_TOML`: at each table take the longest child name that equals the rest of the env name or prefixes it followed by `_`, descend, and fall back to splitting the unmatched remainder on `_` (so `COX_TIERS_<custom>_MODEL` still reaches a user-defined tier as before). (2) Keep the `ignore` list ahead of the map, so it still matches pre-split names (`expect_sandbox`, and T29.1's `plain`, `ax_startup_quiet_ms`). (3) Regression test `config_env_overrides_keys_with_underscores` that sets `COX_TUI_SHOW_THINKING` and `COX_TIERS_CODE_MAX_TOKENS` and fails without the fix. `docs/config.md` is generated from `default.toml` and does not state the env rule, so it stays untouched; the rule is clarified in §1.6.
Check:
```bash
mise exec -- cargo nextest run -p cox config_env
```
Done when: the test passes, and `COX_TIERS_CODE_MODEL` (existing test) still works.
Out of scope: env names for keys containing `-` (`providers.z-ai`), which a shell cannot export anyway.

What landed (commit `T29.3: COX_* env overrides for keys with an underscore`): `default_key_tree()` extracts the embedded defaults as a figment `Dict`; `env_key(tree, name)` walks it taking the longest known name at each level and splits only the unmatched remainder on `_`; the env provider's `.split("_")` became `.map(env_key)`, still after `.ignore(...)`. Tests `config_env_overrides_keys_with_underscores` (load through every layer) and `env_key_resolves_known_keys_and_splits_the_rest` (`hooks.timeout_s`, `tiers.code.model`, and fallback into `tui.icons.*` / an unknown tier). §1.6 states the rule. Merge note: T29.1 adds `plain` and `ax_startup_quiet_ms` to the same `ignore` list; that still runs before the map, so its entries keep working unchanged.

Check:
```text
$ mise exec -- cargo nextest run -p cox config_env env_key
        PASS config_load::tests::config_env_overrides_keys_with_underscores
        PASS config_load::tests::config_env_overrides_project
        PASS config_load::tests::env_key_resolves_known_keys_and_splits_the_rest
$ # same test with `.split("_")` restored (fails without the fix):
        FAIL unknown field: found `max`, expected one of `provider`, `model`, `effort`, `max_tokens`, `thinking`, `confirm` for key "default.tiers.code.max" in env
$ COX_HOME=<scratch> COX_TUI_SHOW_THINKING=full COX_HOOKS_TIMEOUT_S=7 cargo run --bin cox -- config show --sources
hooks.timeout_s = 7 # env
tui.show_thinking = "full" # env
$ mise exec -- cargo nextest run --workspace
     708 passed, 3 skipped
$ mise exec -- cargo clippy --workspace --all-targets -- -D warnings
     clean
$ mise exec -- cargo fmt --check
     clean
```

Landed on main 2026-09-26; originally merged only into `sync-2026-09-25-local-main` via PR #37.

#### T23.8 `cargo run` picks `cox` again

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: T23.1 · Size: ~5 · Priority: P1 · Complexity: 1
Goal: the documented `COX_HOME=/tmp/cox-scratch mise exec -- cargo run -- doctor` runs `cox` instead of failing with "could not determine which binary to run ... available binaries: cox, kitty_probe", and the T23.1 PTY tests still spawn `kitty_probe`.
Files: `Cargo.toml`.
Steps: (1) The root manifest is virtual, so `default-run` (a `[package]` key) is not available; add `default-members = ["crates/cox"]` to `[workspace]`, which is the set bare `cargo run`/`build` resolve against. `kitty_probe` stays where it is: every test/lint command in `AGENTS.md`, `justfile` and CI already passes `--workspace` or `-p`, so it is still built and `CARGO_BIN_EXE_kitty_probe` still resolves.
Check:
```bash
# doctor exits 1 without an API key; the Check is that `cox` ran at all.
out="$(COX_HOME="$(mktemp -d)" mise exec -- cargo run -q -- doctor 2>&1 || true)"
grep -q '^toolchain: ' <<<"$out"
mise exec -- cargo nextest run -p cox-tui --test shell
```
Execution plan: edit `Cargo.toml` `[workspace]`; run the Check, then nextest/clippy/fmt under `mise exec`; confirm `cargo fmt --check` still covers every member.
Done when: the Check passes and the three workspace commands are clean.
Deviations: first claimed and committed as T23.7 in a separate worktree; renumbered to T23.8 because T23.7 is "Resize hardening". The `Check` was rewritten while the task was open: `doctor` exits 1 without an API key, so the check greps its `toolchain:` row instead of the exit code.
Out of scope: moving or feature-gating `kitty_probe` (either needs more code and the test would have to opt in to a feature).

Check output:
```
$ out="$(COX_HOME="$(mktemp -d)" mise exec -- cargo run -q -- doctor 2>&1 || true)"; grep -q '^toolchain: ' <<<"$out"
exit 0 (doctor itself exits 1: no API key in the scratch home)
$ mise exec -- cargo nextest run -p cox-tui --test shell
7 tests run: 7 passed, 0 skipped
$ mise exec -- cargo nextest run --workspace
841 tests run: 841 passed, 3 skipped
$ mise exec -- cargo clippy --workspace --all-targets -- -D warnings
clean
$ mise exec -- cargo fmt --check
clean
```

Landed on main 2026-09-26; originally merged only into `sync-2026-09-25-local-main` via PR #37.

#### T31.1 Request bodies build without expect

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: - · Size: ~20 · Priority: P1 · Complexity: 1
Goal: the Anthropic, OpenAI Responses and OpenAI Chat request builders stop calling `expect` on the `json!` body (v0.1 DoD §4.6, §6 A50).
Files: `crates/cox-provider/src/openai/chat.rs`.
Steps: replace `body.as_object_mut().expect(..)` with `let Value::Object(obj) = &mut body else { return .. }`; the else arm returns the body unchanged and is unreachable for an object literal.
Deviation: the branch also touched `anthropic/request.rs` and `openai/responses.rs`, but by the time this landed on `main`, T30.11–T30.12 had already rewritten both builders around typed `wire::CreateMessageParams`/`wire::CreateResponse` and made them return `Result` without ever using `body.as_object_mut().expect(..)`; those two hunks were dropped as obsolete (the remaining `.expect()` calls in those files are confined to `#[cfg(test)]` helpers), and only the `openai/chat.rs` fix landed.
Check:
```text
$ mise exec -- cargo nextest run -p cox-provider
     all pass (part of the workspace run below)
```
Done when: no `expect` remains in the three builders and the provider snapshots are unchanged.
Landed on main 2026-09-26 from branch t31-beta-mvp.

#### T31.2 Jev client construction is fallible

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: - · Size: ~25 · Priority: P1 · Complexity: 1
Goal: `JevProvider::with_key` returns `Result<Self, ProviderError>` instead of `expect`ing the reqwest builder (v0.1 DoD §4.6, §6 A50).
Files: none — see deviation.
Deviation: fully superseded before this landed. T30.23 (A46 U1) had already changed every provider constructor, `JevProvider::new`/`with_key` included, to take `&Transport` and return `Result<Self, ProviderError>` (the client now builds through `crate::http::client_with_timeout`, mapping a build failure to `ProviderError`), and `session::provider_for` already propagates that `Result` with `?`. Replaying the branch's `jev.rs`/`session.rs` diff onto current `main` produced an empty diff once conflicts were resolved in favor of `main`, so no code changed for this task.
Check: n/a — nothing to run; `JevProvider::with_key`/`new`'s fallibility and `session::provider_for`'s propagation are already covered by T30.23's and T30.28's tests.
Done when: `JevProvider::with_key`/`new` are fallible and `session::provider_for` propagates the error — already true on `main` via T30.23.
Landed on main 2026-09-26 from branch t31-beta-mvp (no code changed; fully superseded by T30.23).

#### T31.3 Config overrides and config show without expect

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: - · Size: ~40 · Priority: P1 · Complexity: 1
Goal: the CLI override tree and `cox config show` stop calling `expect` (v0.1 DoD §4.6, §6 A50).
Files: `crates/cox/src/config_load.rs`, `crates/cox/src/config_cmd.rs`, `crates/cox/src/main.rs`.
Steps: `set_dotted` walks the dotted path through a recursive `set_path` that replaces any non-object with an empty object and matches the map with `let-else`; `config_cmd::show` returns `anyhow::Result<()>` and `main.rs` returns it.
Check:
```text
$ COX_HOME=<scratch> ./target/debug/cox --model claude-haiku-4-5 --sandbox read-only config show --sources | grep '# flag'
sandbox.mode = "read-only" # flag
tiers.code.model = "claude-haiku-4-5" # flag
```
Done when: the flag overrides still land at their dotted keys and no `expect` remains in either file.
Landed on main 2026-09-26 from branch t31-beta-mvp.

#### T31.4 End-to-end test of cox mcp serving read, grep and glob

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: - · Size: ~140 · Priority: P1 · Complexity: 2
Goal: prove v0.1 DoD §4.5 ("`cox mcp` serves `read`/`grep`/`glob`") against the built binary, not stand-in tools (§6 A50).
Files: `crates/cox/tests/mcp_serve.rs` (new), `crates/cox/src/mcp_cmd.rs`.
Steps: spawn `cox --cwd <tmp> mcp` with a scratch `COX_HOME`, speak newline-delimited JSON-RPC over stdio (`initialize`, `notifications/initialized`, `tools/list`, three `tools/call`) with a 20 s per-response timeout so a silent server fails instead of hanging.
Deviation: the first run showed `tools/list` returned `glob`, `grep`, `read` while the default selection named `outline` too — no tool has that name (an outline is `read` with `mode = "outline"`), so `READ_ONLY` drops it and the unit test's counts follow (default 3, with `--allow-write` 6).
Check:
```text
$ mise exec -- cargo nextest run -p cox --test mcp_serve
        PASS [   1.715s] (1/1) cox::mcp_serve cox_mcp_serves_read_grep_and_glob_from_the_built_binary
```
Done when: the test passes and `tools/list` matches the default selection exactly.
Landed on main 2026-09-26 from branch t31-beta-mvp.

#### T31.5 README quick start uses cox run -p; drop stale cli doc

Model: claude-opus-5-5 · Status: done 2026-09-25 · Depends: - · Size: ~5 · Priority: P2 · Complexity: 1
Goal: every command in the README runs as written, and the `Command` doc stops claiming subcommands print `not implemented` (§6 A50).
Files: `README.md`, `crates/cox/src/cli.rs`.
Steps: the 60-second start's `cox -p "..."` becomes `cox run -p "..."` (the top-level `Cli` has only a positional prompt; `-p` belongs to `run`); the doc comment says `main.rs` dispatches each subcommand.
Deviation: checked against the licensing README section (badges, `## License`) added to `main` since this branch was cut — that section sits elsewhere in the file (top badges, bottom `## License`) and is untouched by this change.
Done when: the README commands run as written and the `Command` doc comment matches the shipped binary.
Landed on main 2026-09-26 from branch t31-beta-mvp.

Check for T31.1–T31.5 together, as landed on `main` (branch t31-beta-mvp cherry-picked over T30.19–T30.28 and the crate-split table entries):
```text
$ mise exec -- cargo nextest run --workspace --no-fail-fast
     Summary [ 16.307s] 914 tests run: 914 passed, 3 skipped
$ mise exec -- cargo clippy --workspace --all-targets -- -D warnings
     Finished `dev` profile [unoptimized + debuginfo] target(s) in 47.79s
$ mise exec -- cargo fmt --check
     clean
```
`cox::no_real_keychain_in_tests` and `cox::mcp_serve` are both in that count and both pass; `mcp_serve`'s `cox --cwd <tmp> mcp` never resolves a provider key (its `run` path only loads config and builds the built-in tool list), so it cannot reach the real keychain, and a manual run with `ANTHROPIC_API_KEY=not-a-real-key COX_HOME=$(mktemp -d)` confirms the T31.3 `config show --sources` check with no keychain prompt.

#### T32.1 `cox-sanitize`: the terminal-text guard in its own crate

Depends: — · Moves: `cox-tui/src/text.rs`.
Why: guard (b) and reuse (d). `crates/cox/src/plain.rs` imports the whole TUI for `sanitize`.
Plan: `cox-tui` re-exports `text`; `plain.rs` imports `cox_sanitize`. The AGENTS.md trust list names `cox_sanitize::sanitize`.
Check: `cargo tree -p cox-sanitize` has no workspace dependency.
Status: done 2026-09-26

What landed:
- `crates/cox-tui/src/text.rs` was moved with `git mv` to `crates/cox-sanitize/src/lib.rs`, tests included, with no logic change. Its only dependency is unicode-width.
- `cox-tui` re-exports it (`pub use cox_sanitize as text;`), so `cox_tui::text::sanitize` still resolves.
- `crates/cox/src/plain.rs` imports `cox_sanitize` directly.
- `deps.rs`: `cox-sanitize` has no workspace dependency; `cox-tui` and `cox-acp` may depend on it.
- Docs: AGENTS.md layout row and trust list, the SECURITY.md guard list, and the plan.md §1.1 row and dependency sentence.

Deviations:
- Done in worktree `_worktrees/cox-t32.1`, in parallel with T30.24, then cherry-picked onto main. The `deps.rs` conflict with T30.24's `cox-models` rule was resolved by keeping both rules.

Check:
- `cargo tree -p cox-sanitize` shows only unicode-width.
- `crates/cox-tui/tests/sanitize.rs` passes unchanged through the old path.
- The full suite after landing on main is in the commit message check below.

#### T30.25 `Caps` and adaptive thinking from the catalog

Depends: T30.24 · Size: ~120 · Files: `cox-provider/src/anthropic/mod.rs`, `anthropic/request.rs`, `jev.rs` (and the `400_000` in `session.rs` if it fits; otherwise the next card)
Goal: delete the `Caps.max_context` literals (`200_000`, `128_000`, `400_000`) and `ADAPTIVE_THINKING_PREFIXES`. Context and "sends adaptive thinking" come from the catalog row (item 3).
Check:
- a test that a configured 1M-context Anthropic model reports 1M;
- the request snapshots are unchanged for the built-in models.
Status: done 2026-09-26

What landed:
- `AnthropicProvider` and `JevProvider` carry `max_context`, set by `backend_for_with` from `cox_models::Catalog::load(config, None)`. The three literals (`200_000`, `128_000`, the native-OpenAI `400_000` in `session.rs`) are gone from the capability paths and survive only as the documented fallback for a model with no catalog row, so built-in behaviour is unchanged.
- `ADAPTIVE_THINKING_PREFIXES` left `anthropic/request.rs`. `cox_models::supports_adaptive_thinking(model_id)` is the one place that answers it; `build_body` calls it and stays a pure, snapshot-tested function.
- `crates/cox` depends on `cox-models` directly (`session.rs` resolves the catalog).
- Docs: "Implemented (T30.25)" under item 3 of `docs/design/providers.md`.

Deviations:
- Adaptive thinking is a name rule in `cox-models`, not a per-row `Capabilities.adaptive_thinking` value: the vendor script does not emit that field yet, and a row lookup would stop matching a dated model id with no row. Filling the field from models.dev's `reasoning_options` through `scripts/vendor` is a follow-up, recorded in `ideas.md`.
- `max_context` is per provider section and uses the `code` tier's model for the Anthropic and native OpenAI arms, the section's own model for Jev — the `Caps` shape is per provider today.

Check:
- `session::tests` prove a configured 1M-context Anthropic model reports 1M and an unlisted model falls back to 200k.
- `cargo insta test -p cox-provider`: 133 passed, no snapshots to review.
- `cargo nextest run --workspace`: 927 passed, 3 skipped. clippy and fmt clean. `cox doctor` against a scratch `COX_HOME`: no keychain prompt.

#### T32.3 `cox-sandbox`: `sandbox::Policy` and `path::confine`

Depends: — · Moves: `cox-tools/src/sandbox/*`, `cox-tools/src/path.rs` (~920).
Why: dependencies (a) and guard (b).
Plan: `cox-tools` re-exports `sandbox` and `path`. The AGENTS.md trust list names the new crate.
Check: landlock and seccompiler appear only in `cox-sandbox/Cargo.toml`.
Status: done 2026-09-26

What landed:
- `crates/cox-tools/src/path.rs` and `src/sandbox/{mod,bwrap,landlock,seatbelt}.rs` moved with `git mv` to `crates/cox-sandbox`, tests included, no logic change.
- `cox-tools` re-exports them (`pub use cox_sandbox::path;`, `pub use cox_sandbox::sandbox;`), so `cox_tools::path::confine` and `cox_tools::sandbox::Policy` still resolve for every caller.
- landlock and seccompiler left `cox-tools/Cargo.toml`; `nix` stays there too because `bash/mod.rs` uses it for the pty.
- `deps.rs`: `cox-sandbox` → `cox-protocol` only; `cox-tools` → `cox-protocol`, `cox-sandbox`.
- Docs: AGENTS.md layout row and trust list, the SECURITY.md guard list, the plan.md §1.1 row and dependency sentence.

Deviations:
- Done in worktree `_worktrees/cox-t32.3`, then cherry-picked onto main after T30.25. The integration tests in `cox-tools/tests/` stay where they are and exercise the re-export path, as T32.1 did.

Check:
- landlock and seccompiler appear only in the root pin and `crates/cox-sandbox/Cargo.toml`.
- `cargo check --target x86_64-unknown-linux-gnu -p cox-sandbox` is green (the Linux-gated code compiles).
- The full suite after landing on main is in the commit below.

#### T32.15 `cox-provider-jev`

Status: dropped 2026-09-26 · Depends: T32.12, T30.21–T30.26 as in T32.13 · Priority: P2 · Complexity: 2
Goal (as planned): move `cox-provider/src/jev.rs` into its own crate `cox-provider-jev` (size, item c).
Reason for dropping: superseded by §6 A52 (T33.40, the Jev-as-a-plugin work). Once the Jev plugin reaches parity (T33.40.5–T33.40.6), `jev.rs` is deleted outright by T33.40.12, not extracted into a crate — the built-in Jev client is going away, so splitting it into its own crate first would be immediately undone. Recorded here rather than left `todo`, per the creator's decision of 2026-09-26 (`docs/design/plugins.md` §14 decision 12).
Check: n/a — no code moved for this task.

#### T32.6 `cox-patch`: the V4A patch engine

Depends: — · Moves: `cox-tools/src/v4a/*` (~990).
Why: size (c), a self-contained leaf.
Check: the `v4a` tests pass unchanged in the new crate.
Status: done 2026-09-26

What landed:
- `crates/cox-patch` holds the pure V4A engine: `parse.rs` (moved with `git mv`, 12 tests) and the pure part of `apply.rs` (`Change`, `stage`, hunk matching; 8 tests). No filesystem, no `ToolCx`.
- `ApplyPatchTool` and its `Tool` impl stay in `cox-tools/src/v4a/tool.rs`, because they run `path::confine` and `write::atomic_write`; `v4a/mod.rs` re-exports `cox_patch`'s types, so `cox_tools::v4a::*` still resolves.
- Widened: `Change::status()` only.
- `deps.rs`: `cox-patch` → `cox-protocol` only; `cox-tools` → `cox-protocol`, `cox-sandbox`, `cox-patch`.
- Docs: AGENTS.md layout rows, `docs/design/crates.md` `cox-patch` row, plan.md §1.1 row and dependency sentence.

Deviations:
- The card assumed `v4a` was a leaf; `apply.rs` imports `crate::path::confine` and `crate::write`. Moving it whole would create a `cox-tools` ↔ `cox-patch` cycle. The creator approved the split: the engine moves, the tool wrapper stays, so `confine` keeps its single call site.
- Done in worktree `_worktrees/cox-t32.6`, then cherry-picked onto main after T32.3; the `deps.rs`, AGENTS.md and `cox-tools/Cargo.toml` conflicts kept both sides.

Check:
- `cargo nextest run -p cox-patch`: 14 passed; the 25-patch golden corpus in `cox-tools/tests/v4a.rs` passes unchanged.

#### T30.27 `cox doctor`: catalog and price sync row

Depends: T30.24 · Size: ~60 · Files: `crates/cox/src/doctor.rs`
Goal: a model reachable from `[tiers.*]` or `[providers.*].models` with no catalog price is a doctor warning that names the model and points at `cox-vendor models` (item 5).
Check:
- a doctor test with a config naming an unpriced model;
- `COX_HOME=/tmp/cox-scratch cox doctor` shows the row as green on the defaults.
Status: done 2026-09-26

What landed:
- `Config::configured_model_ids()` in `cox-protocol` is the one enumeration of every reachable model (tiers, single-model sections, every `models` list, custom sections). `cox-models`' `usage_prices_cover_every_configured_model` test and the doctor row both use it.
- `cox doctor` has a "catalog prices" row: every configured model without a catalog price is named, with the fix `uv run --project scripts/vendor cox-vendor models`.
- Docs: "Implemented (T30.27)" under item 5 of `docs/design/providers.md`; the doctor-checks list in `docs/how-it-works.md`.

Deviations:
- Empty model ids (a compatible section with no default `model`) are skipped, so they never produce a blank warning.
- Done in worktree `_worktrees/cox-t30.27`, then cherry-picked onto main.

Check:
- Tests: `catalog_prices_check_is_ok_on_the_default_config`, `catalog_prices_check_warns_and_names_an_unpriced_model`, `catalog_prices_check_names_every_unpriced_model_reachable_from_tiers`.
- `COX_HOME=<scratch> cox doctor`: `catalog prices: ✓ 21 configured models priced`.

#### T32.4 `cox-syntax`: tree-sitter and its grammars

Depends: — · Moves: `cox-tools/src/outline.rs` and the parser setup from `bash/classify.rs` (one `parse_bash` fn).
Why: dependencies (a), namely tree-sitter and five grammar crates, each a C build.
Check: no `tree_sitter*` dependency is left in `cox-tools/Cargo.toml`; the classifier and outline tests are unchanged.
Status: done 2026-09-26

What landed:
- `crates/cox-tools/src/outline.rs` moved with `git mv` to `crates/cox-syntax/src/outline.rs`, byte-identical. `cox-syntax` also owns `parse_bash`, the tree-sitter-bash parser setup; the risk walk in `bash/classify.rs` stays in `cox-tools` and calls it.
- `cox-syntax` re-exports `tree_sitter::Node`, so `classify.rs` names the node type without a tree-sitter dependency.
- tree-sitter and its five grammars left `cox-tools/Cargo.toml`. `cox-tools` re-exports `outline`.
- `deps.rs`: `cox-syntax` has no workspace dependency; `cox-tools` may use it.
- Docs: AGENTS.md layout row, plan.md §1.1 row and dependency sentence.

Deviations:
- `cargo check --target x86_64-unknown-linux-gnu -p cox-syntax` needs a Linux cross gcc for the grammars' C code, which this Mac lacks (true before the move too); `cargo zigbuild --target x86_64-unknown-linux-gnu -p cox-syntax` builds clean instead.
- Done in worktree `_worktrees/cox-t32.4`, then cherry-picked onto main.

Check:
- `grep tree.sitter crates/cox-tools/Cargo.toml` is empty.
- The outline tests (now in `cox-syntax`) and the bash classifier tests pass unchanged.

#### T32.10 `cox-tokens`: token counting

Depends: — · Moves: `cox-provider/src/tokens.rs`.
Why: dependencies (a), namely tiktoken-rs and its BPE data.
Check: `tiktoken-rs` appears only in `cox-tokens/Cargo.toml`; the `fixtures/count_tokens` tests pass.
Status: done 2026-09-26

What landed:
- `crates/cox-provider/src/tokens.rs` moved with `git mv` to `crates/cox-tokens/src/lib.rs`, tests included, no logic change. The whole file moved: it uses no `cox-provider` item, and `count_anthropic` takes a plain `reqwest::Client`.
- `cox-provider` re-exports it (`pub use cox_tokens as tokens;`), so `cox_provider::tokens::*` still resolves.
- tiktoken-rs left `cox-provider/Cargo.toml`.
- `deps.rs`: `cox-tokens` → `cox-protocol` only; `cox-provider` may use `cox-tokens`.
- Docs: AGENTS.md layout row, plan.md §1.1 row and dependency sentence.

Deviations:
- Done in worktree `_worktrees/cox-t32.10`, then cherry-picked onto main.

Check:
- `tiktoken-rs` appears only in the root pin and `crates/cox-tokens/Cargo.toml`.
- `cargo nextest run -p cox-tokens`: 7 passed, including `tokens_estimate_within_15_percent_of_fixtures` over `fixtures/count_tokens`.

#### T32.8 `cox-permission`: the permission engine

Depends: — · Moves: `cox-core/src/permission/*` (448).
Why: guard (b), and it is pure.
Plan: `cox-core` re-exports `permission`. In `deps.rs`, `cox-core` may depend on `cox-protocol` and `cox-permission`. The AGENTS.md trust list names the new crate.
Check: `cox-permission` depends only on `cox-protocol`.
Status: done 2026-09-26

What landed:
- `crates/cox-core/src/permission/{mod,policy,rules}.rs` moved with `git mv` to `crates/cox-permission/src/{lib,policy,rules}.rs`, tests included, no logic change. The engine uses only `cox_protocol` and globset, so it moved whole.
- `cox-core` re-exports it (`pub use cox_permission as permission;`), so `cox_core::permission::Engine` and `cox_core::{Engine, Outcome}` still resolve.
- globset left `cox-core/Cargo.toml`.
- `deps.rs`: `cox-permission` → `cox-protocol` only; `cox-core` may use `cox-protocol`, `cox-models`, `cox-permission`.
- Docs: AGENTS.md layout row and trust list, the SECURITY.md guard list, plan.md §1.1 row and dependency sentence.

Deviations:
- The `Engine::decide` doctest now imports `cox_permission::{Engine, Outcome}` and needs `serde_json` as a dev-dependency, because it compiles in its new crate.
- Done in worktree `_worktrees/cox-t32.8`, then cherry-picked onto main.

Check:
- `cargo tree -p cox-permission --edges normal` shows only `cox-protocol` among workspace crates.
- `cox-core/tests/permission.rs` and `policy_matrix.rs`: 58 passed, unchanged.

#### T32.16 `cox-config`: the one config owner

Depends: — · Moves: `crates/cox/src/config_load.rs`, `config_cmd.rs` (~990).
Why: size (c) and reuse (d). This one crate owns loading, validation, editing and the schema drift test.
Plan: `anyhow` becomes a `thiserror` enum; figment and toml_edit move with the files.
Check: the config-schema drift test lives in the new crate and passes; `cox config` and `cox doctor` behave the same against a `COX_HOME` scratch tree.
Status: done 2026-09-26

What landed:
- `crates/cox/src/config_load.rs` and `config_cmd.rs` moved with `git mv` to `crates/cox-config/src/load.rs` and `cmd.rs`: layering, project guards, provenance, `get`/`set`/`path`/`show_lines`. figment and toml_edit moved with them.
- `ConfigError` (thiserror: `Io`, `Json`, `InvalidToml`, `EmptyKey`, `NotATable`) replaces the anyhow in those files; the messages are the old texts, and crates/cox converts with `?`.
- crates/cox keeps what needs clap, cox-ext or cox-tui: the flag layer from `Cli`, the `.claude/settings.json` reader, `keymap()`, the printing, and a thin `load(cwd, cli)` wrapper. It re-exports the rest at the old `config_load`/`config_cmd` paths.
- New drift test `config_jsonschema_matches_committed_file` with the committed `docs/config.jsonschema`.
- `deps.rs`: `cox-config` → `cox-protocol` only. Docs: AGENTS.md layout row, plan.md §1.1 rows and dependency sentence.

Deviations:
- There was no config-schema drift test to move (done.md records that no committed `Config` schema existed), so the card's Check was met by adding one, per the AGENTS.md "Config files" rule.
- `load` takes the flag layer and a Claude-settings callback, and `show` became `show_lines`; that kept cox-config free of clap, cox-ext, cox-tui and printing. No logic change.
- `ENV_LOCK`/`temp_env` are `pub` behind a `test-util` feature so both crates share one helper.
- Done in worktree `_worktrees/cox-t32.16`, then cherry-picked onto main.

Check:
- `cox config` (show, `--sources`, get, path, set, errors, comment-preserving set, project guards) and `cox doctor`, `--json doctor`: the old and new binaries' output against scratch `COX_HOME` trees diff empty (591 lines each, exit codes included).
- `cargo nextest run -p cox-config`: 9 passed.

#### T30.26 One effort map, with `Effort::Medium`

Depends: T30.24 · Size: ~180 · Files: `cox-models`, `anthropic/request.rs`, `openai/responses.rs` (Chat's field is added in the same card if it fits, else a follow-up)
Goal: `effort_for(api, Effort, &caps)` in `cox-models` is the one mapping (item 4).
- Anthropic: `output_config.effort` plus adaptive thinking.
- Responses: `reasoning.effort`.
- Chat: `reasoning_effort`, only when the row declares it. Whether the OpenAI Chat API and LM Studio accept it is checked against their API references and recorded in R§4.3.3.
- Jev: explicitly `None`.

`Effort` gains `Medium`, and models.dev's `medium` maps to it.
Check:
- a table test over api × effort × caps;
- `clamp_effort` tests pass with `Medium`;
- request snapshots are unchanged except where `Medium` is new.
Status: done 2026-09-26
Result: `cox_models::effort_for(api, Effort, &Capabilities)` in `crates/cox-models/src/effort.rs` is the one mapping. Anthropic sends `output_config.effort`, plus adaptive thinking when `caps.adaptive_thinking`. Responses sends `reasoning.effort`. Chat sends `reasoning_effort` only when the `models` entry declares `reasoning_effort = true`. Jev sends nothing. `Effort::Medium` sits between Low and High; `cox-vendor` maps models.dev `medium` to it, and `/effort` accepts it.
Sources (checked 2026-09-26): the OpenAI Chat reference (https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create) lists `reasoning_effort` as optional and model-dependent. LM Studio's Chat Completions page (https://lmstudio.ai/docs/developer/openai-compat/chat-completions) does not list it. Both are recorded in R§4.3.3.
Check: nextest 934 passed, 3 skipped; clippy and fmt clean; `just vendor-test` 36 passed. Only the help-overlay snapshot changed (the `/effort` line); no request snapshot changed.
Not done: `default.toml` still has three-level effort sets. A live `cox-vendor models` run would add `medium` and also move one unrelated price (deepseek-v4-pro), so it stays a separate vendor refresh. `clamp_effort` still reads the section's `models` list, not `Catalog`.

#### T32.12 `cox-provider-http`: HTTP, retry, SSE and key resolution

Depends: — · Moves: `cox-provider/src/http.rs`, `retry.rs`, `sse.rs` (~500).
Why: reuse (d), shared by every wire.
Check: the retry and SSE tests pass unchanged.
Status: done 2026-09-26
Result: `crates/cox-provider-http` owns `http` (client, `resolve_key`, `resolve_key_with`), `retry` and `sse`, and depends only on `cox-protocol` among workspace crates (`deps.rs` rule). `cox-provider` re-exports all three at their old paths. `keyring`, `eventsource-stream` and `bytes` left `cox-provider`. `resolve_key_with` became `pub`, because `cox-provider`'s tests call it across the new crate boundary.
Check: the 14 moved `http`/`retry`/`sse` tests pass unchanged under the new crate. clippy and fmt are clean, and nextest ran 930 passed, 3 skipped in the worktree.

#### T32.11 `cox-provider-testkit`: scripted and replay providers

Depends: — · Moves: `cox-provider/src/scripted.rs`, `replay.rs` (~750).
Why: reuse (d). Every crate's tests use them without needing the real wires.
Check: each crate takes the testkit as a dev-dependency, or as a normal dependency where a production path uses it today (the card lists which).
Status: done 2026-09-26
Result: `crates/cox-provider-testkit` owns the pure scenario and cassette helpers: `parse_scenario`, `events_for`, `redact_secrets`, `cassette_hash`, `write_cassette` and `nearest_hint`. It depends only on `cox-protocol`. The `Scripted` and `Replay` structs and their `Provider` impls stay in `cox-provider` as thin glue, because they need `tokens::estimate`, `AnthropicStream` and `sse::parse_sse_str`. Every caller keeps its `cox_provider::scripted` or `cox_provider::replay` path.
Users: `cox-provider` depends on the testkit normally, because `from_env()` builds Scripted/Replay from `COX_PROVIDER` at runtime. `cox record` uses `write_cassette` through `cox-provider`. Every other caller uses it only from tests.
Check: nextest ran 932 passed, 3 skipped in the worktree, including two new tests for the functions that became `pub`.

#### T32.5 `cox-search`: grep and glob

Depends: T32.3 · Moves: `cox-tools/src/grep.rs`, `glob.rs` (~870).
Why: dependencies (a), namely ignore, grep-searcher, grep-regex and nucleo.
Check: those four crates appear only in `cox-search/Cargo.toml`. If another tool still uses one of them, the card says so and leaves that dependency shared.
Status: done 2026-09-26
Result: `crates/cox-search` owns the pure grep and glob engines: `grep::search` and `glob::find`, plus `rank_by_query` and `workspace_files`, with a small `thiserror` enum per engine. It is a pure leaf with no workspace dependency. `GrepTool` and `GlobTool` stay in `cox-tools`, because they run `path::confine` and the archive. `ignore`, `grep-searcher`, `grep-regex`, `globset` and `nucleo` left `cox-tools`; `nucleo` is still also used by `cox-tui`'s picker.
Check: the grep golden tests and the glob tests pass unchanged; clippy and fmt are clean; nextest ran 930 passed, 3 skipped in the worktree.

#### T32.9 `cox-telemetry`: tracing setup and the OpenTelemetry stack

Depends: — · Moves: `crates/cox/src/telemetry.rs`.
Why: dependencies (a), five opentelemetry crates.
Plan: the `otel` feature moves with it; `cox`'s `otel` forwards to it. Errors become a `thiserror` enum, because `anyhow` stays in `crates/cox` only.
Check: builds with `--no-default-features` and with defaults are both green.
Status: done 2026-09-26
Result: `crates/cox-telemetry` owns tracing setup and the OpenTelemetry stack behind its `otel` feature; `cox`'s `otel` forwards to it. Errors are `TelemetryError` (`thiserror`). `init` takes the log level, the otel switch and the endpoint instead of `&Config`, so the crate depends on no workspace crate. `crates/cox/src/telemetry.rs` re-exports it.
Check: `cargo build -p cox --no-default-features` and the default build are both green. nextest ran 930 passed, 3 skipped in the worktree.

#### T32.7 `cox-web`: `web_fetch`

Depends: — · Moves: `cox-tools/src/web_fetch.rs`.
Why: dependencies (a), so reqwest leaves `cox-tools`.
Check: there is no `reqwest` in `cox-tools/Cargo.toml`.
Status: done 2026-09-26
Result: `crates/cox-web` owns the fetch and HTML-to-text engine: `client`, a streaming `fetch` with cancellation and a byte cap, and `extract`. It depends only on `cox-protocol`. `WebFetchTool` stays in `cox-tools` (it uses `ToolCx` and `write::str_field`), and its output is unchanged.
Check: `reqwest` no longer appears in `cox-tools/Cargo.toml`. The `web_fetch` integration tests pass unchanged. nextest ran 930 passed, 3 skipped in the worktree.

#### T33.1 `cox-plugin-api`: the manifest and its schema — blocker

Depends: — · Size: ~180 · Files: `crates/cox-plugin-api/src/lib.rs`, `src/manifest.rs`, `crates/cox-protocol/src/lib.rs` (re-export)
Goal: `plugin.toml` parses into typed `PluginManifest`/`Capabilities`/`Limits`/`ProviderDecl`/`ModelDecl`/`McpDecl` with `deny_unknown_fields`. `docs/plugin.schema.json` is generated and drift-tested.
Plan:
1. New pure crate (serde, serde_json, schemars). Add a `deps.rs` rule: no workspace dependency.
2. Types and validation per PL§2: id regex, name lengths after prefixing, `net` is hosts not URLs, `fs` roots.
3. `cox_protocol::plugin` re-export.
4. Drift test modelled on `protocol_jsonschema_matches_committed_file`.
Check: `manifest_rejects_unknown_keys`, `manifest_rejects_net_url`, `manifest_rejects_id_with_double_underscore`, `plugin_schema_matches_committed_file`; `cargo build -p cox-plugin-api --target wasm32-unknown-unknown` succeeds.
Status: done 2026-09-26
Result: `crates/cox-plugin-api` parses `plugin.toml` into `PluginManifest`, `Capabilities`, `Limits`, `ProviderDecl`, `ModelDecl`, `PriceDecl` and `McpDecl`, all with `deny_unknown_fields`. `ModelTier` allows only `cheap` and `code`, so a manifest cannot request `think`. `validate()` applies PL§2:
- `api` major is 1;
- the id matches `^[a-z][a-z0-9-]{1,23}$`, so it can never contain `__`;
- prefixed tool and MCP names fit in 64 characters;
- `net` entries are host patterns only;
- `fs` roots stay inside `$WORKSPACE` or `$PLUGIN_DATA`;
- only the allowed render targets are accepted.
The id-matches-directory check is left to the loader (T33.4). `docs/plugin.schema.json` is generated and drift-tested. `cox-protocol` re-exports the crate as `plugin`; `deps.rs` lets `cox-protocol` depend on `cox-plugin-api` only, and `cox-plugin-api` on no workspace crate. It builds for `wasm32-unknown-unknown`.
Check: `manifest_rejects_unknown_keys`, `manifest_rejects_net_url`, `manifest_rejects_id_with_double_underscore` and `plugin_schema_matches_committed_file` pass. nextest ran 951 passed, 3 skipped in the worktree.

#### T32.13 `cox-provider-anthropic`

Depends: T32.12, T30.21–T30.26 (A46), so the wire moves once, already unified.
Moves: `cox-provider/src/anthropic/*`, `schema/`, `build.rs` (~2.1k).
Why: dependencies (a) (the typify build step) and size (c).
Check: the request snapshots are unchanged; the typify build runs only for this crate.
Status: done 2026-09-26
Result: `crates/cox-provider-anthropic` owns the Anthropic Messages wire (`request`, `stream`, `wire`), its `build.rs` and the vendored `schema/`, so the typify build step runs only for it. It depends on `cox-protocol`, `cox-models` and `cox-provider-http`. `cox-provider` re-exports it as `anthropic` and has no build-dependencies left. `cox-vendor anthropic-spec` writes to the new schema directory, and a new test checks that its default target is the file `build.rs` reads. The 8 insta snapshots moved byte-identical; only their file names follow the new module path.
Check: the request snapshots are unchanged. `cargo tree -e build -i typify` shows typify only under this crate, whose `build.rs` is the only one in the workspace. nextest ran 940 passed, 3 skipped in the worktree, and `just vendor-test` ran 37 passed.

#### T32.14 `cox-provider-openai`

Depends: T32.12, T30.21–T30.26 as in T32.13.
Moves: `cox-provider/src/openai/*` (~2.2k).
Why: dependencies (a) (async-openai) and size (c).
Check: `async-openai` appears only in this crate's `Cargo.toml`.
Status: done 2026-09-26
Result: `crates/cox-provider-openai` owns the OpenAI Responses and Chat wires (`chat`, `responses`, `wire`) and depends on `cox-protocol`, `cox-models` and `cox-provider-http`. `cox-provider` re-exports it as `openai`. The 11 insta snapshots moved with their tests byte-identical; only their file names follow the new module path.
Check: `async-openai` appears only in `crates/cox-provider-openai/Cargo.toml` among crates; `cargo tree -i async-openai` shows the single path through it. nextest ran 940 passed, 3 skipped in the worktree, with no `.snap.new`.

#### T33.2 ABI v1 payload types — blocker

Depends: T33.1 · Size: ~170 · Files: `crates/cox-plugin-api/src/abi.rs`, `src/lib.rs`
Goal: every export and host-function payload in PL§4 (`InitIn/InitOut`, `EventBatch`, `Effects`, `HookCall`, `ToolCallIn`, `CommandIn/CommandOut`, `RenderIn`, `RenderItemIn`, `ProviderCall`, `ModelCall`, `HttpReq/HttpResp`, `Question/Advice`, `AbiError`) as `JsonSchema` types, with `docs/plugin-abi.schema.json` drift-tested.
Plan: reuse the `cox-protocol` types that cross the ABI by referencing them in the schema, not copying them. Because `cox-plugin-api` must not depend on `cox-protocol` (T33.1 rule), those fields are `serde_json::Value` in the api crate, and `cox-plugin` converts them into the typed protocol values at the boundary. Say this in the module header. `CommandOut` is the closed enum from PL§4.
Check: `abi_schema_matches_committed_file`; `command_out_has_no_submission_variant` (a serde round-trip of every variant); `unknown_fields_are_ignored_both_ways`.
Status: done 2026-09-26
Result: `cox-plugin-api::abi` holds every payload of PL§4:
- init: `InitIn`/`InitOut`, `SessionInfo`, `CommandDecl`, `KeyDecl`, `Slot`;
- `EventBatch`, `Effects`;
- calls: `HookCall`, `ToolCallIn`, `CommandIn`/`CommandOut`, `RenderIn`, `RenderItemIn`, `ProviderCall`, `ModelCall`, `HttpReq`/`HttpResp`;
- advice: `Question`/`Advice`/`Answer`;
- `AbiError`.

Fields that carry cox-protocol types are `serde_json::Value`, marked in the schema with `x-cox-protocol`. No payload denies unknown fields, so `InitIn.granted` is a `Value`. `CommandOut` is closed: prompt, compact, toggle_panel, open_overlay, notice or nothing, with no Submission variant. The two-phase decide types (`DecideOut`, `cox_decide_resume`) stay with T33.40.1. `docs/plugin-abi.schema.json` is generated and drift-tested.
Check: `abi_schema_matches_committed_file`, `command_out_has_no_submission_variant` and `unknown_fields_are_ignored_both_ways` pass, and the crate still builds for wasm32. nextest ran 954 passed, 3 skipped in the worktree; on main `-p cox-plugin-api -p cox-protocol` ran 75 passed.

#### T33.5 Grants and kv in `cox-store` — blocker

Depends: T33.1 · Size: ~190 · Files: `crates/cox-store/migrations/00000000000004_plugins/{up,down}.sql`, `crates/cox-store/src/models.rs`, `crates/cox-store/src/lib.rs` (+ `schema.rs`, generated)
Goal: the `plugin_grants` and `plugin_kv` tables per PL§3 through Diesel's typed DSL, and a `PluginStore` trait in `cox-protocol` implemented by `Store`. The kv quota is enforced in the store.
Check: `grant_rows_are_per_digest`, `kv_quota_rejects_oversize_value`, `kv_delete_all_removes_only_that_plugin`; `deps.rs` still has diesel only in `cox-store`.
Status: done 2026-09-26
Result: migration `00000000000004_plugins` adds `plugin_grants`, keyed by `plugin_id`, `scope` and `digest`, and `plugin_kv`, keyed by `plugin_id` and `key` (PL§3). `PluginStore`, `PluginGrant` and `GrantScope` (`User | Project(root)`) sit in `cox-protocol` next to `Store`. Granted capabilities and source stay opaque JSON until T33.6. `cox-store` implements the trait with Diesel's typed DSL. The kv quota is 64 KiB per value and 1 MiB per plugin; going over it returns the new `StoreError::QuotaExceeded`. `schema.rs` is still hand-written, and the schema snapshot now includes the two tables.
Deviation: the card's Files line did not list `cox-protocol`, but PL§3 puts the trait there.
Check: `grant_rows_are_per_digest`, `kv_quota_rejects_oversize_value` and `kv_delete_all_removes_only_that_plugin` pass. nextest ran 954 passed, 3 skipped in the worktree.

#### T30.15 LM Studio provider: the chat loop over `/v1/messages`

Depends: T30.21–T30.23 (the section is one more `Transport` table; A46) · Size: ~150
Goal: `--provider lmstudio` works with no hand-written config: cox talks to LM Studio's Anthropic-compatible `/v1/messages` through the existing Anthropic provider, with LM Studio's optional auth. Evidence in R§4.3.2: the native `/api/v1/chat` takes no custom tool schemas, and cox's OpenAI Chat path drops tool calls, so Messages is the one working chat transport; T30.14 already ran a one-tool task over it at $0.
Plan:
1. `cox-protocol` config: a built-in `[providers.lmstudio]` (`base_url` default `http://localhost:1234`, `api_key_env` default `LM_API_TOKEN`, `model`, `context_window` where `0` means "ask the server" (T30.16), `timeout_s`, `max_retries`); regenerate the committed schema so the drift test passes.
2. `cox-provider/src/anthropic/mod.rs`: a constructor taking an optional key — no key sends no auth header (LM Studio without "Require Authentication"); a key goes out as `x-api-key`, which LM Studio accepts on this path. Reuse the request, stream and retry code as is; no new wire types.
3. `crates/cox/src/session.rs`: `"lmstudio"` in `backend_for`, reading the key from `api_key_env` when set, never from the Anthropic keyring.
4. Tests: wiremock — no key means no `x-api-key`; `LM_API_TOKEN` set means it is sent; a tool-call stream captured from the live server (fixture in `cox-provider/tests/fixtures/`) parses into `ToolUseStart` … `ToolUseEnd`.
Check: `COX_HOME=<scratch> cox run -p "<one-tool task>" --provider lmstudio --tier code=prism-ml/bonsai-27b` finishes with `exit_code` 0 and a `usage` ledger row at $0; the three standard commands clean.
Done when: `--provider lmstudio` runs a tool loop against a live LM Studio with only `tiers.code.model` set.
Out of scope: the native API (T30.16); fixing `openai/chat.rs` (ideas.md).
Status: done 2026-09-26
Result: new `[providers.lmstudio]` section (`LmStudioProviderConfig`: `base_url` `http://localhost:1234`, `api_key_env` `LM_API_TOKEN`, `model`, `context_window`, timeout and retries). Chat runs over LM Studio's Anthropic-compatible `/v1/messages` through the existing `AnthropicProvider` (R§4.3.2: the native `/api/v1/chat` has no tool schemas). `AnthropicProvider.api_key` is now `Option<String>`, so a keyless section sends no `x-api-key`. The key resolves under `lmstudio`, never the Anthropic keyring entry. The router buckets `lmstudio` as `ProviderId::Anthropic`, which matches what the built provider reports to the ledger. An empty `model` falls through to `tiers.code.model`. `context_window` goes: configured value → catalog → 32,768 floor (the live query is T30.16). `cox doctor` treats the key as optional.
Deviation: the live-server Check was not run, because no LM Studio instance was available. Replaced by `stream_sends_no_x_api_key_header_without_a_key`, `stream_sends_x_api_key_header_with_a_key` (wiremock), `backend_for_lmstudio_builds_keyed_and_keyless`, `router_lmstudio_maps_to_anthropic_and_pins_on_tier_model_alone` and `lmstudio_provider_defaults`, plus a scratch-`COX_HOME` run of the real binary: `cox doctor` warns about the missing optional key, and `cox run -p … --provider lmstudio` reaches the outbound request to `localhost:1234/v1/messages`.
Check: nextest ran 945 passed, 3 skipped in the worktree. clippy and fmt are clean.

#### T34.0 Design doc: subagent messaging

Depends: — · Size: design only (≤ 1-page doc + falsifiers) · Files: `docs/design/subagent-messaging.md`
Goal (D15): before any protocol change, the ≤ 1-page doc the creator's own rule requires — the problem in one measurable number (e.g. "0 of N running or finished subagents can receive a follow-up today"), what Claude Code (`SendMessage`, gated behind agent teams and off by default), Codex CLI (one-shot result only, no messaging) and OpenCode (undocumented) do (research.md §4.3.7), what cox will do and why it is at least as good without becoming "agent teams" (`ideas.md`, still creator-unapproved), and what would falsify the design. Written by the `code` tier, reviewed by `think` (D15).
Plan — the doc must answer, concretely enough for T34.4–T34.9 to implement without re-deciding:
1. **Shape of the message.** A new `Submission` variant addressed by `TaskId` (the model only ever sees a `TaskId`, per the existing `TaskCreated`/`TaskCompleted` events) — e.g. `Submission::TaskMessage { task: TaskId, from: Option<TaskId>, text: String }` — and its matching delivery `Event`.
2. **Running vs. finished.** A follow-up to a *running* child queues as a second `Submission::UserTurn` after its current turn finishes — never mid-turn (D2: one `Submission` stream per session, in order). A follow-up to a *finished* child resumes it through the existing `Session::resume`, preserving its `parent_id`/budget-slice relationship — not a new resume mechanism.
3. **Child → parent.** Progress and questions reach the parent the same way `TaskCompleted` already does: a pointer line entered into the *parent's* history after the last cache breakpoint (D6e), never mid-turn context surgery; `ask_user` (T34.3) stays the channel for a question that must block the child.
4. **Sibling routing always through the parent.** A sibling never gets a direct channel to another sibling; it addresses one by name/`TaskId` and the parent session relays it, exactly like `relay_approval` already relays approvals — every session stays a pure `Submission`-in/`Event`-out state machine (D2), no exceptions.
5. **The model-facing tool.** `send_message { to: "parent" | <task name/id>, text }`, available to a subagent (to reach its parent or a named sibling) and to the parent (to reach a named child).
6. **How the recipient sees it.** A pointer line after the last cache breakpoint (D6e / cache-stable prefix) — the same shape as today's `TaskCompleted` notice, never spliced into the middle of an in-flight turn's context.
7. **Flood and loop guards.** A per-task message cap and a hop limit, so an A→B→A ping-pong stops on its own — reusing T34.2's concurrency cap rather than inventing a second one.
8. **Trust.** The permission engine (`cox_permission::Engine`) stays the single guard on every tool call a message might provoke — a message itself never grants a tool; `cox_sanitize::sanitize` runs on every rendered line, the same as any other tool output (D14).
9. **Surfaces.** How the TUI transcript and `/agents` overlay, `stream-json` and ACP each render a delivered message (A29 still applies: no new richer live-progress event beyond the narrow `/agents` card).
10. **Out of scope.** Persistent teammates that outlive their task, split-pane processes, a shared task board, and a sibling roster injected into every subagent's system prompt (it would break the cache-stable prefix). A child addresses a sibling by the name or `TaskId` its parent gave it in the task text. This doc's protocol scope is the one new `Submission` variant and its matching `Event`, nothing wider.
Check: the doc exists, is ≤ 1 page, and states falsifiers; reviewed (not written) by `think` per D15.
Status: done 2026-09-26
Result: `docs/design/subagent-messaging.md` (SM).
- Wire: `Submission::TaskMessage { task, from, hop, text }` and a matching `Event::TaskMessage`, submitted to and emitted by the parent session only (D2).
- Registry: it keeps a live child handle while the child runs. On completion that becomes the child's `SessionId`, `job`, `tier` and `parent_id`, so a finished child can be resumed.
- Siblings: a sibling message is relayed by the parent's `run_task` loop, the same match that `relay_approval` uses. There is no second channel.
- Tool: `send_message { to, text }`, where `to` is `"parent"`, a sibling's name, or a `TaskId`.
- Guards: `MAX_MESSAGES_PER_TASK = 16`, and a causal `MAX_HOPS = 4`. A message inherits the hop of the turn it is sent from, plus one, so an A→B→A→B ping-pong stops while independent sibling messages are never throttled.
- Delivery: always a pointer line after the last cache breakpoint (D6e), sanitized, and labelled like `ApprovalRequired`'s `Source`.
Review (D15): reviewed by the orchestrator (claude-opus-5-5). The draft reused `core.max_concurrent_subagents` as both the message cap and a session-wide hop counter; that would throttle unrelated sibling messages, so it was replaced with the two constants and the causal hop above.
Plan follow-up: T34.5's Files now include `crates/cox-core/src/session.rs`, because `Session::resume` needs a parent, job and tier.
Check: the doc exists, is about one page (104 lines), and states falsifiers.

#### T35.0 Design doc: external agents from plugins

Depends: — · Size: design only (≤ 1-page doc + falsifiers) · Files: `docs/design/external-agents.md`
Goal (D15): before any protocol or manifest change, the ≤ 1-page doc the creator's own rule requires, covering Cursor as the first case (research.md §4.3.8) without over-fitting the design to it. Written by the `code` tier, reviewed by `think` (D15).
Plan — the doc must answer, concretely enough for T35.1–T35.9 to implement without re-deciding:
1. **The manifest capability.** A new `[[external_agents]]` table in `plugin.toml` (PL§2): `name`, `command`, `args`, `mode = "acp" | "stream-json"`, `key_env`. Validated the same way `[[mcp]]` is (PL§2's validation list): name fits the tool-name rule after prefixing, an in-package command is covered by the digest, a PATH program is shown verbatim at approval.
2. **Who spawns it.** The host, never a WASM guest (a guest cannot open a process) — the same split T33.19/T33.42 already made for plugin-shipped MCP stdio servers: `crates/cox-plugin` resolves and validates the command, `crates/cox` wraps it with `sandbox::Policy` before it is spawned, and the capability is one more line in the grant dialog (T33.6's `Verdict`), never auto-granted.
3. **How it reaches the model.** Through P34's own path: a granted `external_agents` entry registers as a discovered `AgentDef` (T34.1's `cox_ext::agents::discover` mechanism, or a sibling of it) so `agent(preset: "cursor")` dispatches it exactly like a `.cox/agents/*.md` definition; a follow-up or a sibling message reaches it through T34.5's existing parent-routing, not a second messaging path.
4. **ACP mode.** cox is the ACP **client** for once, not the server: reuse the workspace `agent-client-protocol` crate `crates/cox-acp` already depends on. `session/request_permission` from the external agent is decided by `cox_permission::Engine` — the one guard, never a second permission path. An `fs/*` or `terminal/*` request from the agent is served only through `path::confine` and the sandbox policy, or refused.
5. **stream-json mode.** Cursor CLI's `agent -p --output-format stream-json` line shapes (`system`/`user`/`assistant`/`tool_call{started,completed}`/`result`, research.md §4.3.8) map onto cox's own `Event`/`Item` enum. **Recommendation: host-side, keyed by the manifest's declared `mode`, not a WASM guest export.** The shape is a fixed, documented, largely stable dialect (D4: adopt existing formats verbatim) that more than one external agent is likely to share; parsing JSON lines into `cox_protocol::Event` is boilerplate, not plugin-specific logic, and doing it in wasm would need a new `cox_emit`-shaped host function only for this one capability (PL§7a already defers streaming ABI providers for the same reason). A host-side mapper also lets T35.7's fake-binary e2e test the mapping without a wasm toolchain. An unrecognised line becomes a sanitized `Notice`, never a hard error (D14).
6. **Cost.** `cox_model_call`'s rule ("every request has a usage row") still holds, but Cursor's CLI event shapes captured in research.md §4.3.8 carry no token counts in `assistant`/`result`, and ACP's `session/update` has none either. **Recommendation: write the usage row at $0 with a `billed_externally: true` marker by default**, and use reported tokens only if a future event ever carries them — never skip the row, and never estimate a token count for spend that lands on the user's own Cursor plan, not cox's ledger.
7. **Trust.** Every rendered line goes through `cox_sanitize::sanitize` (D14) — an external agent's output is exactly as untrusted as an MCP tool result. Fail-open (D14): a missing CLI binary or an unset `key_env` is one `Notice(Warn)` at session open, and the preset is left out of the `agent` tool's names, the same shape T35.8's `cox doctor` row reports.
8. **Hard rule, not a preference (creator decision, A54).** Only the dashboard-issued API key, resolved with `resolve_key(key_env, <section>)` like any other provider key (never read from a test's real keychain, D12/A49), and only the official CLI/ACP surfaces. Never the desktop app's session, never a reverse-engineered proxy (research.md §4.3.8 catalogs several; none is used).
9. **Falsifiers.** What would prove this design wrong — e.g., a Cursor CLI release that removes `--output-format stream-json` or `acp` from `agent`'s documented surface, or an ACP `session/update` that turns out to need a richer permission shape than `cox_permission::Engine` already offers.
Check: the doc exists, is ≤ 1 page, and states falsifiers; reviewed (not written) by `think` per D15.
Status: done 2026-09-26
Result: `docs/design/external-agents.md` (EA).
- Manifest: `[[external_agents]]` (`name`, `command`, `args`, `mode = "acp" | "stream-json"`, `key_env`), with one grant line.
- Spawn: the host spawns the agent, under the same sandbox wrap as MCP stdio servers.
- ACP client: `session/request_permission` goes to `cox_permission::Engine::decide`; `fs/*` and `terminal/*` requests go through `path::confine` and the sandbox, or are refused.
- stream-json: a host-side table maps its events to cox `Event`/`Item`, keyed by the manifest's `mode`. There is no guest export for it.
- Cost: a usage row of $0 marked `billed_externally`.
- Safety: output is sanitized; a missing CLI or key only warns and hides the preset; `cox doctor` gets a row.
- Official paths only (A54).
- Three falsifiers.
- The workspace `agent-client-protocol` (lock 2.1.0) ships the client side (`AcpAgent`, `Client.builder()…connect_with`), so T35.3 needs no new dependency.
Review (D15): reviewed by the orchestrator (claude-opus-5-5). No card changes.
Deviation: at 136 lines the doc runs a little past one page.
Check: the doc exists and states falsifiers.

#### T33.3 `cox-plugin`: host crate, one worker per plugin — blocker

Depends: T33.2 · Size: ~200 · Files: `crates/cox-plugin/src/lib.rs`, `src/host.rs`, `src/error.rs`
Goal: load a module from bytes with extism (`default-features = false`) and call `cox_init` on a worker thread that owns the `Plugin`. Memory cap, per-call deadline via `CancelHandle`, and `PluginError` (thiserror) mapped from `extism::Error`.
Plan:
1. Add the `§1.1` row and the commit reason from A52.
2. `deps.rs` rule: only `cox-plugin` depends on `extism`.
3. Two queues: control first, then events (PL§4).
4. Tests with inline WAT: an echo `cox_init`, an infinite loop, a `memory.grow` past the cap, a missing export.
5. Before and after: record the release size with `scripts/footprint.sh` and a clean build with `cargo build --timings` in R§4.3.5 P25, then apply falsifier 1 of PL§12.
Check: `wat_plugin_init_round_trips_json`, `runaway_call_is_cancelled_at_deadline`, `memory_cap_traps_not_panics`, `missing_optional_export_is_absent_not_error`, `http_request_is_compiled_out` (a WAT guest calling extism's `http_request` gets an error); R§4.3.5 P25 filled.
Status: done 2026-09-26
Result: new crate `crates/cox-plugin`.
- `PluginHost::load` takes module bytes, either binary or WAT; extism's loader accepts WAT, so no `wat` crate is needed.
- Memory is capped by `memory_mib`, clamped to 64. The manifest's `timeout_ms` is the outer per-call cap.
- WASI and the compilation cache are off.
- Each plugin gets one worker thread that owns its `Plugin` and serves a control queue (depth 16) before an event queue (depth 256). A watchdog thread enforces each call's deadline through `CancelHandle`.
- A missing optional export returns `Ok(None)`. `cox_init` is required.
- `PluginError` is a thiserror enum: `Timeout`, `OutOfMemory`, `Trap`.
- `only_plugin_depends_on_extism` in `crates/cox/tests/deps.rs` checks that only `cox-plugin` depends on extism or wasmtime. `cox-plugin` itself may depend only on `cox-protocol`, `cox-plugin-api` and `cox-sanitize`.
- `deny.toml` allows `Apache-2.0 WITH LLVM-exception`.
Falsifiers (A55):
- Size (PL§12 falsifier 1): the binary grows by 16.8 MiB, from 51.2 to 68.0 MiB. The budget is raised to 20 MiB. The `plugins` feature on `crates/cox` is on by default and gates `cox-plugin`; `slim_build_has_no_wasm_runtime` proves a build without it pulls in no WASM runtime.
- Advisories (falsifier 4): RUSTSEC-2026-0222 and RUSTSEC-2026-0269 on wasmtime 43 are ignored in `deny.toml` with a 2026-12-31 review. WASI stays off. The fix is tracked as T33.43.
Build time: clean release build measured under load (load average 36–47). Wall time 159 → 139 s is noise at that load. Summed CPU time went from 1015 to 1478 s. Recorded in research.md P25.
Deviation: `default-features = false` alone does not build extism 1.30 (research.md P38), so `wasmtime` 43 is also declared directly, only to turn on its `anyhow` feature. The card asked for about 200 lines; the crate is about 335 lines plus tests.
Check: `wat_plugin_init_round_trips_json`, `runaway_call_is_cancelled_at_deadline`, `memory_cap_traps_not_panics`, `missing_optional_export_is_absent_not_error`, `http_request_is_compiled_out` and `control_queue_is_served_before_events` pass. `cargo deny check` passes. Clippy is clean, including for a build without the `plugins` feature. nextest: 962 passed, 3 skipped in the worktree.

#### T34.4 Protocol types for task messaging

Depends: T34.0 · Size: ~120 · Files: `crates/cox-protocol/src/types.rs` (`Submission::TaskMessage`, a matching `Event`), `docs/protocol.jsonschema` (regenerated by its own drift test)
Goal: exactly the `Submission`/`Event` shape T34.0 specifies — a message addressed by `TaskId`, carrying who it is from (the parent, or a named sibling `TaskId`) and its text — with round-trip serde tests, so T34.5 has a stable wire shape to route.
Check: `task_message_round_trips_through_serde`, `protocol_jsonschema_matches_committed_file` stays green with the new variants.
Status: done 2026-09-26
Result: `Submission::TaskMessage { task, from, hop, text }` and `Event::TaskMessage` with the same fields (SM§1). `docs/protocol.jsonschema` is regenerated; the diff only adds lines. The new variant is handled explicitly as a no-op where later cards will pick it up: `Session::submit` (T34.5), the child-event relay in `run_task` (T34.5) and the TUI `on_event` (T34.7). stream-json already passes every event through serde, so `TaskMessage` needs no change there. ACP still ignores it through a wildcard until T34.8.
Check: `task_message_round_trips_through_serde` passes, as do the new rstest cases in `event_json_roundtrip`/`submission_json_roundtrip` and `protocol_jsonschema_matches_committed_file`. nextest: 973 passed, 3 skipped in the worktree. clippy and fmt are clean.

#### T35.1 Manifest capability types and schema drift

Depends: T35.0 · Size: ~150 · Files: `crates/cox-plugin-api/src/manifest.rs`, `docs/plugin.schema.json` (regenerated by its own drift test)
Goal: `[[external_agents]]` as its own struct (`name`, `command`, `args`, `mode: AcpOrStreamJson`, `key_env`) in `cox-plugin-api::manifest` (PL§2), validated alongside the existing `[[mcp]]` rules (name fits the tool-name rule after prefixing, `command` inside the package is covered by the digest, a PATH program is flagged for verbatim display at approval); `deny_unknown_fields` applies as it already does for the rest of the manifest.
Check: `external_agent_entry_round_trips_through_toml`, `unknown_mode_value_is_a_validation_error`, `plugin_schema_json_matches_committed_file` stays green with the new table.
Status: done 2026-09-26
Result: `ExternalAgentDecl { name, command, args, mode, key_env }` and `AgentMode` (`acp` | `stream-json`) in `cox-plugin-api`, plus `PluginManifest.external_agents`. It follows the same conventions as `McpDecl` and `ProviderDecl`: `deny_unknown_fields`, and the name is checked after the plugin-id prefix, the same way `[[mcp]]` is. `key_env` must be an env var name (`[A-Za-z_][A-Za-z0-9_]*`); anything else is rejected with the new `ManifestError::KeyEnv`. `docs/plugin.schema.json` is regenerated. PL§2 has no field table, so it is unchanged; §10 already points to EA.
Deviation: the card calls the drift test `plugin_schema_json_matches_committed_file`, but its real name is `plugin_schema_matches_committed_file`.
Check: `external_agent_entry_round_trips_through_toml`, `unknown_mode_value_is_a_validation_error`, `manifest_rejects_external_agent_key_env_that_is_not_an_identifier`, `manifest_rejects_external_agent_name_too_long_after_prefix` and `plugin_schema_matches_committed_file` pass. nextest: 974 passed, 3 skipped in the worktree; the PTY e2e flakes from load passed on rerun.

#### T33.22 Widget tree and its terminal rendering

Depends: T33.2 · Size: ~190 · Files: `crates/cox-plugin-api/src/ui.rs`, `crates/cox-tui/src/plugin_ui.rs`
Goal: the closed `Widget` tree with `StyleToken`s naming `Theme` fields, and its conversion to ratatui lines with every string through `sanitize_with`. Limits: 512 nodes, depth 8, 16 KiB of text.
Check: insta snapshots of every widget kind under two themes and `NO_COLOR`; `widget_text_is_sanitized` (an escape sequence and a bidi override are stripped); `oversize_widget_renders_placeholder`.
Status: done 2026-09-26
Result: `cox-plugin-api::ui` holds the `Widget` tree, a closed snake_case-tagged enum (PL§8). A `Span` is `{text, style, bold, italic}`. The caps are 512 nodes, depth 8 and 16 KiB of text, checked by `Widget::within_limits()`. The crate stays wasm32-clean.
`StyleToken` covers 14 theme fields. The permission-mode colours are left out on purpose, so a plugin cannot imitate that trust signal.
`cox-tui::plugin_ui::render` draws with ratatui widgets: Paragraph, List, Table, LineGauge, Layout and Block. It reaches the types through `cox_protocol::plugin`. Every string goes through the existing sanitize guard, and newlines and tabs are flattened. A tree over the caps renders as one dim placeholder line. The gauge ratio is clamped to 0..=1, because `LineGauge` panics outside that range.
`docs/plugin-abi.schema.json` is regenerated.
Deviations:
- The card asked for about 200 lines; this is about 264 lines of code plus tests.
- `render` draws into a `Buffer`/`Rect` rather than converting to lines, because stacks, tables and blocks need layout that lines cannot express.
Check: snapshots of every widget kind under the dark, light and NO_COLOR themes, plus `widget_text_is_sanitized`, `oversize_widget_renders_placeholder` and `widget_json_uses_snake_case_tags_and_span_defaults`. nextest: 974 passed, 3 skipped in the worktree. clippy and fmt are clean.

#### T30.16 LM Studio native API: loaded context, capabilities, load on demand

Depends: T30.15, T30.24–T30.25 (the loaded context is a catalog override; A46) · Size: ~180
Goal: cox asks LM Studio what it is actually running instead of trusting config: the loaded context length (what compaction must fit — R§4.3.2 found `lms load --context-length 65536` left 251 648 loaded), whether the model was trained for tool use, and whether it is loaded at all; and loads it with the configured context length when it is not.
Plan:
1. `cox-provider/src/lmstudio.rs`: hand-written serde types (D3/A40 step 3: no Rust SDK, no published spec) for the subset of `GET /api/v1/models` cox reads (`key`, `max_context_length`, `loaded_instances[].config.context_length`, `capabilities.trained_for_tool_use`, `capabilities.reasoning.allowed_options`) and for `POST /api/v1/models/load` (`model`, `context_length`) with its response; unknown fields ignored. A fixture captured from the live server.
2. Session open for `lmstudio` (in `crates/cox`, not the core): read the model list; the loaded instance's context length becomes the context window unless `context_window` is set; not loaded and `load = true` in config → `models/load` with `context_length`; a model without `trained_for_tool_use` gets one `Notice(Warn)`; an unreachable server fails as a transport error, not a panic.
3. `cox doctor`: an LM Studio row — reachable, model loaded, loaded vs max context, tool-use capability.
4. Tests: wiremock contract tests over the fixture (loaded, not loaded, load call body, server down); a doctor snapshot.
Check: against the live server, `cox doctor` shows the loaded context 251 648 for `prism-ml/bonsai-27b`, and a session's compaction threshold follows it; the three standard commands clean.
Done when: a `--provider lmstudio` session uses the server-reported context window and `cox doctor` reports the model's state.
Out of scope: `/api/v1/chat` as a chat transport (no custom tools); per-response `stats` (tokens/s, time to first token) in the ledger — `/v1/messages` does not return them; MCP `integrations`.
Status: done 2026-09-26
Result: `crates/cox-provider/src/lmstudio.rs` talks to LM Studio's native `/api/v1` endpoints: `GET /api/v1/models` and `POST /api/v1/models/load`, with a bearer token when a key exists. The serde types ignore unknown fields.
- `LmStudio::prepare` reads the model's state. Only with the new `[providers.lmstudio] load = true` (default false) does it load the model, passing `context_window` as `context_length`; it then reads the allocated context back.
- `session::open` in `crates/cox` queries the server before it builds the provider and resolves the key once for both uses. The core only receives the number.
- `Catalog::overlay_served` makes the server's report the last catalog layer (A46). Context lookup order: configured value → loaded context → catalog → 32,768.
- A model without tool use gets a `Notice(Warn)` through the new `Session::notice`, and so does a model that is not loaded.
- An unreachable server or a model the server does not list fails session open with a clear error.
- `cox doctor` gets an LM Studio row when the code tier uses `lmstudio`. It has a 5 s timeout and never loads a model.
- `fixtures/lmstudio/models.json` is a live capture; R§4.3.2 gets two rows.
Deviations:
- About 14 files instead of the 3-file limit, including a small `Session::notice` API in cox-core.
- `cox acp` still uses the catalog fallback, because it builds its provider synchronously.
- The warning for an unloaded model was not asked for by the card.
Check:
- Live, real binary, scratch `COX_HOME`: `cox doctor` → "LM Studio: ✓ http://localhost:1234 reachable; `prism-ml/bonsai-27b` loaded, context 251648 loaded / 262144 max; tool use: yes".
- With `compact_at = 0.01`, the pre-call guard reported "over compact_at 0.01 × max_context 251648".
- A "say hi" run exited with code 0: 5666 input tokens, $0 usage row.
- Load on demand is tested only against wiremock; no model was loaded or unloaded on the live server.
- nextest: 982 passed, 3 skipped in the worktree.

#### T33.4 Discovery, package digest and `cox plugin list`

Depends: T33.3 · Size: ~190 · Files: `crates/cox-plugin/src/discover.rs`, `crates/cox/src/plugin_cmd.rs`, `crates/cox/src/cli.rs`
Goal: find user plugins (`~/.cox/plugins/<id>/current`) and project plugins (`<git root>/.cox/plugins/<id>`). The user plugin wins a clash with a notice. Validate each manifest, compute the SHA-256 tree digest (PL§1), and list plugins with their state.
Plan: `cox plugin list [--json]` loads each granted plugin, runs `cox_init` against a stub session and reports its contributions. The grant state reads "unknown" until T33.6. `cox ext list` gains a plugins section.
Check: `digest_changes_when_any_file_changes`, `user_plugin_shadows_project_plugin_with_notice`, `malformed_manifest_is_listed_as_skipped`; the real binary against a scratch `COX_HOME` lists a WAT fixture plugin.
Status: done 2026-09-26
Result: `cox_plugin::discover(cox_home, project_root)` scans `~/.cox/plugins/<id>/` (following `current` → `versions/<digest12>/`) and `<git root>/.cox/plugins/<id>/`. When both define the same id, the user plugin wins and a shadowing notice is recorded.
`load_manifest` parses `plugin.toml` with figment and checks that the `id` matches its directory, then runs `validate()`. It rejects a `wasm` path that is absolute or contains `..`; project content is untrusted (D14), and `cox-plugin` cannot call `confine`.
`package_digest` is SHA-256 over the sorted `(relative path, length, bytes)` of every file.
`cox plugin list [--json]` reports each plugin's id, source, version, digest, declared capabilities and grant. It executes no plugin code: the state is `discovered` and the grant is `unknown` until T33.6, which compiles and runs `cox_init` only for granted plugins. All of it sits behind the `plugins` feature.
Review fix: the first version compiled every discovered plugin and ran `cox_init`, project plugins included, before any grant existed. That broke PL§1 ("a project plugin never loads until the user enables it"), so it was removed and a regression test added.
Deviations:
- About 560 lines instead of ~190.
- A fourth file, `crates/cox/tests/plugin_cli.rs`, because the Check runs the real binary.
- `cox-plugin-api` is now an optional dependency of `crates/cox`.
Check: all pass:
- `digest_changes_when_any_file_changes`
- `user_plugin_shadows_project_plugin_with_notice`
- `malformed_manifest_is_listed_as_skipped`
- `wasm_path_escaping_package_dir_is_skipped`
- `contributions_summary_lists_only_nonempty_kinds`
- `cox_plugin_list_reports_a_wat_fixture_plugin`: the real binary against a scratch `COX_HOME`
- `cox_plugin_list_never_runs_a_project_plugins_module`
Slim clippy is clean. nextest: 977 passed, 3 skipped in the worktree.

#### T34.1 Wire custom agent definitions into the `agent` tool

Depends: — · Size: ~190 · Files: `crates/cox-core/src/subagent.rs`, `crates/cox-core/src/session.rs`, `crates/cox-core/Cargo.toml` (new path dependency on `cox-ext`)
Goal: `agent(preset: "<name>")` dispatches an `AgentDef` discovered by `cox_ext::agents::discover` (`.cox/agents/*.md`, `.claude/agents/*.md`, home variants) exactly as the built-in `explore`/`shell` presets already do, and the `agent` schema gains a `tier` field — closing the gap between §1.11's documented `agent` row and the code, which today only ever resolves the two hardcoded presets (`AgentTool::preset`, `PRESETS.iter().copied().find(...)`; `cox-ext::agents::discover` is otherwise called only by `cox ext list`).
Plan:
1. `Session::new`/`resume` discover `AgentDef`s once per session build (the same roots `cox ext list` already reads) and hand them to `AgentTool::new`.
2. `AgentTool::preset()` tries the built-in `PRESETS` first (unchanged behaviour for `explore`/`shell`), then a discovered `AgentDef` by exact name; a miss lists both the built-in and the discovered names in the `ToolError::Denied`.
3. `tools_for()` uses `AgentDef::restrict` when the resolved preset came from a definition, the existing `Preset.tools` path otherwise.
4. `call()` reads an optional `tier` input; when present it overrides `tier_for(def.model)` (or the built-in preset's job tier), but only downward — clamped the same way the router already clamps a decision-point's tier offer (D5 "never up").
5. The `preset` input schema drops its fixed two-value enum for a free-form string; the tool's own description keeps naming `explore`/`shell` and says custom names come from `.claude/agents`/`.cox/agents`.
Check: `agent_dispatches_a_discovered_custom_preset_by_name`, `agent_unknown_preset_lists_builtin_and_discovered_names_in_error`, `agent_tier_override_is_honored_and_clamped_never_above_config`; the existing `subagent_presets_are_explore_and_shell` and `subagent_budget_is_a_slice_of_parent` stay green unchanged.
Status: done 2026-09-26
Result: `agent(preset: "<name>")` now dispatches a custom definition found in `.cox/agents` / `.claude/agents` (and the home variants), just as it dispatches `explore` and `shell`.
- Discovery runs once, when `crates/cox` builds the session. The definitions are installed with `Session::set_agent_defs`, so `cox-core` does no I/O and the tool schema stays byte-stable for the session.
- `AgentDef`, `restrict` and `tier_for` moved to `cox_protocol::agent`, because the type crosses crate boundaries. `cox_ext::agents` re-exports them, so `cox-core` gains no dependency on `cox-ext`.
- `resolve()` checks the built-in presets first, then the discovered definitions. An unknown name is denied, and the error lists every name.
- A new `tier` input can only lower the tier (D5). The tool description names the custom presets.
- Custom presets are charged to the new `Job::Agent`, default tier `cheap`, so `cox stats` does not count them as `shell`.
- `docs/config.jsonschema`, `docs/config.md` and `docs/protocol.jsonschema` are regenerated.
Review fixes: the first version made `cox-core` depend on `cox-ext` and tagged custom presets `Job::Shell`.
Deviations:
- `agent_unknown_preset_lists_builtin_and_discovered_names_in_error` is a unit test on `resolve()` instead of a full-turn test. A failed resolve falls back to `Risk::Exec`, which would stall a turn test on an approval.
- The `cox acp` session build still does not install the definitions. That gap already existed for skills and MCP there.
- 14 files changed.
Check:
- Pass: `agent_dispatches_a_discovered_custom_preset_by_name`, `agent_unknown_preset_lists_builtin_and_discovered_names_in_error`, `agent_tier_override_is_honored_and_clamped`, `agent_def_restrict_keeps_only_named_tools_in_parent_order`. The existing subagent tests are unchanged.
- Real binary with a scripted provider: `.cox/agents/reviewer.md` ran at the cheap tier.
- nextest: 966 passed, 3 skipped in the worktree.

#### T34.5 Core routing in the parent

Depends: T34.4 · Size: ~190 · Files: `crates/cox-core/src/tasks.rs` (registry keeps live child handles plus a finished-session lookup), `crates/cox-core/src/subagent.rs`, `crates/cox-core/src/session.rs` (`Session::resume` today hardcodes `parent_id: None`, `Job::Main`, `Tier::Code`; it gains parameters so a child resumes with its own job, tier, parent and budget slice — SM§2)
Goal: the parent resolves a `TaskId` to either a still-running child handle or a finished child's stored session id (`sessions.parent_id`, already a real link); delivery to a running child queues a `Submission::UserTurn` after its current turn; delivery to a finished child resumes it via `Session::resume` and re-registers it as running for any further messages.
Check: `task_message_reaches_a_still_running_subagent_after_its_current_turn`, `task_message_to_a_finished_subagent_resumes_it_with_history_intact`, `resumed_subagent_keeps_its_parent_id_and_budget_slice`.
Status: done 2026-09-26
Result: the parent now routes `TaskMessage` (SM§2, §3, §5).
- **Registry:** `Inner.children` maps each `TaskId` to one of two states. A running child holds a message queue and the hop of the message that started its current turn. A finished child becomes `Dormant`: its `SessionId` plus a `Spec` (config with the budget slice left, tools, job, tier, worktree and labels).
- **Delivery:** `deliver` either queues the message or wakes the child. `run_task` takes a queued message only on the child's `TurnDone` and runs it as a new `UserTurn`, so a message never arrives mid-turn. When a run ends, `park_child` either downgrades the child to `Dormant` or picks up a message that arrived during wind-down, all under one lock.
- **Wake:** `wake` rebuilds `History` from the child's rollout and calls `spawn_child(.., Some((id, history)))`, which keeps the child's job, tier and `parent_id`.
- **Relay:** the child's `Event::TaskMessage` goes to the parent. The parent stamps `from` and sets `hop` to the sender's current hop + 1. A message with hop > `MAX_HOPS` (4) is dropped with a warning.
- **Message to the parent:** it becomes a history line, the same way `publish_task_result` works.
- **Message cap:** `MAX_MESSAGES_PER_TASK` is left to T34.6.
Deviations:
- Resume goes through a `resume` parameter on `spawn_child`, not the top-level public `Session::resume`.
- The test `MemoryStore` now keeps rollouts per session.
- `resumed_subagent_keeps_its_parent_id_and_budget_slice` is folded into the resume test, and `parent_id` is not asserted directly there.
- About 360 lines of production code plus about 370 lines of tests, against a target of ~200.
Check: passing tests:
- `task_message_reaches_a_still_running_subagent_after_its_current_turn`
- `task_message_to_a_finished_subagent_resumes_it_with_history_intact`
- `sibling_message_is_relayed_through_the_parent`
- `hop_limit_stops_a_ping_pong`

These use the scripted provider. nextest: 1005 passed, 3 skipped in the worktree.

#### T34.3 `ask_user` from a subagent, labelled with `Source`

Depends: — · Size: ~150 · Files: `crates/cox-tools/src/ask_user.rs`, `crates/cox-protocol/src/traits.rs` (`ToolCx` gains the agent label), `crates/cox-core/src/subagent.rs`
Goal: a subagent whose preset/definition grants `ask_user` can pause and ask the user a question exactly like the parent does, and every surface that renders it can say which subagent is asking — reusing the `Source` type `ApprovalRequired` already carries instead of inventing a new one.
Plan:
1. `ask_user::Question` gains an optional `source: Option<Source>` — `None` for the top-level session, `Some` for a subagent.
2. `ToolCx` carries the agent name/preset alongside its existing `session: SessionId`, set once in `spawn_child`/`AgentTool::call`, the same way `relay_approval` builds its `Source` today.
3. Wherever `Answers::Surface` renders a `Question`, it shows the label the same way `ask_permission` labels a relayed approval, when `source` is `Some`.
4. No shipped preset (`explore`, `shell`) grants `ask_user` in this task — that is a content decision for whoever writes the first custom definition that needs it; this task only makes it possible and labelled.
Check: `ask_user_from_subagent_carries_its_source`, `ask_user_surface_shows_which_agent_is_asking`; existing `ask_user.rs` tests unchanged.
Status: done 2026-09-26
Result: `ask_user` called from a subagent reaches the user labelled with the agent that asks (SM§4).
- **Protocol:** `Question` gets `source: Option<Source>`. `ToolCx` gets `agent`/`preset`, which are `None` in a top-level session.
- **Core:** `spawn_child` takes the child's `name` and preset name, the same labels `relay_approval` already uses, so every `ToolCx` the child hands its tools carries them.
- **TUI:** the question modal shows an "X asks:" line. The label goes through `cox_sanitize::sanitize`.
- **Permissions:** no built-in preset grants `ask_user`. A child sees it only if the parent's tool set and the preset allow it.
Deviations: the cherry-pick onto main conflicted with T34.5 in `spawn_child`, which now takes `resume` too, and in `AgentTool::call`. Resolved by keeping T34.5's `Spec`/`spawn` path, and `spawn` now passes `spec.name`/`spec.preset_name`.
Check: passing tests:
- `ask_user_from_subagent_carries_its_source`
- `ask_user_surface_shows_which_agent_is_asking` (insta snapshot)

nextest on main after the merge: 1007 passed, 3 skipped. The merged `spawn_child` needed `#[allow(clippy::too_many_arguments)]`, the same as `build`.

#### T34.2 A subagent concurrency cap

Depends: — · Size: ~140 · Files: `crates/cox-protocol/src/config.rs` (new `core.max_concurrent_subagents`), `crates/cox-core/src/subagent.rs`, `crates/cox-core/src/tasks.rs`
Goal: cap how many subagent tasks (foreground and background) may run at once per session, so a loop of `background: true` calls cannot silently multiply cost or exhaust the parent's budget slice faster than the user can notice. Matches the shape of Codex's `agents.max_concurrent_threads_per_session` and Claude Code's `CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS` (research.md §4.3.7), but as a `cox-config` key (D13: one config file, every flag is a key), not an env var.
Plan: `core.max_concurrent_subagents` (default generous, e.g. 8), read from `self.parent.config.core` in `AgentTool::call()`; counted against the parent's currently-registered `TaskKind::Agent` tasks (`register_task`/`complete_task`, already tracked in `Session::inner.tasks`) before spawning; over the cap is `ToolError::Denied` naming the cap and how many are running, the same shape as T34.1's "unknown preset" denial.
Check: `agent_call_denied_when_concurrent_cap_reached`, `agent_call_allowed_after_a_running_task_completes`, `config_jsonschema_matches_committed_file` stays green with the new key.
Status: done 2026-09-26
Result: `core.max_concurrent_subagents` caps how many `agent` tasks run at once.
- **Reservation:** `Session.agent_slots` is an `Arc<AtomicU32>`. `try_reserve_agent_slot` checks the cap and takes a slot in one CAS step. `AgentSlotGuard` frees the slot on drop, so every exit path releases it exactly once.
- **Where:** `AgentTool::call` reserves before `resolve`. A foreground child holds the guard until `call` returns. A background child moves it into its `drive` task, so it holds the slot until it finishes. Over the cap the call gets `ToolError::Denied`, naming the running count and the cap.
- **Config guard:** the project layer cannot raise `core.max_concurrent_subagents`. The value is reverted and the key is added to `GUARDED_KEYS`, the same treatment as the budget keys. `docs/config.jsonschema` and `docs/config.md` were regenerated.
Deviations:
- Review round: the first draft counted and then registered across an `await`, which races, and it had no project guard. Both were fixed.
- Merging onto main: T34.5 turned the background path into `tokio::spawn(drive(..))`, so the guard now moves into a wrapper future around `drive`.
- Known gap, handed to T34.6 step 3: a dormant child woken by a message (`wake`) runs without a slot.
Check: passing tests:
- `agent_call_denied_when_concurrent_cap_reached`
- `agent_call_allowed_after_a_running_task_completes`
- `agent_calls_reserve_exactly_cap_slots_under_a_parallel_burst` (10 parallel calls over cap 3: exactly 3 spawn and 7 are denied; stable over 15 repeats)
- `config_project_cannot_raise_max_concurrent_subagents`

nextest on main after both landed: 1014 passed, 3 skipped. fmt and clippy clean.

#### T34.10 Optional: per-subagent visibility gate

Depends: T34.1 · Size: ~80 · Files: `crates/cox-ext/src/agents.rs` (an optional frontmatter field, e.g. `disabled: true`), `crates/cox-core/src/subagent.rs` (the tool's own description lists only enabled defs)
Goal: match OpenCode's "a subagent's `permission: deny` removes it from the Task tool's description entirely" (research.md §4.3.7) — lets a project ship a `.cox/agents/*.md` definition that exists on disk (e.g. only for `cox ext list`) without the model being told it can dispatch it. Low priority: T34.1 already gives every discovered definition a working dispatch path; this is a visibility nicety, not a functional gap.
Check: `disabled_agent_def_is_discovered_but_not_offered_to_the_model`.

**Order.** T34.0 blocks T34.4 → T34.5 → T34.6 → (T34.7, T34.8, T34.9), the last three running in parallel once the tool and its caps land. T34.1, T34.2 and T34.3 have no dependency on the messaging design and can run any time; T34.10 waits only on T34.1. The top table gets rows T34.0–T34.10; P0 for the highest-value wiring gap (T34.1), P1 for the messaging design and its critical path plus the `ask_user` labelling (T34.0, T34.3–T34.6, T34.9), P2 for the concurrency cap and the surfaces off the critical path (T34.2, T34.7, T34.8), P3 for the optional visibility gate (T34.10).
Status: done 2026-09-26
Result: an agent definition can be discovered but not offered to the model (SM§6).
- **Frontmatter:** `disabled: true` sets `AgentDef.disabled`. A missing key means `false`.
- **Agent tool:** `custom_names` skips disabled definitions, so the `agent` tool's description and the "unknown preset" error both omit them. `resolve` treats a disabled name as unknown.
- **`cox ext list`:** still shows the definition, with ` (disabled)` in text output and `"disabled": true` in JSON.
- **Field name:** research.md M1 has no Claude Code field for this, so the card's `disabled` is used. It is the same split OpenCode's `permission: deny` gives.
Check: passing tests:
- `disabled_agent_def_is_discovered_but_not_offered_to_the_model`
- `ext_list_marks_a_disabled_agent_but_still_shows_it`
- the frontmatter parse test in `cox-ext`

nextest: 1008 passed, 3 skipped in the worktree. fmt and clippy clean.

nextest on main after both landed: 1014 passed, 3 skipped. fmt and clippy clean.

#### T34.7 TUI and stream-json rendering

Depends: T34.6 · Size: ~150 · Files: `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/view.rs` (the transcript line and the `/agents` overlay)
Goal: a delivered `TaskMessage` shows as a transcript line (sanitized through `cox_sanitize::sanitize`, D14) and updates the `/agents` card for that task (A29's narrow card, not a new progress event stream); `stream-json` needs no special case since `Event` already passes through generically (D2) — this card adds the test that proves it.
Check: `insta` snapshot of a task-message transcript line and an updated `/agents` card; `stream_json_passes_task_message_through_unchanged` (headless e2e).
Plan: starts before T34.6 lands, because `Event::TaskMessage` has existed since T34.4 and the tests feed the event straight into `update`. In `state.rs`, add a `TaskMessage` arm that pushes a sanitized transcript line labelled like `ApprovalRequired`'s relay, and touch that task's `/agents` card. Add the `view.rs` line style. For stream-json, one headless e2e that proves the event passes through unchanged.
Status: done 2026-09-26
Result: a delivered `Event::TaskMessage` now shows up in the TUI (SM§6).
- **Transcript:** `state.rs` handles `TaskMessage` with a new `Cell::TaskMessage { label, text, from_task }`. The label is the task's registered label, or the raw `TaskId` if the task has already finished. Label and text go through `sanitize` once, in `state.rs`.
- **Rendering:** a child speaking to its parent shows as "X says: …"; a message delivered to a task shows as "→ X: …". The label uses the bold `theme.agent` style of the T34.3 "X asks:" line.
- **`/agents`:** the task's card gets a `· last: <first line>` segment (A29's narrow card).
- **stream-json:** needs no special case.
Deviations:
- The style lives in `cells.rs`, not `view.rs`, because every `Cell` variant renders there.
- No scripted scenario can emit a `TaskMessage` until T34.6. So `stream_json_passes_task_message_through_unchanged` calls the stream-json writer's own two steps, `redact::scrub_event` and then `serde_json::to_string`, on a hand-built event. It asserts the event comes back unchanged with `type: "task_message"`. T34.9's e2e covers the live path.
- Started before T34.6 landed: the event has existed since T34.4.
Check: passing tests:
- `cell_task_message_labels_the_speaker_and_direction` (insta)
- `task_message_adds_a_last_message_line_to_the_agents_card` (insta)
- `stream_json_passes_task_message_through_unchanged`

nextest: 1017 passed, 3 skipped in the worktree.

nextest on main after landing: 1017 passed, 3 skipped. fmt and clippy clean.

#### T34.8 ACP rendering, and the dropped task-lifecycle events

Depends: T34.6 · Size: ~120 · Files: `crates/cox-acp/src/server.rs`
Goal: `drive_prompt`'s event loop today falls into `Ok(_) => {}` for everything except `TurnDone` and `ApprovalRequired` (server.rs:344), so an ACP client (Zed, JetBrains) never sees `TaskCreated`/`TaskCompleted`/a delivered `TaskMessage`. This card gives those three their own arms: `TaskCreated`/`TaskCompleted` become a plan/task update the same way `ask_permission` already labels a relayed approval, and a `TaskMessage` renders with the same label.
Check: `acp_reports_task_created_and_completed`, `acp_reports_a_delivered_task_message`.
Plan: starts before T34.6 lands, because the three events already exist and the tests drive `drive_prompt` with injected events. Add three arms in `server.rs`: `TaskCreated`/`TaskCompleted` become a plan/tool-call update, and `TaskMessage` becomes an agent-message chunk with the task label. All text goes through `cox_sanitize::sanitize`.
Status: done 2026-09-26
Result: `drive_prompt` no longer drops the task events.
- **`TaskCreated`** becomes a `SessionUpdate::ToolCall` titled "task: <label>" (`ToolKind::Other`, `InProgress`, keyed by the `TaskId`).
- **`TaskCompleted`** becomes a `ToolCallUpdate` on the same id: `Completed`, plus a one-line "finished ($cost)[, exit N]".
- **`TaskMessage`** becomes an `AgentMessageChunk`. It is prefixed with the sender's label when `from` names a task this prompt has seen (falling back to the bare `TaskId`), and has no prefix when the parent sent it. This mirrors how `ask_permission` labels a relayed approval.
- **Sanitizing:** labels and text go through `cox_sanitize::sanitize`. The workspace crate `cox-sanitize` is now a dependency of `cox-acp`; no external crate was added.
Deviations:
- The three arms build their updates in pure helpers (`task_created_update`, `task_completed_update`, `task_message_update`). The tests assert on the serialized ACP JSON from those helpers rather than going through the full `tests/conformance.rs` transport. The reason: no live `TaskMessage` can be produced from outside `cox-core` until T34.6, and T34.9's e2e covers the live path.
- Started before T34.6 landed.
Check: passing tests:
- `acp_reports_task_created_and_completed`
- `acp_reports_a_delivered_task_message`

nextest: 1016 passed, 3 skipped in the worktree.

nextest on main after landing: 1019 passed, 3 skipped. fmt, clippy and cargo deny clean.

#### T34.6 The `send_message` tool

Depends: T34.5, T34.2 · Size: ~170 · Files: `crates/cox-tools/src/send_message.rs` (new), `crates/cox-core/src/subagent.rs`
Goal: `send_message { to, text }` for a subagent (`to: "parent"` or a sibling's name/`TaskId`, relayed through the parent per T34.0 §4) and for the parent (`to: <child name/id>`). T34.5 already routes and hop-limits (`MAX_HOPS`); this card adds the tool, `Session::self_task`, and the `MAX_MESSAGES_PER_TASK = 16` received-message cap (SM§5: a named constant, not a config key), so a flood is denied instead of spinning.
Plan:
1. `cox-tools/src/send_message.rs`: the `Tool` impl. It parses `{to, text}` and emits the child's `Event::TaskMessage` (or, in the parent, a `Submission::TaskMessage`) through a `ToolCx`/trait hook. It never touches the registry itself.
2. `cox-core/src/subagent.rs`: `self_task` is set in `spawn`. The parent resolves `to` by name or `TaskId`. `MAX_MESSAGES_PER_TASK` counts deliveries per `TaskId`, and the 17th is `ToolError::Denied`, shaped like T34.2's denial.
3. A message that wakes a *dormant* child (`wake`, from `deliver`/`park_child`) must hold a T34.2 slot for that run, reserved with `try_reserve_agent_slot`. At the cap, the sender gets `Denied` and the message is not queued. T34.2 left this gap: a woken child today runs outside the cap.
4. No preset grants `send_message` yet, the same as `ask_user` (SM§4).
Check: `sibling_message_is_relayed_through_the_parent_session`, `message_cap_denies_the_nth_plus_one_follow_up`, `waking_a_dormant_child_at_the_cap_is_denied` (T34.5 already has `hop_limit_stops_a_ping_pong`).
Status: done 2026-09-26
Result: agents can message each other through the parent with `send_message { to, text }` (SM§4, §5).
- **Tool:** `cox-tools/src/send_message.rs` holds a stateless `SendMessageTool`. It reads `cx.relay`, a new `ToolCx.relay: Option<Arc<dyn Relay>>` with the `Relay` trait in `cox-protocol`. `turn.rs` fills that field with the session running the call, so a child's call is stamped and routed by the child's own session. With `None` (the MCP surface) the tool returns `Denied`.
- **Child side:** `Session::self_task` is set in `spawn`. `to` is `"parent"` or a sibling's `TaskId`, and the child emits `Event::TaskMessage`, which T34.5's relay routes.
- **Parent side:** `resolve_addressee` accepts a child's registry name (`explore-2`, indexed by `name_task`) or its `TaskId`, then calls `deliver`.
- **Flood guard:** `MAX_MESSAGES_PER_TASK = 16` is a named constant. The 17th delivery to a task is `Denied`, and `CoreError::Denied` was added so `deliver` can report it (`docs/protocol.jsonschema` regenerated).
- **Slots:** `deliver` decides under one lock (unknown, flooded, at the cap, queued, or wake) and acts after the lock drops. Waking a dormant child reserves a T34.2 slot, and at the cap the sender gets `Denied` and nothing is queued. A foreground `park_child` → `wake` continuation takes over the slot its `call` already holds. This closes the gap T34.2 left.
- **Presets:** no built-in preset grants `send_message`. It is on the top-level session's tool list (`crates/cox/src/session.rs`, since `cox-core` cannot name `cox-tools`), and a custom agent definition can list it.
Deviations:
- About 330 lines of production code against a target of ~200. Most of it is the `Relay` plumbing across three crates and the `deliver` rework.
- Review round: the first draft bound the relay to one shared tool instance through a `OnceLock`, which would have routed a child's call as the top-level session. It also ran a woken foreground continuation over the cap with a warning. Both were fixed.
- A child addresses a sibling by `TaskId` only; the parent resolves names.
Check: passing tests:
- `sibling_message_is_relayed_through_the_parent_session`
- `message_cap_denies_the_nth_plus_one_follow_up`
- `waking_a_dormant_child_at_the_cap_is_denied`
- `each_childs_tool_cx_relay_is_bound_to_its_own_session`
- `the_same_tool_instance_reaches_each_calls_own_relay`
- T34.5's `hop_limit_stops_a_ping_pong` still passes.

nextest: 1022 passed, 3 skipped in the worktree.

nextest on main after landing: 1027 passed, 3 skipped. fmt, clippy and the slim build are clean.
