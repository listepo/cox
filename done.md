

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
