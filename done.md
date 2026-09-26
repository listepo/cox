

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
Goal: one tested module in the `evals` package, not a one-off script, that runs any registered agent against any registered provider and model on a Terminal-Bench 2.0 task list through Harbor, and turns the job results into one per-task table. Its first user was to be the cox vs Claude Code vs Terminus 2 comparison, now an idea (`ideas.md`).
Plan:
1. `evals/src/cox_evals/matrix.py`: registries in code (no config file to schema): `PROVIDERS` (LM Studio, Ollama, Anthropic, OpenAI; each with its host URL, the URL a task container uses, the API shapes it serves — OpenAI Chat, Anthropic Messages — its key env or a dummy key for local servers, and a preflight: server up, model listed); `AGENTS` (cox, Harbor's `claude-code`, `terminus-2`; each states the API shape it needs and builds its `harbor run` argv and env for a provider and model); `PRESETS` (the comparison's 12 tasks, three agents, `lmstudio` + `prism-ml/bonsai-27b`). `harbor_argv`, a sequential runner (`subprocess`, one job per agent × model), `summarize(job_dir)` over Harbor's `result.json` files, a table printer, `main` with `--preset/--agents/--provider/--model/--tasks/--dry-run/--jobs-dir`; console script `cox-bench`.
2. `cox_evals.tbench.CoxAgent`: `provider`/`base_url`/`api` kwargs that write the provider section into the container's fresh `COX_HOME/config.toml`, and a dummy key for local servers. cox reaches LM Studio through its Anthropic Messages endpoint: cox's OpenAI Chat path still drops every tool call (ideas.md), so a chat-shape run would measure that bug, not the agent.
3. `evals/tests/test_matrix.py`: argv and env per agent × provider, shape mismatch is an error before anything runs, container URL rewriting, preset contents, `summarize` over a fake job tree, `--dry-run` prints and runs nothing.
Check: `just test-evals` green; `cox-bench --preset same-model --dry-run` prints three `harbor run` commands.
Done when: the module and its tests land and the dry run matches the comparison's card.
What landed: `evals/src/cox_evals/matrix.py` (console script `cox-bench`): registries `PROVIDERS` (lmstudio, ollama, anthropic, openai; host URL, container URL via `host.lima.internal`, API shapes, key env, prepare commands), `AGENTS` (cox, `claude-code`, `terminus-2`; shape preference and argv/env builder), `PRESETS` (`same-model`); shape negotiation, preflight against the server's `/v1/models`, sequential `harbor run` per agent, `summarize` over trial `result.json`, a markdown per-task table. `CoxAgent` gained `base_url`/`context_window` kwargs that upload a `[providers.*]` section into the container's `COX_HOME/config.toml`. Keys stay in the process env; a local server gets the dummy key `local`.
Deviations: five files (matrix, its tests, `tbench.py`, `test_tbench.py`, `pyproject.toml`) plus `toolchain.md`. Live checks beyond the card: LM Studio serves `prism-ml/bonsai-27b` with tool calls on both `/v1/messages` and `/v1/chat/completions`, and the host cox (`--provider anthropic`, `base_url = "http://localhost:1234"`) finished a one-tool task at $0 — so cox's Messages path works against LM Studio. Not checked: reaching `host.lima.internal` from a task container (left to the comparison run).
Check:
```text
$ just test-evals
39 passed in 3.13s
$ cox-bench --preset same-model --dry-run
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

#### T34.9 e2e: two subagents messaging through the parent

Depends: T34.6 · Size: ~150 · Files: `tests/subagent_messaging.rs` (new)
Goal: with the Scripted provider, a parent spawns two children; child A sends `send_message` to child B by name; the parent relays it; B replies; the parent's history carries both pointer lines; a scripted ping-pong hits T34.6's hop limit and stops instead of looping forever.
Check: `parent_relays_a_message_between_two_children`, `hop_limit_stops_a_scripted_ping_pong` — both against the real event stream, no network, no API key (D12).
Plan:
1. Sibling by name. T34.6 lets a child address a sibling only by `TaskId`, which is random, so a scripted scenario cannot name it. The parent's name→`TaskId` index (`task_names`) becomes an `Arc` shared read-only with each child at `spawn`, so the child's `Relay` resolves `to: "<name>"` itself. Names are deterministic (`<preset>-<n>`). Unit test: `child_resolves_a_sibling_by_name`.
2. The e2e runs the real binary headless (`cox run -p --output-format stream-json`) against a `COX_HOME` scratch tree. It uses a custom agent definition (T34.1) that lists `send_message`, plus a Scripted scenario: the parent spawns `talker-1` and `talker-2` in the background, `talker-1` messages `talker-2`, `talker-2` replies to `parent`, and the test asserts the `task_message` events and the parent's pointer lines. A second scenario ping-pongs and asserts it stops at `MAX_HOPS`.
3. Amendment, found while building the e2e: headless `cox run -p` called `process::exit` while background subagents were still running, which killed their work and dropped their events from stream-json. The headless run now awaits the session's background tasks, using the task registry, before exiting; cancellation still ends it at once. This replaces a draft that relied on `sleep` in the scenario.
Status: done 2026-09-26
Result: two subagents message each other through the parent, end to end, from the real binary. Building this found and fixed two headless bugs.
- **Siblings by name:** the parent's name→`TaskId` index (`task_names`) is an `Arc` shared read-only with each child, so `send_message {to: "talker-2"}` resolves in the child. One helper, `resolve_name_or_id`, serves both parent and child.
- **Scripted provider:** a scenario turn can carry `when_contains`, which pins it to the request whose transcript text contains that marker (`Content::Text` only), with FIFO order for unmarked turns. It is needed because every child shares one `Scripted` instance.
- **Headless wait:** `cox run -p` called `process::exit` while background subagents were still running, dropping their work and their stream-json events. `Session::wait_idle` now awaits the registry's `TaskKind::Agent` entries through a `Notify` bumped by `complete_task`, and cancellation ends the wait. `run.rs` keeps draining events until the turn has ended and the session is idle. Two races were closed along the way: a chain about to restart keeps its registry entry, and a woken child registers before it is spawned.
- **Detached shells:** detached `bash` tasks do not hold the exit open. At exit they are cancelled through `session.interrupt()`, using the bash tool's existing SIGTERM→SIGKILL path, and the run waits up to 5 s for them (`wait_tasks_cleared`) before `shutdown_background`. Before this, every headless run orphaned its detached shells.
Deviations:
- The tests are in `crates/cox/tests/subagent_messaging.rs`, beside `run_cli.rs`, because there is no root `tests/`.
- About 260 lines of tests plus four scenario TOMLs, against a target of ~150.
- Review took three rounds: a `sleep` buffer in the scenarios was replaced by the real wait; the wait was narrowed to agent tasks so a background dev server cannot hang the exit; and the leak of orphaned shell processes was fixed.
- Known gap: a detached shell from an earlier `--loop` turn is out of reach of `interrupt()`, because cancellation is turn-scoped. It is abandoned after the 5 s grace.
- Not checked: whether quitting the TUI while a background shell still runs leaks the same way. `run_tui` has the same runtime-drop shape.
Check: passing tests:
- `parent_relays_a_message_between_two_children`
- `hop_limit_stops_a_scripted_ping_pong`
- `headless_run_waits_for_background_subagents`
- `headless_run_does_not_wait_for_a_background_shell` (the run exits in under 10 s and no `sleep 4001` is left)
- `child_resolves_a_sibling_by_name`
- `scripted_parses_when_contains`

Reverting the wait makes the headless tests fail. The `subagent_messaging` tests passed 5 runs in a row. nextest: 1035 passed, 3 skipped in the worktree.

nextest on main after landing: 1035 passed, 3 skipped. fmt, clippy and the slim build are clean, and no `sleep 4001` was left running.

#### T32.2 `cox-render`: themes, colour, markdown, diff, SVG and glyphs

Depends: T32.1 · Moves: `theme.rs`, `color.rs`, `svg.rs`, `markdown.rs`, `diff.rs`, `glyph.rs` from `cox-tui` (~2.6k).
Why: dependencies (a), namely syntect, two-face, pulldown-cmark and terminal-colorsaurus.
Plan:
1. Before the move, record `cargo build --timings` for two builds: a clean `cox-tui`, and an incremental build after touching `state.rs`.
2. Move the modules.
3. Record the same timings again, both in R§4.3.4.
4. Apply the falsifier in `docs/design/crates.md`.

Check: syntect, two-face and pulldown-cmark appear only in `cox-render/Cargo.toml`; the TUI snapshots are unchanged.
Status: done 2026-09-26
Result: `crates/cox-render` holds `theme`, `color`, `svg`, `markdown`, `diff`, `glyph` and `link` (about 2.76k lines), the five built-in theme files (`assets/themes`), and `Look`.
- **Dependencies:** syntect, two-face, pulldown-cmark and terminal-colorsaurus moved from `cox-tui/Cargo.toml` to `cox-render/Cargo.toml`, versions unchanged. `cox-tui` also dropped its unused `tempfile` dev-dependency.
- **Old paths still work:** `cox-tui` re-exports every module (`cox_tui::{theme, diff, ..}`, `crate::theme` inside `cox-tui`, `cells::Look`), so callers in `crates/cox`, `cox-tools` and the TUI tests did not change.
- **`deps.rs`:**
  - `only_render_depends_on_the_highlighters` is new.
  - `cox-tui` may also depend on `cox-render`.
  - `cox-render` may depend only on `cox-protocol` and `cox-sanitize`.
- **Docs updated:** `plan.md` §1.1 (crate row and dependency direction), `AGENTS.md` Layout, `toolchain.md` rows, and a stale path in `docs/design/plugins.md`.
Timings, recorded in R§4.3.4. Each row is 5 runs of `cargo build -p cox-tui --timings`; figures are medians. Before and after ran back to back at load average 12–15.

| Build | `cox-tui` unit before | `cox-tui` unit after | `cox-render` unit | Wall before → after |
| --- | --- | --- | --- | --- |
| clean | 1.25 s | 0.98 s | 0.50 s | 2.19 → 2.20 s |
| incremental after touching `state.rs` | 0.42 s | 0.39 s | not rebuilt | 1.40 → 1.34 s |

Falsifier (`docs/design/crates.md`): it fires in substance. The gain is real but negligible, about 0.03 s per edit, because cargo never rebuilt the heavy dependencies on an edit anyway. The re-rank it asks for is moot: C5, C7, C9 and C10 had already landed. The outcome is recorded under "Falsifier" in `crates.md`.
Deviations:
- `link.rs` and the `Look` struct moved as well. `markdown`/`diff` need `Look`, and `link::mark` only needs ratatui, so they could not stay behind without a cycle.
- `cox-render` also carries `similar` (for `diff`) and `toml_edit` (theme files).
- Taken by the creator's instruction to do it directly rather than through a subagent.
Check:
- syntect, two-face, pulldown-cmark and terminal-colorsaurus appear only in `cox-render/Cargo.toml`, and `deps.rs` enforces it.
- No `.snap` file changed and no `.snap.new` was written.
- In the worktree: nextest 1036 passed, 3 skipped. fmt, clippy, the slim build and `cargo deny` are clean.
- On main after landing: nextest 1036 passed, 3 skipped. fmt, clippy and the slim build are clean.

#### T34.11 Quitting the TUI kills a still-running background shell

Depends: T34.9 · Size: ~60 · Files: `crates/cox/src/session.rs`, `crates/cox/src/run.rs`, `crates/cox/tests/tui_e2e.rs` (+ scenario `tests/scenarios/tui_background_shell.toml`)
Why: T34.9 made headless `run` cancel and reap a detached `bash` before exit (`interrupt` → `wait_tasks_cleared(SHELL_CANCEL_GRACE)` → `shutdown_background`). `run_tui` builds its own runtime and only drops it, so a `bash {background: true}` still running at quit may outlive cox as an orphan (ppid 1).
Plan:
1. Reproduce: PTY e2e `tui_quit_kills_a_running_background_shell` — a scripted detached `sleep 4002`, quit with Ctrl+C ×2 while it runs, then `pgrep -f 'sleep 4002'`.
2. If it leaks: after the last `Submission::Shutdown` in `run_tui`, run the same `interrupt` + `wait_tasks_cleared(SHELL_CANCEL_GRACE)` on each session the loop leaves (quit, `/clear`, fork, handoff), and end with `rt.shutdown_background()`. Make `SHELL_CANCEL_GRACE` `pub(crate)` in `run.rs` and reuse it; no new helper, no duplicate logic.
3. Known limit, not fixed here: cancellation is turn-scoped, so a shell detached in an *older* turn (the user sent another prompt after it) holds a token `interrupt()` no longer reaches; `wait_tasks_cleared`'s deadline gives up on it. Recorded in `ideas.md` rather than widened into this card.

Check: `tui_quit_kills_a_running_background_shell` fails before the change and passes after; the existing `tui_e2e` and `subagent_messaging` tests stay green; no `sleep 4002` is left running.
Status: done 2026-09-26
Result: reproduced, then fixed. Before the change, Ctrl+C ×2 with a detached `sleep 4002` running left cox hung (the runtime's `Drop` waits on the shell's blocking PTY reader, so `Tui::quit`'s 5 s exit check failed) and, once the test harness killed it, `sleep 4002` lived on with ppid 1. `run_tui` now calls `interrupt` + `wait_tasks_cleared(SHELL_CANCEL_GRACE)` after each session's `Shutdown` (quit, `/clear`, fork, handoff) and ends with `rt.shutdown_background()`; `SHELL_CANCEL_GRACE` is `pub(crate)` in `run.rs` and shared, no new helper.
Not fixed (recorded in `ideas.md`): a shell detached in an older turn. Probed with `sleep 4003` detached, then a second prompt, then quit: quit now returns after the 5 s grace instead of hanging, but that process is still orphaned (ppid 1), because `interrupt()` only reaches the current turn's token.
Check output:
- `tui_quit_kills_a_running_background_shell` failed before the change (`cox did not exit`, orphan `sleep 4002` with ppid 1) and passes after (2.9 s).
- nextest: 1036 passed, 3 skipped. fmt and clippy clean. No `sleep 4002`/`4003` left running.
- On main after landing (cherry-picked from the separate session's branch): nextest 1037 passed, 3 skipped. fmt, clippy and the slim build are clean. No `sleep 4002`/`4003` left running.

#### T33.27 Guest workspace and the Rust SDK

Depends: T33.2 · Size: ~190 · Files: `plugins/Cargo.toml`, `plugins/sdk/src/lib.rs`, `mise.toml` (+ `.github/workflows/ci.yml` `targets:`; `plugins/mise.toml` created empty for later languages)
Goal: `cox-plugin-sdk` over extism-pdk 1.4.1 and `cox-plugin-api`: typed wrappers for every export and host function in PL§4, plus a `register!` macro. Add `rust = { version = "1.97.1", targets = ["wasm32-unknown-unknown"] }`; the target is not a version bump. `docs/plugins.md` gets the author guide.
Check: `cargo build --manifest-path plugins/Cargo.toml -p cox-plugin-sdk --target wasm32-unknown-unknown`; the main-workspace `cargo nextest run --workspace` still never builds guest code; the extism-pdk row is in §1.1 and `toolchain.md`.
Status: done 2026-09-26
Result: `plugins/` is its own Cargo workspace (members `sdk`), outside the root one: the root `members = ["crates/*"]` never picks it up, the same way `fuzz/` stays out, and `cargo metadata` on the root lists no SDK package.
- **`cox-plugin-sdk` (`plugins/sdk`)** builds on extism-pdk 1.4.1 (`default-features = false`: no extism `http` host function, no msgpack) and `cox-plugin-api`, which it re-exports.
  - One typed wrapper per PL§4 host function: `log`, `notify`, `kv_get`/`kv_put`/`kv_delete`, `context`, `invoke_tool`, `model_call`, `http`, `output`, `cancelled`, `redraw`.
  - Errors are `SdkError`: `Host(AbiError)` or `Wire`.
  - `register!(init => f, key => g, …)` exports only the keys listed, because the host probes exports by name.
- **Wire, which host cards from T33.9 on must match** (documented in the `lib.rs` header and `docs/plugins.md`):
  - An export takes one JSON value (empty input is `null`) and returns one JSON value. A handler error sets the extism error text and returns code 1.
  - Every host function is an import in `cox:host/v1` with the signature `(u64) -> u64`. It receives one JSON value, or an object keyed by argument names when there are several arguments.
  - The host replies `{"Ok": T}` or `{"Err": AbiError}`, so a refusal reaches the plugin as a value and not as a trap.
- `mise.toml` pins `rust = { version = "1.97.1", targets = ["wasm32-unknown-unknown"] }` (same version). The CI `test` job's toolchain step gets `targets: wasm32-unknown-unknown`.
- `plugins/mise.toml` exists empty for later languages. `docs/plugins.md` is the author guide.
- Rows added: extism-pdk in `plan.md` §1.1, `toolchain.md` and the workspace `rust.md`.
Deviations:
- `cox_decide` maps `Question → Advice`, as `cox-plugin-api` defines it today. `DecideOut::Call` and `cox_decide_resume` wait for T33.40.1, which adds those types.
- `lib.rs` is about 280 lines without tests, above the ~190 estimate, mostly doc tables and the macro key table.
- CI gets the wasm32 target only. No CI step builds the SDK yet: T33.28's fixture build will, and T33.40.2 adds the `plugins` job.
Check:
- `cargo build --manifest-path plugins/Cargo.toml -p cox-plugin-sdk --target wasm32-unknown-unknown` passes.
- `cargo test --manifest-path plugins/Cargo.toml`: 8 passed. A compile-only module registers every key, so type drift against `cox-plugin-api` fails the build.
- fmt and clippy (`-D warnings`) are clean for both workspaces; the guest one was checked on wasm32 and on the host target.
- A guide-example cdylib built for wasm32 exports only `cox_init`, `cox_command` and `cox_on_event`. Besides extism's own env functions, it imports only `cox:host/v1` `cox_kv_*`.
- In the worktree: nextest 1037 passed, 3 skipped. Two flaky PTY e2e tests failed on the first run under load; the `--no-fail-fast` rerun passed.
- On main after landing: nextest 1037 passed, 3 skipped. fmt, clippy, the slim build, the wasm32 SDK build and the guest `cargo test` are clean.

#### T33.6 Grant check and granted-only loading — blocker

Depends: T33.4, T33.5 · Size: ~170 · Files: `crates/cox-plugin/src/grant.rs`, `crates/cox/src/session.rs`
Goal: the pure `grant::check(manifest, digest, stored) -> Verdict { Granted | NeedsApproval { added, removed } | Disabled }`. Session open loads only `Granted` plugins. Headless and ACP print one `Notice(Warn)` per ungranted plugin naming the command to run. `plugins.enabled` is a config key with an env var and `--no-plugins` (D13).
Check: `narrower_request_needs_no_reapproval`, `new_digest_needs_approval`, `widened_capability_is_reported_in_added`, `headless_never_loads_ungranted_plugin` (e2e, scratch `COX_HOME`); `every_flag_has_a_config_key` still green.
Status: done 2026-09-26
Result:
- **`crates/cox-plugin/src/grant.rs`:** the pure `check(manifest, digest, Option<&PluginGrant>) -> Verdict { Granted | NeedsApproval { added, removed } | Disabled }`.
  - `capability_list` defines the stored grant format: a sorted JSON array of capability strings.
  - Also `scope` and `enable_command`.
  - `cox-plugin` now depends on `cox-protocol`, which `deps.rs` already allowed.
- **Session open** (`plugin_notices` in `crates/cox/src/session.rs`, a no-op in the slim build):
  - Discovers plugins and reads each grant through `PluginStore::grant_get`.
  - Loads only `Granted` plugins.
  - Emits one `Notice(Warn)` per other plugin naming `cox plugin enable <id> [--project]`. Headless, plain, TUI and ACP (`acp_cmd.rs`, into the rollout) all get these notices.
- **Config:** `plugins.enabled` (`PluginsConfig`, `default.toml`, regenerated `docs/config.md` and `docs/config.jsonschema`), `COX_PLUGINS_ENABLED` and `--no-plugins`.
- **Project config** may turn plugins off, but never back on once the user turned them off. This adds an eighth guarded key.
- **Docs:** PL§3 (grant list format and rules), the `AGENTS.md` layout row and `plan.md` §1.6.
Deviations:
- A granted plugin is compiled and then dropped, and `cox_init` does not run. The session has no place to keep an instance until T33.9. A broken granted package is still reported as "failed to load".
- The grant list also covers `wasi` and one line per `[[provider]]`, `[[mcp]]` and `[[external_agents]]` entry, because these reach the network or run programs. `[limits]` and `[[models]]` are not in it.
- `model:code` covers a `model:cheap` request; every other capability must match its string exactly.
- `Disabled` wins over a digest check. A corrupt row or a store error counts as no grant.
- Disabled plugins, malformed manifests and discovery shadowing notices each get a warning too. The TUI shows the same warnings until T33.8 adds the grant modal.
- About 290 lines across 15 files, including generated docs, against ~170.
Not done:
- `cox plugin list` still reports `grant: "unknown"`, because `plugin_cmd.rs` belongs to T33.7.
- A new digest reports every capability in `added`. Diffing it against an older grant needs a list-grants store method (T33.7/T33.31).
Check:
- `narrower_request_needs_no_reapproval`, `new_digest_needs_approval`, `widened_capability_is_reported_in_added`, `headless_never_loads_ungranted_plugin` (e2e in `plugin_cli.rs`, scratch `COX_HOME`) and `every_flag_has_a_config_key` pass. `disabled_grant_never_loads_and_corrupt_row_grants_nothing` and `config_project_cannot_turn_plugins_on` are new and pass too.
- The real binary against a scratch `COX_HOME`: an ungranted project plugin gives one warning, "plugin trap is not loaded: it asks for kv and needs approval; run `cox plugin enable trap --project`". `COX_PLUGINS_ENABLED=false` silences it.
- In the worktree: nextest 1043 passed, 3 skipped; fmt, clippy and the slim build are clean.
- On main after landing (with T33.6, T33.16, T35.4, T35.3): nextest 1055 passed, 3 skipped. fmt, clippy and the slim build are clean.

#### T33.16 Models: the plugin catalog layer

Depends: T33.6, T30.24 · Size: ~150 · Files: `crates/cox-models/src/catalog.rs`, `crates/cox/src/session.rs`
Goal: `Catalog::load` takes plugin rows. The layer order is built-in < plugin (fill-only for existing ids) < config < user prices. A price conflict is ignored with a notice. Two plugins defining the same new id: the lower id wins. `ModelRow.source` records where each row came from.
Check: `plugin_cannot_override_builtin_price`, `plugin_fills_missing_context_window`, `config_overrides_plugin_row`, `duplicate_plugin_model_lower_id_wins`.
Status: done 2026-09-26
Result:
- `Catalog::load(config, plugins: &[PluginModels], user_prices)` layers built-in < plugin < config < user prices. `PluginModels` borrows `cox_protocol::plugin::ModelDecl`, so `cox-models` still depends only on `cox-protocol`.
- **Plugin layer:**
  - Plugins are applied in plugin-id order, so the lower id wins.
  - A new id is taken whole.
  - On an existing row a plugin only fills `None` fields.
  - A differing value, including a price, is ignored with a warning, and so is a second plugin defining the same new id.
  - Missing cache rates default to the plugin's input rate, so cost is never under-counted.
- **Warnings** stay on the catalog (`Catalog::warnings()`), and nothing is printed.
- `ModelRow.source: RowSource` (`Builtin`, `Plugin(id)`, `Config`, `UserPrices`, `Served`) records the layer that created the row.
- `session.rs` and `doctor.rs` pass `&[]` for now.
Deviations:
- Four files: `doctor.rs` needed the call-site change.
- About 180 lines of code, above ~150.
- `RowSource::Served` was added because `overlay_served` can create a row.
- The tests use the made-up id `acme-coder`, because `jev-latest` is already a built-in row.
Left: feed the granted plugins' `[[models]]` into `Catalog::load` in `backend_for_with`, and show `catalog.warnings()` as `Notice(Warn)`. That wiring needs a session that keeps loaded manifests (T33.9 or later).
Check:
- `plugin_cannot_override_builtin_price`, `plugin_fills_missing_context_window`, `config_overrides_plugin_row` and `duplicate_plugin_model_lower_id_wins` pass, plus `plugin_row_for_a_new_id_is_taken_whole`.
- `cox doctor` against a scratch `COX_HOME` exits 0.
- In the worktree: nextest 1042 passed, 3 skipped. fmt, clippy and the slim build are clean.
- On main after landing (with T33.6, T33.16, T35.4, T35.3): nextest 1055 passed, 3 skipped. fmt, clippy and the slim build are clean.

#### T35.4 stream-json adapter

Depends: T35.2 · Size: ~170 · Files: `crates/cox-core/src/external_agent.rs` (new), `crates/cox-core/src/subagent.rs`
Goal: a pure, host-side line mapper (EA§5) from Cursor CLI's `stream-json` event shapes (research.md §4.3.8: `system`/`user`/`assistant`/`tool_call{started,completed}`/`result`) onto `cox_protocol::Event`/`Item`; an unrecognised line becomes a sanitized `Notice`, never a hard error (D14), matching `broken_hook_is_skipped_not_fatal`'s fail-open shape.
Check: `stream_json_assistant_line_maps_to_cox_event`, `stream_json_tool_call_started_and_completed_pair_map_to_one_item`, `unrecognised_stream_json_line_becomes_a_sanitized_notice`.
Status: done 2026-09-26
Result: `cox_core::external_agent::StreamJsonMapper::new(turn_id, sanitize)` exposes `map_line(&mut self, &str) -> Vec<Event>`. It is pure and keeps state across lines. `lib.rs` gains the module declaration; `subagent.rs` is unchanged.
- `system`/`init` becomes a `Notice(Info)` with the model and the permission mode.
- `user` lines and blank lines are dropped.
- `assistant` becomes `ItemStarted(AssistantMessage)` then `ItemDone`.
- `tool_call` `started` becomes `ItemStarted(ToolCall)` plus `ToolCallRequested`. The name is the key without `ToolCall`, and the input is `args`.
- `tool_call` `completed` becomes `ToolCallOutput` (sanitized; only when non-empty), `ToolCallDone`, then `ItemDone` on the item `started` opened, matched by `call_id`.
- A `result` line becomes `TurnDone(EndTurn)`. With `is_error` it becomes `Error{fatal:false}` then `TurnDone(Error)`.
- Anything else becomes a `Notice(Warn)` quoting the sanitized line, capped at 200 characters. It is never an error, and mapping goes on.
Deviations:
- `cox-core` may not depend on `cox-sanitize`, so the caller passes the guard in as `fn(&str) -> String`. T35.5 must pass `cox_sanitize::sanitize`. The tests use a stand-in.
- `ToolCallRequested` is emitted beside `ItemStarted(ToolCall)`, because the surfaces draw calls from it. `TurnDone(Error)` follows `Error`, as in the native loop.
- The agent's error travels as `CoreError::Provider(BadRequest)`. A dedicated `CoreError` variant would be a protocol and schema change, which is flagged for the creator.
- The documented shapes leave some fields unset:
  - a completed call is always `ok: true`;
  - `risk` is `Exec`;
  - `subject` is empty;
  - `duration_ms` is 0.
- The $0 `billed_externally` usage row is left to T35.5. `--stream-partial-output` lines are not merged.
Check:
- `stream_json_assistant_line_maps_to_cox_event`, `stream_json_tool_call_started_and_completed_pair_map_to_one_item` and `unrecognised_stream_json_line_becomes_a_sanitized_notice` pass, plus `result_line_ends_the_turn_and_an_error_result_reports_it_first`.
- In the worktree: nextest 1041 passed, 3 skipped. PTY e2e tests timed out under load on the first run and passed on the rerun. fmt and clippy are clean.
- On main after landing (with T33.6, T33.16, T35.4, T35.3): nextest 1055 passed, 3 skipped. fmt, clippy and the slim build are clean.

#### T35.3 ACP client adapter

Depends: T35.2 · Size: ~190 · Files: `crates/cox-acp/src/client.rs` (new), `crates/cox-acp/src/lib.rs`
Goal: cox as an ACP client over the spawned process's stdio, reusing the `agent-client-protocol` crate `crates/cox-acp` already depends on as a server (no new dependency, per EA§4). `session/request_permission` from the agent is decided by `cox_permission::Engine`, the same single guard every other tool call goes through; an `fs/*` or `terminal/*` request is served only through `path::confine` and the sandbox policy already governing the spawned process, or refused with the reason named.
Check: `acp_client_relays_request_permission_through_the_engine`, `acp_client_fs_request_is_confined_to_the_workspace`, `acp_client_terminal_request_without_sandbox_grant_is_refused`.
Status: done 2026-09-26
Result: `cox_acp::client` makes cox an ACP client of an external agent over any `ConnectTo<Client>` transport. It reuses `agent-client-protocol`, so there is no new external crate.
- **Permission requests (`session/request_permission`):**
  - The agent's tool kind maps to the cox tool whose rules apply: Read→`read`, Search→`grep`, Fetch→`web_fetch`, Edit/Move→`edit`, Delete→`edit` (destructive). Anything else is judged as `bash`.
  - The request is judged by `cox_permission::Engine`, reached through `cox_core::permission`. The subject is sanitized.
  - The verdict maps onto the agent's options without granting more than the engine allowed. `Allow` picks allow-once only. `AllowForSession` picks allow-always and records the grant for this connection. `Deny` picks reject.
  - An `ask` goes to the `Approver` trait off the dispatch loop.
- **`fs/*` requests:**
  - Every path goes through `cox_sandbox::path::confine`. Because of this, `cox-acp` now also depends on `cox-sandbox`; `deps.rs` and the `plan.md` §1.1 dependency sentence are updated.
  - Writes follow the spawned process's sandbox policy: refused when it is read-only, and under `readonly_in_workspace` paths.
- **`terminal/create`** is always refused, with the reason named. `initialize_request()` advertises `terminal = false`.
- **Interface for T35.2/T35.5:** `connect(transport, ClientHost { roots, cwd, sandbox, engine, mode, approval, grants, approver }, main_fn)` and `initialize_request()`.
Deviations:
- Every terminal request is refused, not only those without a sandbox grant. Running a command with a grant (sandboxed spawn, output, wait/kill) would exceed the task size and add trust surface. It needs its own card if it is wanted.
- T35.2 must adapt tokio pipes to futures-io, for example with `tokio-util`'s `compat` feature, which is not enabled in the workspace yet.
Check:
- `acp_client_relays_request_permission_through_the_engine`, `acp_client_fs_request_is_confined_to_the_workspace` and `acp_client_terminal_request_without_sandbox_grant_is_refused` pass. `cox-acp`: 7 passed.
- fmt, clippy and the slim build are clean in the worktree. The full nextest run did not finish there because the disk filled up (ENOSPC); the full run is on main after landing.
- On main after landing (with T33.6, T33.16, T35.4, T35.3): nextest 1055 passed, 3 skipped. fmt, clippy and the slim build are clean.

#### T33.40.2 Jev guest crate: the System One wire

Depends: T33.27 · Size: ~190 · Files: `plugins/jev/src/lib.rs`, `plugins/jev/src/wire.rs`, `plugins/Cargo.toml` (member; `plugins/jev/Cargo.toml` and `plugins/jev/plugin.toml` are manifests)
Goal: the typed wire, pure and tested on the host target:
- `SystemOneRequest { state: Value, model, questions: BTreeMap<String, Question> }`, where `Question` is `Choice { instructions, criteria }`, `Score { instructions, levels }` or `Noul { instructions, criteria? }`;
- `Answer`, and `parse` returning a typed error with no default guessed.

It is ported from `crates/cox-provider/src/jev.rs` with two fixes:
- a Noul's certainty is `|2p − 1|`, not `p`, because the API returns no Noul confidence (J11);
- state plus the longest question is capped at 32k estimated tokens (J6).

The manifest declares `id = "jev"` and `[[provider]] name = "typesafe", api = "plugin"`. `decide`, `context` and `[[models]]` (`jev-1.13.0`, `jev-latest`, `context_window = 64000`, price 0.042/0.0) are added by later cards.
Plan: the extism-pdk glue sits behind `cfg(target_arch = "wasm32")`, so the pure modules test on the host. Port the six `jev.rs` tests with the documented bodies (J1, J2). A `just plugin-test` recipe runs the crate's tests, and the `plugins` CI job runs it.
Check: `cargo test --manifest-path plugins/Cargo.toml -p cox-plugin-jev`, with these tests:
- `choice_answer_parses_with_probabilities`;
- `score_answer_parses_with_legend`;
- `noul_certainty_is_distance_from_half`;
- `missing_answers_is_an_error_not_a_guess`;
- `unknown_answer_kind_is_an_error`;
- `state_over_32k_tokens_is_truncated_with_marker`;
- `manifest_validates` (through `cox-plugin-api`'s parser).

`cargo build … --target wasm32-unknown-unknown` succeeds.
Status: done 2026-09-26
Result: `plugins/jev` (`cox-plugin-jev`, cdylib + rlib) is a member of the guest workspace.
- **`src/wire.rs`** is the pure System One wire, ported from `crates/cox-provider/src/jev.rs`: `SystemOneRequest`, `Question` (`Choice`, `Score`, `Noul`), `Answer`, and `parse(body) -> Result<BTreeMap<String, Answer>, WireError>`.
  - A Noul's certainty is `|2p − 1|` (J11).
  - `SystemOneRequest::new` keeps state plus the longest question within 32k estimated tokens (J6). It truncates only the state, at a char boundary, and adds a marker; a question is never cut.
  - Missing or empty `answers` is an error, never an empty map.
- **`src/lib.rs`:** the extism glue sits behind `cfg(target_arch = "wasm32")`, so the pure module tests on the host.
- **`plugin.toml`:** `id = "jev"` and `[[provider]] name = "typesafe", api = "plugin"`.
- **`just plugin-test`** runs the guest workspace tests.
- **CI:** a new `plugins` job runs the guest tests and the wasm32 build, and is added to `revert-on-failure`'s `needs`.
Deviations:
- figment is a dev-dependency here, only for `manifest_validates`. It is already in `toolchain.md`, used by cox-config.
- One extra test: `state_under_cap_is_untouched`.
Check:
- `cargo test --manifest-path plugins/Cargo.toml -p cox-plugin-jev`: 8 passed. That covers the card's seven tests: `choice_answer_parses_with_probabilities`, `score_answer_parses_with_legend`, `noul_certainty_is_distance_from_half`, `missing_answers_is_an_error_not_a_guess`, `unknown_answer_kind_is_an_error`, `state_over_32k_tokens_is_truncated_with_marker` and `manifest_validates`.
- The wasm32 build succeeds.
- The guest workspace's fmt and clippy are clean on the host and on wasm32.
- Root workspace in the worktree: fmt and clippy are clean; nextest 1037 passed, 3 skipped. Two PTY/MCP e2e tests failed under load and passed on the rerun.
- On main after landing: nextest 1055 passed, 3 skipped. fmt, clippy and the slim build are clean; the guest workspace tests and the jev wasm32 build pass.

#### T33.19 MCP servers from plugins

Depends: T33.6, T32.3 · Size: ~160 · Files: `crates/cox-mcp/src/discovery.rs`, `crates/cox/src/session.rs`
Goal: `[[mcp]]` entries become the lowest-precedence discovery source (`plugin:<id>`), named `<id>-<name>`. `crates/cox` wraps a stdio command with the sandbox policy before `cox-mcp` spawns it. An in-package command is covered by the digest; an HTTP `url` must be in `net`.
Check: `project_mcp_json_shadows_plugin_server`, `plugin_stdio_server_runs_under_sandbox` (it writes outside the workspace and is denied; macOS and Linux paths as in T4.1/T4.2), `changing_bundled_server_binary_changes_digest`.
Status: done 2026-09-26
Result: a granted plugin's `[[mcp_servers]]` join MCP discovery.
- `cox_mcp::discovery::add_plugin(found, id, servers)` adds them at the lowest precedence, named `<id>-<name>`, with source `plugin:<id>`, and without `${VAR}` expansion. `Discovered::sources` is now `HashMap<String, String>`.
- `crates/cox/src/session.rs`: `load_plugins(..., writable) -> Plugins { notices, mcp }` walks granted plugins once. `plugin_program` refuses a symlinked server binary. `sandboxed_argv` wraps a stdio server with `cox_tools::sandbox::command` through `/bin/sh -c 'exec "$0" "$@"'`. On a host with only Landlock or no backend, a plugin server is refused with a warning.
- An HTTP server's url host must pass the plugin's `net` capability: `Capabilities::net_allows(host)` in `cox-plugin-api` (exact hosts, strict subdomains of a wildcard).
- A bundled server binary is part of the plugin digest, so changing it re-asks for the grant.
Check:
- `project_mcp_json_shadows_plugin_server`, `plugin_stdio_server_runs_under_sandbox`, `changing_bundled_server_binary_changes_digest` pass, plus `symlinked_server_binary_is_refused`, `plugin_http_server_needs_its_host_in_net`, `net_allows_exact_hosts_and_strict_subdomains_of_a_wildcard`.
- A real-binary run against a scratch `COX_HOME` showed the Seatbelt denial for a plugin server writing outside its roots.
- On main after landing: nextest 1061 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.9 Base host functions and the context snapshot

Depends: T33.6 · Size: ~190 · Files: `crates/cox-plugin/src/hostfn.rs`, `src/context.rs`
Goal: `cox_log`, `cox_notify` (level capped at `Warn`), `cox_kv_*` (through `PluginStore`), `InitIn.config` from `[plugins.<id>]` (the `PluginsConfig` flatten, following `HooksConfig`), and `cox_context` folded from events. Each host function checks the grant and the calling context (PL§4).
Check: `notify_cannot_raise_security_level`, `kv_denied_without_capability`, `context_snapshot_is_redacted` (a secret in a tool result does not reach the snapshot); the config docs drift test is green with the new section.
Status: done 2026-09-26
Result: the base `cox:host/v1` host functions exist on the wire the SDK (T33.27) expects: one JSON block in, `{"Ok":..}` or `{"Err":AbiError}` out.
- **`crates/cox-plugin/src/hostfn.rs`** implements `cox_log`, `cox_notify`, `cox_kv_get`, `cox_kv_put`, `cox_kv_delete` and `cox_context`.
  - Each call checks the grant (`NotGranted`) and the running export (`NotInThisContext` for notify and kv inside `cox_render`/`cox_render_item`).
  - Notify: any level other than `info` becomes `Warn`, so a plugin never raises `Security` or `Budget`. Text is sanitized and secret-redacted; at most 16 notices queue until the session drains `HostEnv::take_notices()`.
  - Log goes to `tracing`, 20 lines per second per plugin; dropped lines are counted.
  - Kv goes through `PluginStore` as JSON; a quota refusal is `TooLarge` with the limit hit; keys are capped at 256 bytes.
  - All 12 ABI imports are registered so any SDK plugin links; the six later ones answer `Err(Failed)` until their cards replace the arm in `HostEnv::dispatch`.
  - `init_input(manifest, &PluginsConfig, SessionInfo) -> InitIn`: `config` is the plugin's `[plugins.<id>]` table or `{}`.
- **`crates/cox-plugin/src/context.rs`**: `Context::fold(&Event)` and `snapshot()` (session id, cwd, main tier and model, usage totals, last 50 items without thinking, the last successful todo list, compactions). Redaction runs on read, string by string, so a secret split across two deltas is still caught.
- `PluginHost::load` keeps its signature and calls the new `load_with(wasm, limits, Arc<HostEnv>)` with an environment that grants nothing.
- `PluginsConfig` gains flattened `entries` for `[plugins.<id>]`; config schema and docs regenerated; `docs/plugins.md` describes kv limits, the snapshot and `InitIn.config`.
Deviations:
- `scrub`/`REDACTED` moved from `cox-core::redact` to `crates/cox-sanitize/src/redact.rs` (cox-plugin may depend on cox-sanitize only); cox-core re-exports them at the old path and keeps `scrub_event`; `deps.rs` allows cox-core → cox-sanitize.
- `PluginStore::kv_delete(plugin_id, key)` added (trait had only `kv_delete_all`), with `kv_delete_removes_only_that_key`.
- `KV_VALUE_LIMIT`/`KV_PLUGIN_LIMIT` moved to `cox_protocol::traits` so host and store report the same limits.
- 19 files, ~1190 lines, well over the ~190 estimate.
Not done (later cards):
- Session wiring: `session.rs` still loads through the grant-nothing environment and drops the host; keeping a live instance (`HostEnv::new(id).with_grant(..).with_context(..)`, `load_with`, `init`, draining notices) needs an `Arc<dyn PluginStore>`.
- Nothing calls `Context::fold` yet; T33.10's event tap should.
- A project `.cox/config.toml` can set a user plugin's `[plugins.<id>]` table; no guard added. A plugin id `enabled` would collide with the fixed key.
Check:
- `notify_cannot_raise_security_level`, `kv_denied_without_capability`, `context_snapshot_is_redacted` pass; `plugin_table_flattens_into_entries` added.
- In the worktree: nextest 1062 passed, 3 skipped (3 load timeouts in `mcp_serve`/`plain` passed on rerun); fmt and both clippy runs clean.
- On main after landing (with T33.9, T33.7, T35.12): nextest 1074 passed, 3 skipped, 1 load timeout (`cox_mcp_serves_read_grep_and_glob_from_the_built_binary`) passed on its own rerun; fmt, clippy, the slim build and the guest `cargo test` clean.

#### T33.7 `cox plugin install | enable | disable`

Depends: T33.6 · Size: ~180 · Files: `crates/cox/src/plugin_cmd.rs`, `crates/cox/src/cli.rs`
Goal:
- `install <dir>` copies into `versions/<digest12>/` and writes `current` atomically; the only v1 source is a local path, recorded in the grant's `source`.
- `enable [--project] [--yes]` prints the capabilities in words and asks on stdin.
- `disable` clears `enabled`.
Check: e2e in a scratch `COX_HOME`: install → enable `--yes` → `list` shows `loaded`; `disable` → `not loaded`; `project_plugin_needs_project_grant`.
Status: done 2026-09-26
Result: `cox plugin install <dir> [--yes]`, `cox plugin enable <id> [--project] [--yes]` and `cox plugin disable <id> [--project]` exist, and `cox plugin list` runs a real `grant::check` per plugin.
- `list` shows `granted`, `disabled` or `needs_approval` (with `grant_added`/`grant_removed`) instead of `unknown`, plus `loaded: bool` (`loaded`/`not loaded` in the human form). A missing or unreadable store counts as no grant.
- `install` always targets the user scope (`~/.cox/plugins/<id>/versions/<digest12>/`): it parses, validates and digests the source directory through the same `load_manifest` path `discover` uses, copies it, writes `current` by temp file and rename, then runs the same approval flow as `enable`, recording `source = {kind: "path", path, digest}` (PL§1).
- The shared `decide()` prints name, description and the capability list, each line through `cox_sanitize::sanitize`, and prompts `[y/N]` unless `--yes`; approval writes the grant in the shape `grant::capability_list` defines. A decline writes no row, so the next `enable` sees `NeedsApproval`, never a stale `Disabled`.
- `enable`/`disable` look for project plugins only with `--project`; the grant is scoped to that repository root.
Deviations:
- `cox_plugin::discover::load_manifest` is now `pub` and takes `expect_id: Option<&str>`; `cox_store::now_rfc3339` is now `pub` (reused for `decided_at`).
- The two T33.4 assertions in `crates/cox/tests/plugin_cli.rs` now expect `needs_approval` and `loaded == false`.
Not done: `update`/`remove`/`new` (their own cards) and the TUI grant dialog (T33.8). `declared_summary` in `list` ignores `kv`/`context`/`model`, so such a plugin reads "no declared capabilities" though `grant` lists them; cosmetic, pre-existing.
Check:
- `install_then_enable_yes_then_list_shows_loaded_then_disable_shows_not_loaded` and `project_plugin_needs_project_grant` pass.
- In the worktree: nextest 1058 passed, 3 skipped; fmt and both clippy runs clean. A real-binary run against a scratch `COX_HOME` (install → decline → list → enable --yes → list → disable → list --json) matched the Check's wording.
- On main after landing (with T33.9, T33.7, T35.12): nextest 1074 passed, 3 skipped, 1 load timeout (`cox_mcp_serves_read_grep_and_glob_from_the_built_binary`) passed on its own rerun; fmt, clippy, the slim build and the guest `cargo test` clean.

#### T35.12 A dedicated error for a failed external agent

Depends: T35.4 · Size: ~80 · Files: `crates/cox-protocol/src/errors.rs` (`CoreError`), `crates/cox-core/src/external_agent.rs`, `docs/protocol.jsonschema` (generated)
Goal: T35.4 reports an external agent's `result` with `is_error: true` as `CoreError::Provider(ProviderError::BadRequest { message })`, because `CoreError` has no better variant. That misnames the failure: the provider did not fail, and a surface or a retry rule cannot tell the two apart.
- Add `CoreError::ExternalAgent { agent, message }`, with the message sanitized by the caller-supplied guard, as T35.4 already does.
- Map the error `result` line onto it.
- Regenerate `docs/protocol.jsonschema` with its drift test.
- Every surface that matches on `CoreError` (TUI, stream-json, ACP) shows it as the external agent's error. It must not trigger provider retry or fallback.
Check: `stream_json_error_result_is_an_external_agent_error` (it replaces the `BadRequest` expectation in `result_line_ends_the_turn_and_an_error_result_reports_it_first`), `external_agent_error_is_not_retried_as_a_provider_error`; the protocol schema drift test is green.

**Order.** T35.0 → T35.1 → T35.2 is the critical path (it also waits on T33.6, T33.19, T33.42, T34.1, T34.5, whichever lands last). After T35.2: T35.3 and T35.4 run in parallel → T35.5 → (T35.6, T35.8 in parallel) → T35.7 → T35.9; T35.10 runs whenever the creator has a key. The top table gets rows T35.0–T35.10; P1 for the design doc and the critical path through the host spawner and the preset wiring (T35.0–T35.2, T35.5), P2 for the two drivers, the plugin package, the fixture e2e, doctor reporting and the user guide (T35.3, T35.4, T35.6–T35.9), P3 for the optional live check (T35.10).
Status: done 2026-09-26
Result: an external agent's error `result` is now `CoreError::ExternalAgent { agent, message }` (tag `external_agent`, shown as "external agent {agent} failed: {message}"), not `CoreError::Provider(BadRequest)`.
- `StreamJsonMapper::new(turn, agent: impl Into<String>, sanitize)` takes the agent's plugin or preset id; T35.5 passes it.
- Every surface renders `CoreError` through `Display` (TUI `Cell::Error`, stream-json `plain.rs`, ACP), and none matches on a variant, so no surface code changed.
- The only retry classifier, `cox_provider_http::retry::retryable(&ProviderError)`, cannot take a `CoreError`, so the new variant can never reach provider retry or fallback.
- `docs/protocol.jsonschema` regenerated by its drift test (one additive block); `plan.md` §1.14's `CoreError` row lists the variant.
Check:
- `stream_json_error_result_is_an_external_agent_error` (replaces `result_line_ends_the_turn_and_an_error_result_reports_it_first`) and `external_agent_error_is_not_retried_as_a_provider_error` pass.
- In the worktree: nextest 1056 passed, 3 skipped; fmt, both clippy runs and the schema drift test clean.
- On main after landing (with T33.9, T33.7, T35.12): nextest 1074 passed, 3 skipped, 1 load timeout (`cox_mcp_serves_read_grep_and_glob_from_the_built_binary`) passed on its own rerun; fmt, clippy, the slim build and the guest `cargo test` clean.

#### T35.5 Wiring as a subagent preset

Depends: T35.3, T35.4, T34.1, T34.5 · Size: ~190 · Files: `crates/cox-core/src/subagent.rs`, `crates/cox-core/src/session.rs`, `crates/cox-core/src/tasks.rs`
Goal: a granted `[[external_agents]]` entry registers as one more name `AgentTool::preset()` resolves (T34.1's path), so `agent(preset: "cursor")` dispatches it exactly like a discovered `.cox/agents/*.md` definition, picking its ACP (T35.3) or stream-json (T35.4) driver from the manifest's `mode`; a follow-up to a running or finished external-agent task, and a sibling message addressed to it, are routed the same way T34.5 already routes to any other child task — no second messaging path. Usage is recorded per EA§6 (a `$0`/`billed_externally: true` row unless the driver ever reports tokens).
Check: `agent_dispatches_a_granted_external_agent_preset_by_name`, `task_message_reaches_a_running_external_agent_task` (reusing T34.5's fixture shape), `external_agent_turn_writes_a_billed_externally_usage_row`.
Status: done 2026-09-26
Result: the core side of external agents as subagent presets. The host drivers are split into T35.13.
- `cox_protocol::traits::ExternalAgent { name(); async turn(turn, prompt, events, cancel) -> Result<Option<Usage>, CoreError> }`. `turn` returns `None` when the agent reported no tokens.
- `Session::set_external_agents(Vec<Arc<dyn ExternalAgent>>)` is set once, like `set_agent_defs`. `run_turn_inner` hands a child's turn to `external_turn` when a driver is set. `external_turn` forwards the driver's events, writes each `AssistantMessage` into history, holds back the driver's `TurnDone`, writes the usage row, then emits `Usage` and ends the turn. A driver error becomes a non-fatal `Event::Error`.
- The usage row is `provider = ProviderId::External` (new variant), `model = <name>`, `job = Agent`, `cost_usd = 0`, with tokens only when the driver reported them. This is EA§6's "billed externally" without a new column.
- `subagent::resolve` looks up built-ins, then discovered defs, then external agents. An external preset gets no cox tools, `risk` is `Exec`, and the names appear in the tool description and in the unknown-preset error. `Spec` carries the driver, so a parked child resumes on the same one. Follow-ups and sibling messages use T34.5's path unchanged.
Deviations:
- Files are `cox-protocol` `traits.rs`/`types.rs` and `cox-core` `session.rs`/`subagent.rs`. The trait has to live in cox-protocol because cox-core does no I/O, and `tasks.rs` needed no change. About 240 lines without tests.
- Picking the ACP or stream-json driver by `mode` is host work (both drivers do I/O), so it moved to T35.13.
Check: `agent_dispatches_a_granted_external_agent_preset_by_name`, `task_message_reaches_a_running_external_agent_task` and `external_agent_turn_writes_a_billed_externally_usage_row` pass (in-process fake driver). In the worktree: nextest 1064 passed, 3 skipped; fmt and both clippy runs clean.
- On main after landing: nextest 1078 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.11 Hooks: plugins as a hook source

Depends: T33.9 · Size: ~190 · Files: `crates/cox-ext/src/hooks.rs`, `crates/cox-core/src/hooks.rs`, `crates/cox-plugin/src/hooks.rs`
Goal:
- `Hook::interested(event)` with the config check as the default, so `fire_configured` asks the hook instead of `[hooks]`.
- The `ShellHooks` chaining loop becomes one shared function used by a new `HookChain`.
- `PluginHooks` implements `Hook` through `cox_hook`.
- Order: shell hooks first, then plugins by id; the deadline is the smaller of `hooks.timeout_s` and `limits.call_ms`.
Check: `plugin_hook_fires_without_hooks_config`, `shell_block_wins_over_plugin`, `plugin_modify_chains_into_next_hook`, `plugin_hook_timeout_fails_open_with_notice`; invariant 10 `broken_hook_is_skipped_not_fatal` still green.
Status: done 2026-09-26
Result: plugins can be a hook source through one shared chaining loop.
- `Hook::interested(event, &HooksConfig)` defaults to the old `[hooks]` check. `fire_configured` asks the installed hook. `PresenceHook::interested` forwards to the hook it wraps.
- `cox_ext::hooks::chain(steps, payload, step)` is the one chaining loop. The first `Block` or `Failed` stops it, and a `Modify` feeds `tool_input` to the later steps. `ShellHooks::run` uses it; the old index assignment that panicked on a non-object payload is now a safe insert.
- `HookChain::new(shell: Option<Arc<dyn Hook>>, plugins: Vec<(String, Arc<dyn Hook>)>)` runs shell hooks first, then plugins sorted by id, through the same `chain`.
- `cox_plugin::PluginHooks::new(id, Arc<PluginHost>, granted)` answers only its granted `hooks:<Event>` lines. It calls `cox_hook` with `HookCall { event: "<serde HookEvent>", payload }` on the control lane inside `spawn_blocking` and expects a `HookOutcome` tagged by `type`. A trap, timeout, bad output or missing export becomes `Failed { "plugin <id>: …" }`, so it fails open. The deadline is `min(hooks.timeout_s, limits.call_ms)`.
Deviations:
- `interested` takes the `HooksConfig` too, since the default must be the config check. 9 files: the trait lives in `traits.rs`, `PresenceHook` needed `interested`, and `crates/cox-plugin` needed `lib.rs`, `Cargo.toml` (async-trait and tokio, both existing workspace deps) and a shared WAT test helper.
- `plugin_hook_timeout_fails_open_with_notice` lives in cox-plugin, which cannot depend on cox-core. It proves the timeout returns `Failed` within the deadline; `broken_hook_is_skipped_not_fatal` proves the core turns `Failed` into the notice.
Not done: the session still installs `PresenceHook(ShellHooks)`; the `HookChain` wiring is T33.44.
Check:
- `plugin_hook_fires_without_hooks_config`, `shell_block_wins_over_plugin`, `plugin_modify_chains_into_next_hook`, `plugin_hook_timeout_fails_open_with_notice` and the added `plugin_hook_answers_only_its_granted_events` pass; `broken_hook_is_skipped_not_fatal` still passes.
- In the worktree: nextest 1080 passed, 3 skipped; fmt and both clippy runs clean.
- On main after landing (with T33.11, T33.42, T33.8, T33.17, T33.10, T33.31): nextest 1110 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.42 Sandbox every MCP stdio server, with a per-server opt-out

Depends: T33.19 · Size: ~140 · Files: `crates/cox-mcp/src/client.rs`, `crates/cox/src/session.rs`, `crates/cox-protocol/src/config.rs`
Goal: extend the sandbox wrap from T33.19 to every MCP stdio server, not only plugin-shipped ones (`docs/design/plugins.md` §7c, resolved 2026-09-26, §14 decision 4). `crates/cox` wraps every stdio command with `sandbox::Policy` before `cox-mcp` spawns it, including today's user-configured `.mcp.json`/config servers. A per-server `sandbox = false` key opts a named server out, for setups that need it, and `cox doctor` gains a row naming any server that opted out.
Check: `every_stdio_server_runs_under_sandbox_by_default`, `sandbox_false_opts_a_named_server_out`, `doctor_lists_unsandboxed_servers`; existing `.mcp.json`/config MCP e2e tests still pass with the wrap applied.
Status: done 2026-09-26
Result: every stdio MCP server now runs under the session's `sandbox::Policy`, not only plugin ones.
- `McpServerConfig.sandbox: bool` defaults to `true`. Servers from `.mcp.json` or `~/.claude.json` are always sandboxed; only cox's own `[mcp.servers.<name>]` can set `sandbox = false`.
- `crates/cox/src/session.rs`: T33.19's `sandboxed_argv` is no longer plugin-gated and is the one wrap. `sandbox_stdio_servers(found, config, writable)` wraps each non-plugin stdio server before `connect_all` spawns it, and skips the wrap under `danger-full-access`. On a Landlock-only or no-backend host, user servers run unwrapped with one aggregated Warn notice; plugin servers are still refused (T33.19).
- The project-config guard (`mcp.servers.*.sandbox`) reverts any `sandbox = false` that the project layer causes, for existing and new servers alike, since the sandbox is what contains a command a cloned repository chose.
- `cox doctor` has an `mcp sandbox` row naming every stdio server with `sandbox = false`.
- Config schema and docs regenerated. `docs/compat.md` and `docs/how-it-works.md` updated, as is the §1.6 `[mcp]` example.
Deviations: about 330 changed lines (roughly 155 without tests), over the ~140 estimate. Files outside the card's list: `cox-config` `load.rs` for the guard, `doctor.rs`, `cox-mcp` `discovery.rs`, and one fixture fix in `client.rs`.
For T35.2: `sandboxed_argv(program, args, config, writable) -> Result<Vec<String>, String>` is the wrap to reuse. `sandbox_stdio_servers` shows the warn-and-continue pattern; `plugin_mcp` shows wrap-or-refuse.
Check:
- `every_stdio_server_runs_under_sandbox_by_default`, `sandbox_false_opts_a_named_server_out`, `doctor_lists_unsandboxed_servers` and `config_project_cannot_disable_an_mcp_server_sandbox` pass.
- In the worktree: nextest 1062 passed, 3 skipped (3 PTY/e2e load timeouts passed on rerun); fmt and both clippy runs clean.
- On main after landing (with T33.11, T33.42, T33.8, T33.17, T33.10, T33.31): nextest 1110 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.8 TUI grant dialog

Depends: T33.6 · Size: ~150 · Files: `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/modal.rs`, `crates/cox/src/session.rs`
Goal: `Modal::PluginGrant` asks for each `NeedsApproval` plugin at session open, queued in the single modal slot. `y` grants that digest and `n` skips it for this session. Project plugins show their repository in warning style. Every manifest string is sanitized.
Check: insta snapshots (a new plugin, a widened plugin with the diff, a project plugin); `grant_dialog_sanitizes_description`.
Status: done 2026-09-26
Result: the TUI asks for plugin grants at session open.
- `Modal::PluginGrant(PluginGrantDialog)` shows name, description and the capability list for a new plugin, the added/removed diff for a widened one, and the repository for a project plugin (theme warning token). Every manifest string passes through `sanitize` at render time.
- `State.pending_grants` queues one dialog per `NeedsApproval` plugin. `y` grants, `n` skips, and the next dialog pops.
- cox-tui never touches the store. `Cmd::PluginGrant(GrantDecision)` goes through a channel `app::run` takes to the `poll` task in `crates/cox`, the same pattern as `Cmd::PersistConfig`.
- `plugin_cmd::write_grant(store, id, scope, digest, capabilities, source)` is the one place a `PluginGrant` row is built, with `cox_store::now_rfc3339()`. The CLI `decide()` (T33.7) and the TUI path both call it.
- A plugin granted in the dialog loads next session, since the session's tool and plugin set is fixed at construction. Headless and ACP are unchanged and still only warn.
Deviations: `view.rs`, `app.rs` (`run` gains a grants sender) and `bin/kitty_probe.rs` changed too, forced by the exhaustive `Modal`/`Cmd` matches and the probe's `app::run` call.
Check:
- `grant_dialog_sanitizes_description` and the snapshots `new_plugin_grant_snapshot`, `widened_plugin_grant_shows_the_diff` and `project_plugin_grant_shows_its_repository` pass.
- In the worktree after rebasing on T33.7: nextest 1082 passed, 3 skipped; fmt and both clippy runs clean. `cox doctor` against a scratch `COX_HOME` ran clean apart from the expected missing key.
- On main after landing (with T33.11, T33.42, T33.8, T33.17, T33.10, T33.31): nextest 1110 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.17 Providers, declarative form

Depends: T33.16 · Size: ~130 · Files: `crates/cox/src/session.rs`, `crates/cox-plugin/src/provider.rs`
Goal: a `[[provider]]` with `api = "chat" | "responses"` merges into `providers.custom` as a `CompatibleProviderConfig`. A user config section of the same name wins. The key comes through `resolve_key(api_key_env, name)`.
Check: `plugin_chat_section_builds_openai_shaped_client` (wiremock), `config_section_shadows_plugin_section`, `plugin_provider_request_has_usage_row`.
Status: done 2026-09-26
Result: a granted plugin's `[[provider]]` with `api = "chat" | "responses"` joins `providers.custom`.
- `cox_plugin::provider::merge(&ProvidersConfig, &[PluginProviders]) -> Merged { custom, warnings }` is pure. It walks plugins in id order and adds a `CompatibleProviderConfig { base_url, api_key_env, api }` for each declaration. It skips with a warning:
  - a reserved native name;
  - a name the user config already has, because config wins;
  - a name a lower-id plugin already claimed;
  - `auth = x-api-key`, because the OpenAI-shaped client sends only `Authorization: Bearer`.
  `api = "plugin"` rows are left for T33.18.
- `load_plugins` returns the merged `providers` and the warnings as notices. `open()` now loads plugins before it builds the session's provider, so `tiers.<tier>.provider` can name a plugin provider. The key resolves through the existing `resolve_key(api_key_env, name)`.
- No `net_allows` check: the grant's capability list already names each provider's base URL (PL§7a), and `net` governs `cox_http`.
Deviations: `open()` moves plugin loading earlier. `crates/cox` gains `wiremock` as a dev-dependency (already a workspace dependency).
Check:
- `plugin_chat_section_builds_openai_shaped_client` (wiremock), `config_section_shadows_plugin_section`, `plugin_provider_request_has_usage_row` and six more unit tests pass.
- In the worktree: nextest 1070 passed, 3 skipped; fmt and both clippy runs clean.
- On main after landing (with T33.11, T33.42, T33.8, T33.17, T33.10, T33.31): nextest 1110 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.10 Events: the tap, the rings, `cox_on_event`

Depends: T33.9 · Size: ~180 · Files: `crates/cox-protocol/src/traits.rs`, `crates/cox-core/src/session.rs`, `crates/cox-plugin/src/events.rs`
Goal: `EventTap` in `cox-protocol`, and `Session::set_event_tap`, called in `emit` after the rollout append with the scrubbed event. A per-plugin ring of 256 with drop-oldest and a counter, delivered in batches with `first_seq`/`dropped`. `Effects.redraw` is forwarded.
Check:
- `tap_never_blocks_emit`: a plugin that sleeps in `cox_on_event` does not slow a scripted 50-turn session beyond noise;
- `ring_drops_oldest_and_counts`;
- `plugin_sees_only_subscribed_kinds`;
- invariant 7 `no_event_after_turn_done` still green.
Status: done 2026-09-26
Result: the session has an event tap, and plugins get their subscribed events in batches.
- `cox_protocol::traits::EventTap { fn offer(&self, seq, &Event) }`. `Session::set_event_tap` works once, like `set_external_agents`; child sessions get no tap. `emit` offers the scrubbed event right after `rollout_append`, with the rollout `seq`.
- `cox_plugin::events::PluginTap`:
  - `offer` folds the event into the shared `Context`, serializes it once, and pushes it into each subscribed plugin's ring.
  - A ring holds 256 events (`EVENT_DEPTH`), drops the oldest and counts what it dropped.
  - One pump thread per plugin calls `cox_on_event` with `EventBatch { first_seq, dropped, events }` on the event lane, with a 50 ms deadline.
  - `Effects.notices` go to the plugin's notice queue, and `Effects.redraw` calls the `Redraw` callback with the plugin id.
  - A plugin without `cox_on_event` has its feed closed. A call error is logged and the batch skipped.
- `subscriptions(subscribe, granted)` returns what `InitOut.subscribe` asked for, limited to the `events:<tag>` grant lines.
- `HostEnv::notify` is now the one notice path, shared by `cox_notify` and `Effects.notices`.
Deviations:
- `offer` takes a plain lock rather than PL§5's `try_lock`, because a failed `try_lock` would lose an event while the pump swaps the ring. The lock is never held while the plugin runs.
- Files outside the card's list: `hostfn.rs`, `lib.rs`, and `Cargo.toml`. `Cargo.toml` adds only dev-dependencies: `cox-core`, `cox-provider` and `tokio`, for the scripted-session tests.
Not done (T33.44): nothing builds a `PluginTap` or calls `set_event_tap` yet. The tap never sees `SessionStarted`, so the session must fold it into the `Context` itself. There is no detach and no cut-off for a plugin that keeps timing out.
Check:
- `ring_drops_oldest_and_counts`, `plugin_sees_only_subscribed_kinds`, `tap_never_blocks_emit` (a 50-turn scripted session with a plugin sleeping 100 ms per batch), `subscriptions_are_what_was_asked_and_granted` and `effects_redraw_and_notices_are_forwarded` pass.
- The invariant 7 test `turn_no_event_after_turn_done` still passes.
- In the worktree: nextest 1083 passed, 3 skipped; fmt and both clippy runs clean.
- On main after landing (with T33.11, T33.42, T33.8, T33.17, T33.10, T33.31): nextest 1110 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.31 `cox plugin update` and rollback

Depends: T33.7 · Size: ~190 · Files: `crates/cox/src/plugin_cmd.rs`, `crates/cox-plugin/src/install.rs`
Goal: PL§1b: re-read the recorded path source, validate the schema, `api` and digest, print a capability diff, and support `--check`. Staging uses temp and rename; `current` is swapped only after approval; one `previous` is kept. `--rollback` reuses the stored grant for that digest. Headless and ACP never approve a widening.
Check: e2e offline against a local path: install → rebuild with changed bytes → `update --check` shows the diff → `update` requires a re-grant → `--rollback` restores the old digest without asking; `update_in_headless_keeps_current_and_warns`.
Status: done 2026-09-26
Result: `cox plugin update [ids…] [--all] [--check] [--rollback] [--yes]` (PL§1b).
- `crates/cox-plugin/src/install.rs` is the only code that changes a user plugin's layout, `<home>/plugins/<id>/{current, previous, versions/<d12>/}`:
  - `stage` copies into `versions/<d12>.tmp`, fsyncs, re-digests the copy (rejecting bytes changed since validation), then renames it into place.
  - `activate` moves the replaced version to `previous`, swaps `current` by temp file and rename, and prunes other versions.
  - `install` uses both, so there is no second copy of that logic.
- `update` re-reads the `{kind: "path", path, digest}` source recorded on the current grant (install now stores the canonical path), validates it, and prints the capability diff from `grant::check`.
  - `--check` changes nothing.
  - Approval goes through `decide`, then `activate`. A digest that already has a grant switches without asking.
- `--rollback` re-digests `versions/<previous>` through `load_manifest` and switches back with the stored grant.
- Headless means no TTY and no `--yes`: `update` warns and keeps `current`, even with `y` piped in.
Deviations:
- Files outside the card's list: `cli.rs`, `main.rs`, `lib.rs` and the e2e tests.
- Reinstalling over a plugin now sets `previous` and prunes older versions.
- A declined update leaves its staged version until the next successful swap prunes it.
Not done: no in-session notice or TUI `/plugin update`, and only user plugins are covered.
Check:
- `update_check_regrant_then_rollback_restores_old_digest` (install → changed bytes → `--check` diff → re-grant → `--rollback` without asking) and `update_in_headless_keeps_current_and_warns` pass, plus three `install.rs` unit tests.
- In the worktree: nextest 1080 passed, 3 skipped; fmt and both clippy runs clean.
- On main after landing (with T33.11, T33.42, T33.8, T33.17, T33.10, T33.31): nextest 1110 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.23 TUI status segments

Depends: T33.10, T33.22 · Size: ~160 · Files: `crates/cox-tui/src/status.rs`, `crates/cox-tui/src/state.rs`, `crates/cox/src/session.rs`
Goal: `Msg::Plugin(PluginUiMsg)` and `Cmd::Plugin(PluginRequest)`. `status.left`/`status.right` segments are cached in `State`. `cox_render` runs only on redraw requests, resize or visibility, never from `view`. Plugin segments drop first on a narrow terminal. A render has 20 ms, and three misses show "⚠ <id> slow".
Check: insta snapshots (wide and narrow); `view_never_calls_plugin` (a counting fake bus); `slow_render_keeps_last_good_segment`.
Status: done 2026-09-26
Result: plugins can put segments in the TUI status line.
- `Msg::Plugin(PluginUiMsg { Declare, Redraw, Rendered, Missed })` and `Cmd::Plugin(PluginRequest::Render { plugin, input })`; `State.plugin_status` caches each segment's last good widget and its miss count.
- `status::on_plugin` and `status::render_requests` are the only places a render is requested: on `Declare`, a redraw or a resize, never from `view`. `status.left` segments sit before the model segment and `status.right` after the mode segment. On a narrow terminal plugin segments drop first, rightmost first.
- A segment is 24 columns (`SEGMENT_COLS`), drawn through the existing `plugin_ui::render`, so sanitizing and theme tokens share one path. After three misses in a row (`MAX_MISSES`) the slot shows `⚠ <id> slow` in the warn colour and is not asked again that session.
- `crates/cox/src/plugin_ui.rs` (`plugins` feature): `serve(hosts, rx, feed)` answers one request at a time with a 20 ms deadline (`RENDER_DEADLINE`) and skips duplicates queued behind a slow render. `app::run` takes the request sender, and `Cmd::Plugin` is sent with `try_send`.
Deviations:
- About 330 lines over 7 files plus 2 snapshots. The render seam is its own module, so `session.rs` changes only 9 lines.
- A segment coming back after a narrow-terminal drop does not trigger a render of its own. Row width still counts bytes, as before.
Not done (T33.44): `session.rs` passes `serve` an empty host list, `plugin_ui::redraw` is unused, and nothing sends `Declare` after `cox_init`.
Check:
- The snapshots `plugin_segments_wide` and `plugin_segments_drop_first_when_narrow` pass, as do `view_never_calls_plugin` (a counting fake bus: 40 view renders and 10 ticks make no call) and `slow_render_keeps_last_good_segment`.
- Also added: `plugin_segment_text_is_sanitized_and_capped`, plus `render_answer_carries_the_widget` and `slow_render_is_missed_at_its_deadline` (WAT fixture).
- In the worktree: nextest 1117 passed, 3 skipped; fmt and both clippy runs clean.
- On main after landing: nextest 1116 passed, 3 skipped, 1 load failure (`bash_cancel_stops_the_command`) passed three reruns on its own; fmt, clippy and the slim build clean.

#### T35.2 Host spawner, sandbox and grant — blocker

Depends: T35.1, T33.6, T33.19, T33.42 · Size: ~190 · Files: `crates/cox-plugin/src/external_agent.rs` (new), `crates/cox/src/session.rs`
Goal: resolve a granted `[[external_agents]]` entry to a `std::process::Command` (in-package path or PATH program), the same resolution shape T33.19 gives `[[mcp]]`; `crates/cox` wraps it with `sandbox::Policy` before spawning, exactly as it already does for a plugin's MCP stdio server (PL§7c) — no second sandbox path. The capability is one more line the grant dialog lists in words (PL§2's "the capability list is the unit of approval"); `grant::check` needs no change, since it already treats the manifest's capability set generically.
Check: `external_agent_command_is_wrapped_by_sandbox_before_spawn`, `path_program_is_shown_verbatim_at_approval`, `ungranted_external_agent_is_not_spawned` (matches `headless_never_loads_ungranted_plugin`, T33.6).
Status: done 2026-09-26
Result: a granted plugin's `[[external_agents]]` entry resolves to a sandbox-wrapped command.
- `cox_plugin::external_agent::package_program(dir, command)` is T33.19's `plugin_program`, moved so there is one copy. It accepts a PATH program as written or a regular file inside the package, and refuses symlinks, `..` and absolute paths. `plugin_mcp` calls it.
- `ExternalAgentCommand::resolve(plugin, dir, decl, wrap)` is the only constructor, so an unwrapped command cannot be built. A failed or empty wrap refuses the entry (`ExternalAgentError::Sandbox`).
  - Accessors: `plugin()`, `name()`, `mode()`, `key_env()`, `argv()`.
  - `command()` returns a fresh, already-wrapped `std::process::Command`. The caller sets env, stdio and cwd, and supplies the key from `key_env`.
- `load_plugins` keeps its single walk and fills `Plugins.external_agents` through `plugin_agents`, which wraps with `sandboxed_argv`. When the wrap is impossible the entry is skipped with a notice (wrap-or-refuse). Ungranted plugins resolve nothing.
- The approval line keeps T35.1's format, `agent:<name> <command args> key=<key_env>`. It shows a PATH program verbatim, and the CLI prompt and TUI dialog both list it.
Deviations: no per-entry `sandbox = false` opt-out for external agents; EA§2 and T35.8 mention one, and it would need its own card. `Plugins.external_agents` carries `#[allow(dead_code)]` until T35.13 reads it.
Check:
- `external_agent_command_is_wrapped_by_sandbox_before_spawn` (the agent really runs under Seatbelt: a write in the workspace lands, one under `$HOME` is denied), `path_program_is_shown_verbatim_at_approval` and `ungranted_external_agent_is_not_spawned` pass. `headless_never_loads_ungranted_plugin` and `plugin_stdio_server_runs_under_sandbox` still pass.
- In the worktree: nextest 1116 passed, 3 skipped; fmt and both clippy runs clean.
- On main after landing (with T35.2, T33.32): nextest 1135 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.32 `cox plugin remove`

Depends: T33.31 · Size: ~150 · Files: `crates/cox/src/plugin_cmd.rs`, `crates/cox-plugin/src/install.rs`
Goal: PL§1c: confirm (`--yes` skips), disable, delete the plugin's own directory (resolved and checked to be under `~/.cox/plugins/`, no symlinks followed out), delete every grant row, and delete kv unless `--keep-data`. Report config references and edit none of them. A project plugin keeps its files.
Check: e2e: remove → files, grants, kv and contributions are gone and a sibling plugin is untouched; `--keep-data` keeps kv; `remove_refuses_path_outside_plugins_dir` (a symlinked version dir).
Status: done 2026-09-26
Result: `cox plugin remove <id> [--keep-data] [--yes]` (PL§1c).
- `cox_plugin::install::remove(cox_home, id)` canonicalizes `<home>/plugins/<id>` and refuses, deleting nothing, if the result is not under `<home>/plugins/`. Otherwise it runs `remove_dir_all`, which never follows an inner symlink. A missing directory is a no-op.
- `plugin_cmd::remove` finds the plugin by directory (user or `<git root>/.cox/plugins/<id>`), so a plugin with a broken manifest can still be removed. The steps:
  1. Confirm through `confirm()` unless `--yes`. Headless without `--yes` deletes nothing.
  2. `grants_delete(id)` for every scope and digest.
  3. Delete a user plugin's files. A project plugin keeps its files and the path is printed.
  4. `kv_delete_all(id)` unless `--keep-data`.
  5. List config references and edit none: `[plugins.<id>]`, `[plugins.decide]` entries, `[providers.<id>-*]` and any `tiers.*.provider` that names one, `[mcp.servers.<id>-*]`, and `keybindings.toml` `plugin.<id>.*`.
Deviations: PL§1c's "disable" and "delete every grant row" collapse into one `grants_delete` up front. That leaves every digest ungranted without needing a readable current digest. The store already had `grants_delete` and `kv_delete_all`. No `--project` flag.
Not done: the TUI `/plugin remove` and stopping a running instance's tools are T33.33.
Check:
- The e2e tests pass: remove clears files, grants and kv and leaves a sibling plugin untouched; `--keep-data` keeps kv; a decline deletes nothing; an unknown id is a no-op; a project plugin keeps its files.
- `remove_refuses_path_outside_plugins_dir` (a symlinked `<id>` directory, canary survives), two more `install.rs` tests and six `config_refs`/`keybinding_refs` tests pass.
- In the worktree: nextest 1122 passed, 3 skipped; fmt and both clippy runs clean.
- On main after landing (with T35.2, T33.32): nextest 1135 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.15 `cox_model_call` and `Job::Plugin`

Depends: T33.9 · Size: ~170 · Files: `crates/cox-protocol/src/types.rs` (`Job::Plugin`), `crates/cox-core/src/router.rs`, `crates/cox-plugin/src/hostfn.rs`
Goal: a plugin's model call goes through the router at or below its granted tier (never `think`), passes the budget gate and writes one `usage` row with job `plugin:<id>`.
Check: `plugin_model_call_writes_usage_row` (Scripted provider), `plugin_model_call_blocked_by_budget`, `plugin_cannot_reach_think_tier`; invariant 8 green.
Status: done 2026-09-26
Result: a plugin's `cox_model_call` goes through the router, the budget gate and the ledger.
- `Job::Plugin(String)` serializes as `"plugin:<id>"`; every other job stays a bare string, through hand-written serde and schema impls. `Job` is no longer `Copy`, so a few clones follow.
- `cox_protocol::traits::ModelCaller { async fn call(&self, id, tier, request) -> Result<Vec<ProviderEvent>, CoreError> }` is the seam. `cox-core/src/plugin_model.rs` implements it for `Session`:
  1. Refuse `Tier::Think`.
  2. `Router::pick`; `Job::Plugin` uses the tier the caller passes.
  3. Budget gate: `Stop` becomes `CoreError::Budget`.
  4. Call the provider.
  5. Write one `UsageRow` with `job = plugin:<id>`.
- `HostEnv::with_model_caller(caller, runtime_handle)` feeds the `cox_model_call` arm. It checks the grant (`model:code` covers `model:cheap`) and `outside_render()`, clamps the tier to min(requested, granted), blocks the worker thread on the call through the handle, and maps `Budget` to `AbiError::Budget` and everything else to `Failed`.
- `docs/protocol.jsonschema` regenerated by its drift test.
Deviations: `Job` losing `Copy` touched `session.rs`, `subagent.rs` and two cox-core test files mechanically. `config.rs` needs a `Job::Plugin` arm. cox-plugin gains `tokio`, plus `async-trait` for dev only; both were already workspace dependencies.
Not done: nothing installs a real `ModelCaller` yet; that is T33.44.
Check:
- `plugin_model_call_writes_usage_row` (Scripted provider), `plugin_model_call_blocked_by_budget` and `plugin_cannot_reach_think_tier` pass, along with four `hostfn` tests and `job_tags_are_plain_strings_and_plugin_round_trips_by_id`. Invariant 8, `turn_every_request_has_a_usage_row`, is green.
- In the worktree: nextest 1086 passed, 3 skipped; fmt and both clippy runs clean.
- On main after landing: nextest 1143 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.44 The session keeps one live instance per granted plugin — blocker

Depends: T33.9, T33.10, T33.11, T33.16 · Size: ~190 · Files: `crates/cox/src/session.rs`, `crates/cox-plugin/src/live.rs` (new)
Goal: split from T33.9, T33.10, T33.11 and T33.16. Each of them built its piece against a caller-supplied `PluginHost`, and the session still loads plugins through a grant-nothing environment and drops them. At session open, for each `Granted` plugin, `crates/cox`:
- builds `HostEnv::new(id).with_grant(grant::capability_list(..), store).with_context(ctx)` with an `Arc<dyn PluginStore>`;
- calls `PluginHost::load_with`, then `cox_init` with `init_input(..)`;
- keeps one `Arc<PluginHost>` that hooks, the event tap and later tools all share;
- installs `PresenceHook(HookChain::new(shell, plugins))`, with `shell` set to `None` under `--no-hooks`;
- sets the T33.10 event tap, which feeds `Context::fold`;
- drains `take_notices()` into `Event::Notice`;
- passes each granted plugin's `[[models]]` to `Catalog::load` and shows `catalog.warnings()` as notices (the part T33.16 left).

A plugin whose load or `cox_init` fails is warned about and skipped, never fatal.
Check: `granted_plugin_runs_cox_init_once_per_session`, `plugin_notify_reaches_the_transcript`, `hooks_and_event_tap_share_one_plugin_instance`, `plugin_init_failure_is_skipped_with_a_warning`, `granted_plugin_models_join_the_catalog`.
Status: done 2026-09-26
Result: `cox_plugin::live` (`crates/cox-plugin/src/live.rs`) is the per-session plugin bundle, and `crates/cox/src/session.rs` wires it in.
- **`LivePlugins`** holds one shared `Arc<Context>`, a late-bound model caller and one `Live` per granted plugin: the manifest, one `Arc<PluginHost>`, its `Arc<HostEnv>` (granted capabilities, kv store, context), the granted lines and `InitOut`.
  - `load` compiles each granted plugin once, in the same walk `load_plugins` already did.
  - `start(&PluginsConfig, SessionId, cwd)` folds `SessionStarted` into the context and runs each `cox_init` once. A plugin whose init fails is dropped with "plugin X failed to start: …".
  - `hooks()`, `hosts()`, `take_notices()` (each notice prefixed "plugin <id>: "), `Live::granted_status()` (status slots minus those whose `ui.*` capability is not granted), `into_tap(redraw, notices)`, `bind_model_caller`.
- **One instance per plugin.** The `HookChain` (`PresenceHook(HookChain::new(shell, plugin_hooks))`; `shell` is `None` under `--no-hooks`, plugin hooks still run), the event tap and the T33.23 UI server (`plugin_ui::serve(live.hosts(), …)`) all share the same `Arc<PluginHost>`.
- **Notices.** Init warnings and notices `cox_init` queued are emitted by `open` as `Event::Notice` after the load warnings. Later ones are drained by the tap after every event except a `Notice` (so a plugin answering notices with notices cannot loop) and forwarded by a tokio task to `session.notice`.
- **Models.** Granted plugins' `[[models]]` join `Catalog::load` through `provider_for_served`/`backend_for_with`; `catalog.warnings()` join the open notices.
- **Model caller.** Each `HostEnv` gets `with_model_caller(LateCaller, Handle::try_current())`; `LateCaller` holds a `Weak<dyn ModelCaller>`, and the strong `Arc` of the session belongs to the notice-forwarding task, so the plugin graph holds no strong reference back to the session.
- **UI.** After `cox_init`, one `PluginUiMsg::Declare { plugin, slots: granted_status() }` per plugin with granted slots; the tap's redraw is `plugin_ui::redraw(feed)` on the TUI and a no-op elsewhere.
Deviations:
- `cox_init` runs after `Session::new`, because `SessionInfo` needs the session id. The declarative parts (`[[provider]]`, `[[models]]`, `[[mcp]]`, `[[external_agents]]`) join at load time, so a plugin whose init fails keeps them for that session; only its hooks, events and instance are dropped.
- Files beyond the card's two: `hostfn.rs` (test module and `MemKv` are `pub(crate)`), `lib.rs`, and `acp_cmd.rs`, `run.rs`, `plain.rs`, `plugin_ui.rs` for the new `open` parameter, the store type and the T33.23 seam.
Not done (left for later cards):
- `doctor.rs` still passes `&[]` to `Catalog::load`.
- ACP (`plugin_notices`) still compiles plugins and drops them; it has no live plugins.
- No session-level test drives a real `cox_model_call`; the weak binding has a unit test only.
- `cox_shutdown` is not called at session end.
- T33.12 needs either late tool registration in cox-core or `start` before `Session::new`; the hook-in point is `start_plugins`, between `live.start` and `live.into_tap`.
Check:
- In `session.rs`, with real WAT fixtures installed and granted in a temp `COX_HOME`: `granted_plugin_runs_cox_init_once_per_session`, `plugin_notify_reaches_the_transcript`, `hooks_and_event_tap_share_one_plugin_instance` (a WAT global counts hook calls; the `cox_on_event` notice reports it, so two instances would report 0), `plugin_init_failure_is_skipped_with_a_warning`, `granted_plugin_models_join_the_catalog`.
- In `live.rs`: `hooks_and_event_tap_share_one_plugin_instance`, `failed_init_drops_the_plugin_with_a_warning`, `only_granted_status_slots_are_declared`, `model_caller_is_held_weakly`.
- Real binary against a scratch `COX_HOME`: `cox plugin install --yes`, then `cox run -p hi --output-format stream-json` printed `{"type":"notice","level":"info","text":"plugin hello: hello from cox_init"}`.
- In the worktree: nextest 1152 passed, 3 skipped; fmt, clippy (`-D warnings`, all targets) and the slim build clean.
- On main after landing (with T33.44 and T35.8 together): nextest 1156 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T35.8 `cox doctor` reporting

Depends: T35.2 · Size: ~120 · Files: `crates/cox/src/doctor.rs`, `crates/cox-plugin/src/external_agent.rs`
Goal: a doctor row per granted `[[external_agents]]` entry: CLI binary found on `PATH` (and its `--version`, best-effort), `key_env` set or missing, sandboxed or opted out (T33.42's per-server opt-out shape). A missing CLI or key is the fail-open warning EA§7 specifies, with the preset left out of `agent`'s names, not a hard failure.
Check: `doctor_reports_missing_cli_as_a_warning_not_a_failure`, `doctor_reports_key_env_set_and_cli_version`.
Status: done 2026-09-26
Result: `cox doctor` has one row per granted `[[external_agents]]` entry (`crates/cox/src/doctor.rs`, feature `plugins`; the slim build adds none).
- `check_external_agents` walks the same `discover::discover` + `grant::scope`/`grant::check` path `cox plugin list` uses; there is no second discovery.
- Per entry, `check_external_agent_with` (injectable, like `check_api_keys_with`):
  - resolves `command` with `cox_plugin::external_agent::package_program`;
  - wraps `["--version"]` (never the mode args, so the real driver never starts) through `session::sandboxed_argv` (now `pub(crate)`), the same wrap a driver gets;
  - runs it with a 3 s timeout and keeps the first stdout line best-effort;
  - checks `key_env` for presence only, never printing its value.
- Row text:
  - success: "sandboxed; found, <version>; key_env <VAR> set|not set";
  - sandbox refusal: "refused: cannot run under the sandbox on this host (<reason>); …";
  - missing CLI: "<cmd> not found on PATH; …".
- A missing CLI, a missing key, a sandbox refusal and a bad in-package command are all `warn`, never `fail` (EA§7 fail-open).
- `doctor::run` takes `cwd` to find the project plugin root, the same way `session::mcp_servers` does.
Deviations:
- The row reports "sandboxed" or "refused" rather than EA§7's "sandboxed or opted out": external agents are wrap-or-refuse (T35.2) and have no `sandbox = false` opt-out.
- `crates/cox-plugin/src/external_agent.rs` is untouched; everything fit in `doctor.rs`. `main.rs` passes `cwd`.
Check:
- `doctor_reports_missing_cli_as_a_warning_not_a_failure`
- `doctor_reports_key_env_set_and_cli_version`, which also covers the same CLI with the key missing
- `doctor_reports_sandbox_refusal_as_a_warning_not_a_failure`
- `doctor_reports_a_bad_in_package_command_as_a_warning`
- In the worktree: nextest 1139 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (with T33.44 and T35.8 together): nextest 1156 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.25 Plugin commands and keys

Depends: T33.23 · Size: ~180 · Files: `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/keymap.rs`, `crates/cox-tui/src/commands.rs`
Goal: `/<id>:<name>` in the palette after the built-ins, and a `CommandOut` limited to PL§4's closed set. Keys work only as `<leader> <key>`; `plugin.leader` can be rebound in `keybindings.toml`. Built-ins and user bindings win. A clash between plugins goes to the lower id and is reported by `Keymap::conflicts()`.
Check: `builtin_command_wins_over_plugin`, `plugin_command_prompt_submits_user_turn`, `plugin_key_only_under_leader`, `plugin_key_conflict_is_reported`.
Status: done 2026-09-26
Result: plugins add `/` commands and leader keys to the TUI.

**Commands.**
- A plugin's commands appear in the palette as `/<id>:<name>`, after the built-ins. `compose()` dispatches `file_command → plugin_command → commands::parse`, so a built-in always wins, and no built-in name contains `:`.
- `Action::PluginCommand` becomes `Cmd::Plugin(PluginRequest::Command{..})`.
- `crates/cox/src/plugin_ui.rs` calls `cox_command` (and `cox_key`) on `Lane::Control` with `COMMAND_DEADLINE` = 5 s. A timeout, an error, a missing export or an unknown plugin becomes `out: None` (fail-open, like render).
- `CommandOut` is PL§4's closed set:
  - `Prompt` submits a user turn;
  - `Compact` becomes `Cmd::Compact{focus}`;
  - `Notice` becomes a sanitized transcript notice;
  - `TogglePanel` and `OpenOverlay` are accepted but have no effect until panels and overlays exist (T33.24).

**Keys.**
- `plugin.leader` (default Ctrl+K, idle) arms the next key, which is always consumed. `Keymap::resolve_plugin_key` resolves it.
- `declare_plugin_keys` sorts by plugin id, so on a clash the lower id wins. `Keymap::conflicts()` reports the clash as "<leader> K: plugin a and plugin b".

**Declare.** `PluginUiMsg::Declare` carries `commands` and `keys` as well as `slots`.
- `Live::granted_commands()` and `granted_keys()` filter by `ui.commands` and `ui.keys`.
- `serve_plugin_ui` declares a plugin that has any of the three, so a plugin with commands only and no status slot is still declared.
- Every string from a plugin passes `cox_sanitize::sanitize` inside `state.rs`.

**Docs.** `plugin.leader` is in `docs/config.md`, `docs/getting-started.md` and `KEYBINDINGS_DOCS` in `cox-protocol/src/config.rs`, the literal the config-docs drift test compares. The help-overlay snapshots gain the new row.

Deviations: 12 files instead of the card's three.
- `plugin_ui.rs` has the T33.23 seam.
- `status.rs` needed the match to stay exhaustive.
- `config.rs` and the two docs pages are required by the doc-drift tests.
- `live.rs` and `session.rs` wire the Declare fields.
- The two snapshots change on purpose.

Check:
- `builtin_command_wins_over_plugin`
- `plugin_command_prompt_submits_user_turn`
- `plugin_key_only_under_leader`
- `plugin_key_conflict_is_reported`
- `declare_plugin_keys_replaces_the_previous_set`
- `command_answer_carries_command_out`
- `key_request_calls_cox_key_not_cox_command`
- `commands_and_keys_are_dropped_without_their_capability`

In the worktree after the rebase onto T33.44: nextest 1164 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (with T35.6): nextest 1164 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T35.6 The Cursor plugin package

Depends: T35.1, T35.5 · Size: ~120 · Files: `plugins/cursor/plugin.toml`, `plugins/cursor/src/lib.rs`, `plugins/Cargo.toml` (member)
Goal: the first real user of `[[external_agents]]`, the same role Jev played for the ABI provider form (T33.40): `plugin.toml` declares one entry (`name = "cursor"`, `command = "agent"` resolved on `PATH`, `mode = "acp"` by default with `stream-json` as the manifest's documented alternative, `key_env = "CURSOR_API_KEY"`); the guest exports only `cox_init` (no other capability), since spawning and driving the process is entirely the host's job (EA§2) — the smallest possible plugin, unlike Jev's `api = "plugin"` provider guest.
Check: `cox plugin list` (e2e, scratch `COX_HOME`) reports the `cursor` plugin's `external_agents` capability; `cursor_plugin_toml_matches_the_manifest_schema`.
Status: done 2026-09-26
Result: `plugins/cursor` (`cox-plugin-cursor`, a member of the `plugins/` workspace) is the smallest possible plugin.
- `plugin.toml` declares one `[[external_agents]]` entry: `name = "cursor"`, `command = "agent"`, `mode = "acp"` (with `stream-json` documented as the alternative) and `key_env = "CURSOR_API_KEY"`. It has no `[capabilities]`, `[[provider]]` or `[[mcp]]`.
- The guest exports only `cox_init` (`register!(init => init)`). Spawning and driving the CLI is the host's job (EA§2).
- No new dependency. `figment` is a dev-dependency, shared with jev's manifest test.
Check:
- `cursor_plugin_toml_matches_the_manifest_schema` passes. It uses the same Figment + `PluginManifest` + `validate()` path as jev's `manifest_validates`.
- The wasm32 build of `cox-plugin-cursor` is clean, and so are fmt and clippy for the `plugins/` workspace (host and wasm32 targets).
- e2e against a scratch `COX_HOME`:
  - Before the grant, `cox plugin list` (text and `--json`) shows the capability `agent:cursor agent key=CURSOR_API_KEY`.
  - `cox plugin install --yes` prints the same line in its prompt and reports "grant granted, loaded".
  - After the grant, the `declared` summary covers only `[capabilities]`. This predates the card: the same gap noted under T33.7 for jev's `[[provider]]`.
- In the worktree: nextest 1143 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (with T33.25): nextest 1164 passed, 3 skipped; fmt, clippy, the slim build, the wasm32 build and the cursor test clean. T35.13 later sets `args = ["acp"]` in `plugin.toml`.

#### T33.20 Decision points: the `Advisor` trait and `route`

Depends: T33.15 · Size: ~190 · Files: `crates/cox-protocol/src/traits.rs` (+ `Event::Advised` in `types.rs`), `crates/cox-core/src/router.rs`, `crates/cox-plugin/src/advisor.rs`
Goal: `Advisor` set on `Session` like the hook, with `[plugins.decide]` naming one plugin per point plus `min_confidence` and a latency budget. `route` offers only tiers at or below the static pick and never `think`. Every answer is an `Event::Advised { applied }`. On silence, lateness or low confidence the static pick is used.
Plan note (amended 2026-09-26, R§4.3.6 J20): a turn a decision plugin routes down must strip thinking blocks in its own `Request` only — never rewrite `inner.history` in place, and never emit `ModelSwitched` for a same-turn tier offer. That stripping, and the cache-aware filter that decides whether `cheap` is even offered, are implemented by T33.40.8; this card only wires the `Advisor` trait and the `route` offer through it.
Check: `route_advice_never_routes_up`, `late_advice_falls_back_to_static_pick`, `advised_event_in_rollout_replays_identically`; `docs/protocol.jsonschema` regenerated.
Status: done 2026-09-26
Result: the first decision point, `route`, is wired end to end.
- **`Advisor` trait** (`cox_protocol::traits`): `id()` and `async advise(Question, Duration) -> Option<Advice>`. `None` means the static pick.
- **Config.** `[plugins.decide]` is `DecideConfig { route, min_confidence = 0.6, route_ms = 300 }`, flat with one field per point and `deny_unknown_fields`. `docs/config.jsonschema`, `docs/config.md` and `default.toml` are regenerated.
- **Tier order.** `Tier` derives `Ord` (Cheap < Code < Think). This is now the only place tiers are compared: the model-call grant clamp in `hostfn.rs` became `requested.min(granted)`, and its private `tier_rank` is gone.
- **Router.** `router::route_offer(static)` offers only `cheap`/`code` at or below the static pick, never `think`; nothing is asked when the static pick is `cheap`. `router::apply_route` takes the advice only if its first choice was offered and its confidence is at least `min_confidence`; missing confidence never counts.
- **Session** (`Session::route_turn` in `cox-core/src/advise.rs`):
  - It runs after the think gate in `run_turn_inner`. The question carries the static tier and the prompt, scrubbed by `cox_sanitize::redact::scrub` and cut to 8000 chars.
  - The core enforces `route_ms` with `tokio::time::timeout` on top of the plugin deadline.
  - A followed tier lives in `Inner::routed` for every main-job call of that turn and is cleared however the turn ends. History is never rewritten and no `ModelSwitched` is emitted.
  - Subagents get no advisors.
- **Event.** Every answer emits `Event::Advised { point, plugin, advice, applied }` before `TurnStarted`; `docs/protocol.jsonschema` is regenerated. Silence or lateness emits nothing and keeps the static pick, and so does an advised tier that cannot be routed (with `applied = false`).
- **Plugin side.** `cox_plugin::PluginAdvisor` calls `cox_decide` only for points granted as `decide:<point>` and treats any error as silence. `LivePlugins::advisors()` is wired at `start_plugins` via `session.set_advisors`. The WAT test helpers `answering` and `spinning` are shared in `host::tests`.
Deviations:
- About 320 lines excluding tests against ~190, and more than three files: session wiring, the config table, `advise.rs`, `LivePlugins::advisors`, a no-op TUI arm, the `tier_rank` dedup and the test-helper move.
Not done (other cards):
- T33.40.8 owns stripping thinking blocks from a routed-down turn's own `Request`, the cache-aware `cheap` filter, and skipping `Job::Plan`, `/think` and turns after `/model`. The seam is `route_offer` plus a comment in `route_for`. Until then, a routed-down turn can carry thinking blocks, and only when `[plugins.decide] route` is set.
- `[plugins.decide]` is not on the project-config guard list. A project can name an advisor, which still needs a `decide:route` grant and can only route down. This belongs with the `[plugins.<id>]` guard gap noted under T33.9.
- A `route` naming a plugin that is not live is silently off, with no warning.
- `cox plugin remove` does not report `[plugins.decide]` references (PL§1c).
- The two-phase `DecideOut` is T33.40.1.
Check:
- `route_advice_never_routes_up`, `late_advice_falls_back_to_static_pick`, `advised_event_in_rollout_replays_identically`
- `low_confidence_advice_is_recorded_but_not_applied`, three `plugin_advisor_*` tests, and an `advised` case in `event_json_roundtrip`
- The real binary with a scratch `COX_HOME`: `cox config get plugins.decide` shows the table, and an unknown point is rejected.
- In the worktree: nextest 1164 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (with T33.20, T33.12 and T35.13 together): nextest 1183 passed, 3 skipped; fmt, clippy, the slim build and the cox-plugin-cursor tests clean.

#### T33.12 Tools from plugins

Depends: T33.9, T33.44 · Size: ~170 · Files: `crates/cox-plugin/src/tool.rs`, `crates/cox/src/session.rs`
Goal: `WasmTool` implements `Tool` as `wasm__<id>__<tool>`. It is always deferred, its specs are frozen at `cox_init`, and tools are sorted by (id, tool) and appended after MCP. `Concurrency::Exclusive` per plugin. `cox_output` and `cox_cancelled` inside `cox_tool_call`.
Check: `plugin_tool_specs_frozen_within_session` (new invariant 15); `prefix_bytes_identical_between_turns` with a plugin tool discovered mid-session; `plugin_tool_output_is_archived_before_truncation`.
Status: done 2026-09-26
Result: plugins contribute tools.

**WasmTool** (`crates/cox-plugin/src/tool.rs`):
- `WasmTool` implements `Tool` as `wasm__<id>__<tool>` (`tool::PREFIX`, `tool::qualified`). Plugin ids have no `_`, so the second `__` always ends the id.
- Its spec is frozen from the one `cox_init`. It is always deferred and `Concurrency::Exclusive`, whatever the plugin declares.
- `risk` is the declared value, or `Write` when none is declared (mirrors MCP's `readOnlyHint`). `subject` is the qualified name.
- Only granted `tools:<name>` entries are kept; an ungranted or duplicate entry is dropped with a warning.

**Calls:**
- `call` sends `ToolCallIn` to `cox_tool_call` off the async runtime, under the plugin's `call_ms` deadline. A lock shared by the plugin's tools allows one call per plugin at a time.
- Cancel returns `ToolError::Cancelled` at once; the guest stops cooperatively through `cox_cancelled`. A timeout becomes `ToolError::Timeout`; any other error becomes an `is_error` output.
- Only `text` and `is_error` are kept. `structured` is dropped because the core reads `structured.discovered` as a `tool_search` result, and a plugin must not be able to forge it.
- `cox_output` and `cox_cancelled` (`hostfn.rs`) work only inside `cox_tool_call` and answer `NotInThisContext` elsewhere.

**Session wiring:**
- `open()` picks the session id (the resumed one, or `SessionId::new()`) and runs `cox_init` in `plugin_tools()` before `Session::new_with_id`/`resume`. The tool list is therefore complete at construction, and the cache prefix is byte-stable by construction.
- `LivePlugins::tools()` sorts by (id, tool). The tools are appended after MCP.
- `LivePlugins::start` skips plugins already started, so `start_plugins` is otherwise unchanged.

Deviations:
- Design (b), init before `Session::new`, instead of late registration. Late registration would need a shared mutable tool list across session clones, `AgentTool`'s parent handle and the `tool_search` index.
- The `tool_search` index is now built after MCP and plugin tools are added. Before, deferred MCP tools (and `mcp.deferred` defaults to true) could never be found by `tool_search`; that bug is fixed here.
- `cox_model_call` from inside `cox_init` answers Denied ("no session is serving model calls"), because init now runs before the session exists.
- About 230 lines of code against ~170, across 7 source files. `tokio-util` is a dev-dependency of `cox-plugin`; it is already a workspace dependency.

Not done:
- The optional `cox_tool_subject` and `cox_tool_risk` exports are not called, because `Tool::subject` and `Tool::risk` are synchronous.
- No real-binary run with an installed plugin tool; the session tests go through the real discover/grant/load path.

For later cards:
- T33.13 adds `cox_invoke_tool` in `HostEnv::dispatch` behind `invoke:<name>` grants, through PreToolUse → Engine → sandbox → archive, and refuses a nested call into the calling plugin (it would deadlock).
- T33.33 catches a removed plugin's `wasm__<id>__*` name where the tool lookup in `turn.rs` finds nothing, and answers `ToolError::Denied`.

Check:
- `plugin_tool_specs_frozen_within_session`
- `prefix_bytes_identical_between_turns_with_plugin_tool_discovered`: records a real session. The system and tool prefix changes once at discovery, then stays byte-identical over three turns.
- `plugin_tool_output_is_archived_before_truncation`: all 20 000 bytes are archived, and the model's text carries `expand #<id>`.
- `tool_call_streams_output_and_stops_on_cox_cancelled`
- `output_and_cancelled_work_only_inside_tool_call`
- In the worktree: nextest 1161 passed, 3 skipped; fmt, clippy and the slim build clean. The real binary with a scripted provider and a scratch `COX_HOME` exits 0.
- On main after landing (with T33.20, T33.12 and T35.13 together): nextest 1183 passed, 3 skipped; fmt, clippy, the slim build and the cox-plugin-cursor tests clean.

#### T35.13 Host drivers: install granted external agents in the session

Depends: T35.2, T35.5, T35.12 · Size: ~180 · Files: `crates/cox/src/session.rs`, `crates/cox-plugin/src/external_agent.rs`, `crates/cox-acp/src/client.rs`
Goal: split from T35.5, whose core side landed as the `ExternalAgent` trait and `Session::set_external_agents`. For each granted `[[external_agents]]` entry, `crates/cox` builds one driver behind `ExternalAgent` over T35.2's sandboxed `Command`:
- `mode = "stream-json"` runs the CLI per turn and feeds stdout lines to `StreamJsonMapper::new(turn, name, cox_sanitize::sanitize)` (T35.4, T35.12).
- `mode = "acp"` wraps T35.3's `connect` with a `ClientHost` built from the session's roots, sandbox, engine and grants.
- The answer arrives as `ItemStarted { AssistantMessage }`. The driver never sends `Usage`, returns `Some(usage)` only when the CLI reported tokens, and honours `cancel`.
- One driver per entry is kept for the session. An entry whose CLI or `key_env` is missing is left out with one Warn notice (EA§7).
- `set_external_agents` is called next to `set_agent_defs`. `docs/design/external-agents.md` says that the agent's own tool calls are not judged per call by the Engine or PreToolUse hooks; the process sandbox is the guard (EA§2).
Check: `stream_json_driver_answers_a_child_task` (fake CLI script under the sandbox), `missing_cli_leaves_the_preset_out_with_one_warning`, `cancel_kills_the_external_agent_process`.
Status: done 2026-09-26
Result: `crates/cox/src/external_agents.rs` (feature `plugins`) holds the host drivers behind `cox_protocol::traits::ExternalAgent`.

- **`drivers(agents, config, cwd, writable, PATH, key_fn)`** returns the drivers plus one warning per entry it leaves out.
  - An entry is left out when its CLI is not on PATH (`external_agent::missing_on_path`, the single PATH lookup that doctor also uses) or its key does not resolve.
  - The key comes from `key_fn` = `resolve_key(key_env, <entry name>)` in the binary.
  - `session.rs` calls `set_external_agents` right after `set_agent_defs`, and the warnings join the plugin warnings.

- **What every child gets:**
  - a cleared env plus `CHILD_ENV_ALLOWLIST` (`PATH`, `HOME`, `LANG`, `LC_*`, `TERM`, `TMPDIR`, `USER`, `SHELL`) plus `key_env`;
  - the session cwd;
  - the sandbox wrap (`session::sandboxed_argv`, whose policy comes from the single `session::sandbox_policy`, which `mcp_cmd.rs` now reuses too);
  - its own process group, killed however the turn ends (`cox_tools::bash::kill_group`, reusing bash's `killpg`).

- **Arguments:** the manifest `args` are the mode's own invocation. The host adds only the prompt, as the last argument, for stream-json.

- **StreamJson driver:** one CLI run per turn. Each stdout line goes through `StreamJsonMapper` with `sanitize`, and the mapper's `TurnDone` ends the turn. An exit without a `result` line is `CoreError::ExternalAgent`, carrying the sanitized end of stderr.

- **Acp driver:** `cox_acp::connect`, then `initialize` → `session/new` → one `session/prompt` per turn.
  - `agent_message_chunk` text becomes one `AssistantMessage`. `ClientHost.updates` forwards `session/update`.
  - `fs/*` is confined to the writable roots.
  - A permission request is decided by an `Engine` compiled from the same `[permissions]`, mode and approval policy. `Ask` is refused with a reason naming a permissions rule.

- **Usage and cancel:** neither driver reports `Usage` (they return `Ok(None)`), and both honour cancel.

- **Docs:** `docs/design/external-agents.md` §2 now says the agent's own tool calls are not judged per call by the Engine or PreToolUse hooks, because the process sandbox is the guard. §2 and §4 describe the drivers.

- **Cursor package:** `plugins/cursor/plugin.toml` now declares `args = ["acp"]`, with the stream-json alternative documented; its schema test asserts it.

- **doctor:** it runs `missing_on_path` before the `--version` probe, so a CLI missing under the sandbox wrap reports "not found on PATH" instead of "no version output".

Deviations:
- ACP approvals that would need the user are refused rather than shown. `ExternalAgent::turn` cannot reach the session's approval prompt, and the session's `Engine` and grants are crate-private. This needs a core change and its own card.
- Every turn starts a fresh CLI process, for ACP too, so the agent's earlier context does not carry over.
- There is no usage: ACP's `PromptResponse.usage` is behind an unstable feature, and stream-json's result line has no token counts.
- The ACP driver has no end-to-end test; that is T35.7.
- `async-trait` and `tokio-util` (with `compat`) move from `crates/cox` dev-dependencies to normal ones, and `agent-client-protocol` is added (already a workspace dependency). There are no new crates in `Cargo.lock`, and the `toolchain.md` rows are updated.

Check:
- `stream_json_driver_answers_a_child_task`: a real parent `Session` with a scripted provider, where `agent(preset:"cursor")` runs a fake CLI under the real Seatbelt wrap.
- `missing_cli_leaves_the_preset_out_with_one_warning`
- `cancel_kills_the_external_agent_process`
- `acp_client_forwards_session_updates_to_the_driver`
- The three card tests were rerun 15 times with no failures.
- The real binary, run against a scratch `COX_HOME` (`run -p` with the scripted provider, then `doctor`), exits 0.
- In the worktree after the rebase: nextest 1170 passed, 3 skipped; fmt, clippy and the slim build clean; `cox-plugin-cursor` tests pass.
- On main after landing (with T33.20, T33.12 and T35.13 together): nextest 1183 passed, 3 skipped; fmt, clippy, the slim build and the cox-plugin-cursor tests clean.

#### T33.24 TUI panel and overlay

Depends: T33.23 · Size: ~170 · Files: `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/view.rs`, `crates/cox-tui/src/modal.rs`
Goal: a bottom `panel` (≤ 8 rows, above the composer, toggled by the plugin's command or key) and `Modal::Plugin { id }` as a full-screen overlay that Esc closes. Sizes are sent through `Cmd::Plugin`.
Check: insta snapshots of the panel open and closed and of the overlay; `esc_closes_plugin_overlay`.
Status: done 2026-09-26
Result: plugins get a bottom panel and a full-screen overlay in the TUI.
- **Panel.** `State.plugin_panel_open: Option<String>` controls it, and `view.rs` draws it as an 8-row band above the composer, after the todo band.
- **Overlay.** `Modal::Plugin { id }` has zero modal height and is drawn over the transcript like `Diff`/`Transcript`. Esc closes it; other keys keep it open. When nothing has rendered yet, `modal::plugin_overlay_placeholder` shows instead (fail open).
- **Commands.** T33.25's `CommandOut::TogglePanel` toggles the panel and `OpenOverlay` opens the overlay. Both re-ask `status::render_requests`, because opening counts as a slot becoming visible.
- **Slots** (`status.rs`, the one slot-render machinery):
  - `Declare` now registers `panel` and `overlay` slots and reuses `PluginSegment` and its 3-miss stop.
  - `slot_visible` renders panel and overlay only once they are opened; status segments are always visible.
  - `area_for` sizes each request: status stays 24×1, the panel is `(term cols, 8)` and the overlay is the full terminal.
- **Terminal size.** `State.term` is fed by `Msg::Resize` and seeded in `app.rs` from the size the first draw uses.
- Plugin output goes through the existing widget-tree path (`plugin_ui::render`) and its sanitize boundary.
Deviations:
- `status.rs` and `app.rs` are touched beyond the card's three files. `status.rs` already owns slot rendering, so extending it avoids a second copy; `app.rs` gets the one-line size seed.
- About 210 lines of code against ~170.
Check:
- `esc_closes_plugin_overlay`
- `plugin_command_toggles_panel_and_opens_overlay`
- insta snapshots: `plugin_panel_open_and_closed`, `plugin_overlay_snapshot`
- In the worktree: nextest 1168 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing: nextest 1187 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.26 Custom rendering of tool results and messages

Depends: T33.23 · Size: ~160 · Files: `crates/cox-tui/src/cells.rs`, `crates/cox-tui/src/state.rs`
Goal: `cox_render_item` for `tool:<name>` and `item:assistant_message`, asked once when the cell completes and cached in the cell. `None`, a timeout or an error uses the built-in rendering. A target outside the plugin's own tools needs its explicit grant.
Check: insta snapshots (plugin-rendered, and fallback after a timeout); `renderer_output_never_reaches_rollout_or_model` (the rollout and the next `Request` are byte-identical with and without the renderer).
Status: done 2026-09-26
Result: a plugin can draw a finished tool result or assistant message in the TUI with `cox_render_item`.

- **When it asks** (`crates/cox-tui/src/item_render.rs`): a tool cell asks its renderer on `ToolCallDone`, an assistant cell on `ItemDone`. Each cell asks once and caches the answer in the cell (`render: ItemRender` on `Cell::Tool` and `Cell::Assistant`).
  - While a render is pending, `Cell::done()` is false, so the cell stays in the viewport.
  - `None`, a timeout, an error or a missing export gives the built-in rendering.
  - A TUI-side cap of 5 ticks covers a request `app.rs` dropped with `try_send`.
- **What it replaces:**
  - For a tool card, the widget replaces only the output body. The header, the diff and the footer (with `cox expand`) stay built-in, so a renderer cannot hide what ran or what changed.
  - For an assistant message, it replaces the markdown.
- **Drawing:** `plugin_ui::lines(widget, width, …)` draws through the existing `render`, the sanitize boundary. There is one `row_spans`, moved from `status.rs`. The width is `state.term.0` (T33.24).
- **Host side:** `crates/cox/src/plugin_ui.rs` serves `PluginRequest::RenderItem` through `cox_render_item` with `RENDER_DEADLINE` on `Lane::Control`, and builds `RenderItemIn` (the serialized `ToolCall`/`ToolResult`, or `ItemKind::AssistantMessage`).
- **Grants:** `Live::granted_renderers()` accepts a target with its own `ui.render:<target>` grant line. A `tool:wasm__<id>__<t>` target (`tool::qualified`) needs no line when `t` is one of the plugin's own granted tools. `PluginUiMsg::Declare` carries `renderers`.
- **Rendering is display-only:** it never reaches the rollout or a model `Request`.
Deviations:
- Files beyond the card's two: the new `item_render.rs`, `plugin_ui.rs` (TUI and host), `live.rs`, `session.rs`, `status.rs` (the `row_spans` move), `tool.rs` (`GRANT_PREFIX` is `pub(crate)`) and `tests/frames.rs`.
- The rollout is compared before and after the render within one session, because events carry random ids. The `Request`s are compared across runs with and without the renderer.
Check:
- insta snapshots `plugin_rendered`: an injected `\e[2J` is gone from the cell.
- insta snapshots `fallback_after_timeout`: a `None` answer, no answer, and a late answer all match the built-in look.
- `renderer_output_never_reaches_rollout_or_model`: a real scripted session with a granted WASM renderer over two turns. The cell draws `PAINTED`, and `PAINTED` is in neither the rollout nor any `Request`.
- `render_item_answers_its_cell_or_none_at_the_deadline`, `renderers_outside_own_tools_need_their_grant`
- In the worktree after the rebase onto T33.24: nextest 1192 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (with T33.26, T33.28, T33.13 and T35.7 together): nextest 1203 passed, 3 skipped; fmt, clippy, the slim build, the plugins workspace tests and the vendor pytest (44) clean.

#### T33.28 Rust reference example and e2e

Depends: T33.27, T33.11, T33.23, T33.25 · Size: ~190 · Files: `plugins/examples/rust/src/lib.rs`, `crates/cox-plugin-fixtures/build.rs`, `tests/plugins.rs`
Goal: the shared example from PL§13 (a turn-count status segment, a `PostToolUse` failure counter, `/<id>:reset`, a `turn_started` subscription). `cox-plugin-fixtures` (`publish = false`) builds it to `OUT_DIR` for `wasm32-unknown-unknown`. With the target missing, the build fails and names `mise install`.
Check: e2e with the real binary in a scratch `COX_HOME` and the Scripted provider: install → enable `--yes` → a two-turn `run -p` → the rollout shows the hook's effect and `cox plugin list --json` shows the contributions; `just bench` records the PL§11 timings in R§4.7.
Status: done 2026-09-26
Result: the PL§13 reference example lives in `plugins/examples/rust`, a member of the `plugins/` workspace; its plugin id is `example`.
- **Capabilities:** `events=["turn_started"]`, `hooks=["PostToolUse","PostToolUseFailure"]`, `kv`, `ui.status`, `ui.commands`.
- **Behaviour:**
  - A `status.right` segment shows "turns N · failed tools M".
  - The hook counts failed tool calls and sends the notice "failed tool calls: N".
  - `/example:reset` zeroes both counters.
  - Counters live in in-memory atomics written through to kv and reloaded in `cox_init`, because `cox_render` may not read kv (`outside_render`).
- **`crates/cox-plugin-fixtures`** (`publish = false`, no dependencies) builds the example in `build.rs`:
  - It checks the wasm32 target first; if the target is missing, `cargo::error` names `mise install`.
  - It runs the plugins workspace's cargo with `build --release --locked --target wasm32-unknown-unknown`, its own `--target-dir` under OUT_DIR and `CARGO_INCREMENTAL=0`, clearing host rustflags and clippy wrappers.
  - It copies the result to `OUT_DIR/example/{plugin.toml, example.wasm}` and deletes the nested target, so each OUT_DIR is about 332 KB.
  - Exports: `EXAMPLE_DIR`, `EXAMPLE_WASM`, `EXAMPLE_MANIFEST`.
- **Bench:** `crates/cox/examples/plugin_bench.rs` (feature `plugins`) is run by `just bench`.
- **CI:** the release `verify` job gets the wasm32 target, which its nextest and clippy runs now need.
- **Docs:** R§4.7 "Plugin timings", a row in the AGENTS.md layout table, the `toolchain.md` rust row, and one line in `docs/plugins.md`.
Deviations:
- The e2e is in `crates/cox/tests/plugins.rs`; there is no root `tests/`.
- About 245 lines excluding the guest and tests, against ~190. The bench is the extra.
- Warm start cannot be measured yet. `PluginHost::load_with` disables the wasmtime cache, so every start is a cold compile (57–97 ms), and PL§12 falsifier 2 cannot be judged. The module is 325 KiB, not the 1 MiB PL§11 names.
- `cox plugin list --json` shows only declared capabilities, not `cox_init` contributions. PL§13's "list runs `cox_init` against a stub session" is not implemented, so the e2e asserts runtime contributions through the host API.
- Every workspace build now does one nested guest build: about 15 s when the example, SDK or `cox-plugin-api` changes.
Check:
- `example_plugin_counts_failures_across_a_two_turn_headless_run`: the real binary in a scratch `COX_HOME` with the Scripted provider runs install → `enable --yes` → `run -p` → `run -p --resume`. The rollout shows "failed tool calls: 1", then "2" across both processes, and `plugin list --json` shows `loaded: true` with the declared events, hooks, kv, ui.status and ui.commands.
- `example_plugin_counts_turns_in_its_status_and_resets_them`
- `just bench`, release build on an Apple M3 Max at load average 19–25, range over 3 runs:
  - start (cold) 57–97 ms;
  - on_event (batch of 16) p50 0.12–0.17 ms;
  - render p95 0.09–0.51 ms;
  - hook round trip p95 0.38–0.72 ms.
- The plugins workspace test, clippy (host and wasm32) and fmt are clean.
- In the worktree: nextest 1166 passed, 3 skipped; `headless_run_does_not_wait_for_a_background_shell` flaked under load and passed on rerun. fmt, clippy and the slim build clean.
- On main after landing (with T33.26, T33.28, T33.13 and T35.7 together): nextest 1203 passed, 3 skipped; fmt, clippy, the slim build, the plugins workspace tests and the vendor pytest (44) clean.

#### T33.13 `cox_invoke_tool` through the engine

Depends: T33.12 · Size: ~150 · Files: `crates/cox-plugin/src/hostfn.rs`, `crates/cox-core/src/turn.rs` (a plugin-origin entry point that reuses the `PreToolUse` → engine → sandbox → archive path)
Goal: a plugin calls only the tools listed in `invoke`, and each call passes `PreToolUse`, `Engine::decide`, the sandbox and the archive. `Ask` shows "plugin <id> asks to run …". Headless mode denies it. Hook, decide, provider and render contexts get `NotInThisContext`.
Check: `plugin_invoke_denied_by_rule`, `plugin_invoke_outside_grant_is_refused`, `invoke_from_hook_context_is_refused`.
Status: done 2026-09-26
Result: a plugin runs a cox tool through the same path a model call takes.

**The call.**
- `cox_protocol::traits::ToolInvoker` has the same shape as `ModelCaller`: `invoke(id, name, input) -> Result<ToolResult, CoreError>`. A denial or a hook block is `Ok` with `ok: false`.
- `ToolInvoker for Session` (`cox-core/src/plugin_model.rs`) calls the existing `turn::run_tools` with one call and a fresh `TurnId`, as `user_shell` does. PreToolUse, `Engine::decide`, the sandbox, the tool events and the archive are therefore reused, with no second permission check.
- Before the call it emits `Notice "plugin <id> runs <name>"`, which attributes the call in the transcript and the rollout.

**Approval.**
- `turn.rs` gains a task-local `ORIGIN` ("plugin <id>"). `ask` puts it in `Source.agent`, so the normal approval prompt reads "plugin <id> asks: approve …".
- `ask` restores the session state it found, instead of forcing `RunningTools`, so a plugin can ask while no turn runs.
- In headless mode the existing no-approver Deny applies.

**Host function** (`hostfn.rs` `invoke_tool`):
- It is allowed only from `cox_on_event`, `cox_command`, `cox_key` and `cox_tool_call`; every other export gets `NotInThisContext`.
- It requires `invoke:<name>`.
- It refuses the plugin's own `wasm__<id>__*` tools. It also refuses any plugin that holds a `hooks:` grant: the invoked call fires hooks, which would queue behind the plugin's blocked worker with no wait timeout.
- It returns `ToolOutput { text: visible text, is_error: !ok, diff }`.

**Binding.** `live.rs` generalizes `LateCaller` into `Late<T>`, adds `LivePlugins::bind_tool_invoker` and `HostEnv::with_tool_invoker`. The session binds one `Arc<Session>` to both traits, held weakly.

Deviations:
- About 180 lines of code across 6 files, against ~150 and 2.
- The `hooks:` refusal is blunt. A later card could let a plugin's own invoked calls skip that plugin's hooks.

Not done:
- A cycle across two plugins' tools (A invokes B's tool, and B's tool invokes A's) is not refused. It ends at the per-call `call_ms` deadline of `WasmTool`.
- The plugin sees only the visible, possibly truncated text; it cannot `expand`.
- A plugin prompt and a model prompt at the same time share one session-state field.

Check:
- `plugin_invoke_denied_by_rule`
- `plugin_invoke_outside_grant_is_refused`
- `invoke_from_hook_context_is_refused`
- `invoke_that_would_wait_on_its_own_worker_is_refused`
- `plugin_invoke_ask_is_denied_headless` (asserts `source.agent == "plugin t"`)
- `plugin_invoke_allowed_runs_and_is_archived`
- In the worktree: nextest 1189 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (with T33.26, T33.28, T33.13 and T35.7 together): nextest 1203 passed, 3 skipped; fmt, clippy, the slim build, the plugins workspace tests and the vendor pytest (44) clean.

#### T35.7 e2e: a fake `agent` binary replaying recorded fixtures

Depends: T35.5, T35.6, T35.13 · Size: ~190 · Files: `tests/external_agents_cursor.rs` (new), `tests/fixtures/cursor/*.json` (data), `scripts/vendor/src/cox_vendor/cursor_fixtures.py` (+ its tests)
Goal: fixtures are the documented `stream-json` and ACP event shapes from research.md §4.3.8, recorded by a saved, tested script under `scripts/vendor` (AGENTS.md: a file no package manager fetches comes only from such a script, never hand-pasted) — no live Cursor call, no key. A test-only fake `agent` binary replays a fixture's lines over stdio in both modes; the e2e drives it through the real `cox-plugin`/`cox-acp`/`cox-core` path (D12: no network, no API key).
Check: `fake_agent_stream_json_reaches_a_cox_event_stream_unchanged`, `fake_agent_acp_permission_request_is_decided_by_the_engine` — both against the real code path, no scripted-provider shortcut for this one (it is not a model call).
Status: done 2026-09-26
Result: an end-to-end test drives a fake Cursor `agent` through the real external-agent path.

**Fixtures.**
- `crates/cox/tests/fixtures/cursor/{stream_json,acp}.json` are written only by `cox-vendor cursor-fixtures` (`scripts/vendor/src/cox_vendor/cursor_fixtures.py`, 7 tests). No Cursor API is called.
- stream-json: the "Example sequence" block from https://cursor.com/docs/cli/reference/output-format.md, verbatim.
- ACP:
  - the message shapes come from the ACP spec pages (agentclientprotocol.com/protocol/{initialization,session-setup,prompt-turn,tool-calls}.md);
  - the permission option ids are checked against https://cursor.com/docs/cli/acp.md.
- Nothing is written if validation fails. `--check` is a dry run, and a re-run is byte-stable.
- Run with `uv run --project scripts/vendor cox-vendor cursor-fixtures`.

**Fake agent** (`crates/cox/tests/support/fake_agent.rs`):
- It is `[[bin]] fake_agent` of `crates/cox`, with `test = false` and `doc = false`, so the tests get `CARGO_BIN_EXE_fake_agent`.
- `default-run = "cox"`, and `[package.metadata.dist.binaries]` keeps it out of release archives.
- It checks the mode args and `CURSOR_API_KEY`.
- stream-json: it replays the documented lines.
- ACP: it answers `initialize`, `session/new` and `session/prompt`, sends the chunk and the permission request, then echoes cox's chosen `optionId` in one more chunk.

**e2e** (`crates/cox/tests/external_agents_cursor.rs`):
- It runs the real binary: `plugin install --yes` of a plugin shaped like `plugins/cursor`, with `agent` on PATH pointing to the fake and a fake key, then `run -p --output-format stream-json`.
- Only the parent model is scripted (`tests/scenarios/external_agent_cursor.toml`). The agent goes through the real grant, the sandbox wrap and the mapper or the ACP client.

Deviations:
- The paths are under `crates/cox/tests/`, since there is no root `tests/`.
- The in-crate helpers in `external_agents.rs` are `pub(crate)` test code, so the e2e runs the binary instead.
- About 395 lines of Rust plus 162 of Python, against ~190.
- The fixtures carry their source URLs but no fetch date, to stay byte-stable.

Not done:
- `cargo install` of `crates/cox` would also install `fake_agent`; only the dist release excludes it.
- The e2e needs a real sandbox backend (Seatbelt, or bwrap on Linux).
- Found, not touched: `dist plan` ships cox-tui's test-only `kitty_probe` as its own release app.

Check:
- `fake_agent_stream_json_reaches_a_cox_event_stream_unchanged`: the child rollout matches every documented line in order. The init notice carries the model and mode, and the parent's `agent` result equals the last documented assistant text.
- `fake_agent_acp_permission_request_is_decided_by_the_engine`: `allow = ["Agent"]` is an Ask and fails closed to `reject-once`; `allow = ["Agent","Bash"]` gives `allow-once`.
- Both tests assert there were no warning notices.
- pytest 44 passed.
- In the worktree: nextest 1185 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (with T33.26, T33.28, T33.13 and T35.7 together): nextest 1203 passed, 3 skipped; fmt, clippy, the slim build, the plugins workspace tests and the vendor pytest (44) clean.

#### T35.11 ACP client terminals under the sandbox

Depends: T35.2, T35.3 · Size: ~190 · Files: `crates/cox-acp/src/client.rs`, `crates/cox-acp/src/terminal.rs` (new)
Goal: EA§4 allows a `terminal/*` request "only under the same `sandbox::Policy` already governing the spawned process". T35.3 refuses every such request, including when a sandbox grant is present, and advertises `terminal = false`. This card serves the terminal methods when a grant is present:
- `terminal/create` runs the command under the process's own `sandbox::Policy`, through the same sandboxed spawn `bash` uses. There is no second spawn path, and the working directory goes through `path::confine`.
- `terminal/output` returns the buffered output, capped at the request's `outputByteLimit`, and reports truncation.
- `terminal/wait_for_exit`, `terminal/kill` and `terminal/release` behave as ACP defines them. A released or finished terminal frees its process; the process group is killed, as `bash` does.
- Each command is judged by `cox_permission::Engine` as a `bash` call, just like a permission request.
- `initialize_request()` advertises `terminal = true` only when the sandbox grant is present.
Without a grant, the T35.3 refusal and its reason stay. Output shown to the user is sanitized, and output over the cap is archived before it is shortened.
Check: `acp_terminal_runs_under_the_sandbox_policy` (it writes outside the workspace and is denied; macOS and Linux paths as in T4.1/T4.2), `acp_terminal_output_respects_byte_limit_and_reports_truncation`, `acp_terminal_release_kills_the_process_group`, `acp_terminal_command_is_judged_by_the_engine` and `acp_terminal_without_sandbox_grant_is_still_refused`.
Status: done 2026-09-26
Result: the ACP client serves `terminal/create`, `output`, `wait_for_exit`, `kill` and `release` when the agent runs under a sandbox grant, and advertises `terminal = true` only then (`initialize_request(sandboxed)`). Without a grant, it refuses with T35.3's reason as before.

**Registry** (`crates/cox-acp/src/terminal.rs`) keeps one live terminal per id.
- Each command runs through `cox_tools::bash::run_line`, a thin `pub` wrapper over bash's own runner. It uses the same `sandbox::command` wrap with the agent's `SandboxPolicy`, the same env allowlist, PTY, `setsid` and group signals, so there is no second spawner.
- Output keeps the last `min(outputByteLimit, 1 MiB)` bytes, cut on a character boundary, reports `truncated`, and is sanitized.
- A 30-minute ceiling stops terminals nobody releases.
- `kill` stops the command and keeps the terminal. `release` stops it and forgets the terminal. The end of the session stops everything.

**Judging** (`client.rs` `judge()`, which `permission()` also uses):
- Each command is judged as a `bash` call. The line is the subject and the risk comes from `bash::classify`. `Ask` goes to the host approver, and the driver's `RefuseAsk` turns it into a refusal.
- A line with control characters is refused. Without this, `git status\x1b[;rm -rf x` would match a `Bash(git:*)` rule once sanitized, while the shell ran `rm`.
- The cwd passes `confined()` (`path::confine`).
- `create` and `wait_for_exit` run off the dispatch loop, so `kill` still gets through.

**Behaviour change in bash `run`:** a cancelled or timed-out run now always ends with a SIGKILL of the process group, even when the shell already exited on SIGTERM. Before this, a grandchild that ignored SIGTERM survived.

Deviations:
- 7 files instead of 2, and about 330 lines against ~190.
- `cox-acp` now depends on `cox-tools` and `tokio-util` (workspace crates; `deps.rs` allows it).
- `command` is treated as a shell fragment and each `args` entry is single-quoted.
- The request's `env` is ignored.
- A stopped command reports `signal: "SIGTERM"` with no exit code.

Not done:
- Output over the cap is not archived before it is shortened. `ClientHost` has no archive sink or ids, so this needs a follow-up card.
- No real-binary run: it needs a real ACP agent CLI.
- No dedicated test that terminals die at session end; that relies on each terminal's `DropGuard`.

Check:
- `acp_terminal_runs_under_the_sandbox_policy`: a write to `$HOME` is blocked, a write in the workspace lands, and the exit is non-zero.
- `acp_terminal_output_respects_byte_limit_and_reports_truncation`
- `acp_terminal_release_kills_the_process_group`: covers `kill` then `wait_for_exit`, and a background grandchild dies on `release`.
- `acp_terminal_command_is_judged_by_the_engine`: a deny rule refuses, `Ask` goes to the approver, and the call is rated as `bash` with `classify` risk.
- `acp_terminal_without_sandbox_grant_is_still_refused`
- Two `terminal.rs` unit tests.
- In the worktree: nextest 1189 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing: nextest 1209 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.41 Optional: `cox plugin link` (dev loop)

Depends: T33.7 · Size: ~110 · Files: `crates/cox/src/plugin_cmd.rs`, `crates/cox-plugin/src/grant.rs`
Goal: use a built plugin in place without installing it. A linked plugin asks again only when its capabilities widen, never on byte changes. It is marked `dev` in `list`, `doctor` and the TUI grant dialog. `cox plugin build` and `cox plugin dev` are not planned (PL§13).
Check: `linked_plugin_rebuild_does_not_reask`, `linked_plugin_widening_reasks`, `linked_plugin_marked_dev_everywhere`.
Status: done 2026-09-26
Result: `cox plugin link <dir> [--yes]` points a user plugin at a working directory, so the author rebuilds in place without reinstalling.
- **Pointer.** `install::link` writes the `link` pointer, reusing `write_pointer`/`read_pointer`. `discover::resolve_user_dir` prefers `link` over `current`, and `Plugin.dev` marks a linked plugin.
- **Grant key.** `Plugin::grant_digest()` is the one grant key every call site uses (`enable`, `disable`, `verdict_for`, `link`, the TUI grant requests): `link_digest()` for a dev plugin, a fixed path-independent constant, and the content digest otherwise.
  - Why a fixed key: `PluginStore::grant_get` matches `(plugin_id, scope, digest)` exactly, so a rebuild-volatile digest would never find the row again.
- **Re-asking.** `grant::check` accepts a digest mismatch only for a grant whose `source.kind == "link"`. So a rebuild does not re-ask, and widening the capabilities still does.
- `cox plugin update` refuses a linked plugin with "linked plugin: rebuild in place".
- `list` (text and JSON) and the TUI grant dialog mark the plugin "(dev)".
Deviations:
- The grant storage key is the fixed `link_digest()`, not the live digest (see above). `check` still receives the live digest for display.
Not done:
- `cox doctor` has no `dev` marker yet; T33.39 can read `discover::Plugin.dev` directly.
- The TUI grant path (`GrantDecision` → `write_plugin_grant`) still writes `source = {}` for every grant, which predates this card. A TUI-approved grant for a linked plugin is stored under the right key, but it re-asks after the next rebuild until the CLI `link`/`enable` rewrites it with `source.kind = "link"`. The follow-up is to thread `source` through `GrantDecision`.
Check:
- `linked_plugin_rebuild_does_not_reask`, `linked_plugin_widening_reasks`, `non_linked_plugin_still_reasks_on_changed_bytes`
- `link_pointer_wins_over_current_and_pins_the_grant_digest`
- `link_writes_a_readable_pointer_and_repointing_overwrites_it`
- `plugin_grant_dialog_marks_a_linked_plugin_dev`
- e2e with the real binary in a scratch `COX_HOME`: `link_grants_in_place_survives_rebuild_and_reasks_on_widening`, `update_on_a_linked_plugin_names_the_dev_loop`
- In the worktree: nextest 1118 passed, 3 skipped; fmt, clippy and the slim build clean.
- Rebased onto T33.44/T35.8. Two call sites the merge had left keyed by the live content digest now use `grant_digest()`: `session::load_plugins` and `doctor::check_external_agents`. Without the fix, a linked plugin re-asked or was skipped on every session open and doctor run. `update`/`rollback` refuse a linked plugin before any grant lookup. `remove` deletes the whole directory, pointer included (`remove_also_clears_the_link_pointer`).
- In the worktree after the rebase: nextest 1173 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (T33.41, T33.40.8, T33.21, T35.9, T33.29 and T33.39 together): nextest 1247 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.40.8 `route` in the core: cache-aware downgrade offer, turn-local thinking strip

Depends: T33.20 · Size: ~170 · Files: `crates/cox-core/src/router.rs`, `crates/cox-core/src/context.rs`, `crates/cox-core/src/session.rs`
Goal: J§5.2, core side (C2).
- The `route` point is offered only for `Job::Main`, once per `UserTurn`, sticky for that turn's calls.
- It is never offered for `Plan`, for subagents, or after `/model`.
- `cheap` is offered only when its predicted turn cost is ≤ `(1 − route_margin)` × the `code` cost. The prediction uses catalog prices, the last request's prefix size and the cache-read vs cache-write formula in J§5.2. `route_margin` goes in `[plugins.decide]` (default 0.15).
- A downgraded turn strips thinking in its own `Request` only. `inner.history` is unchanged, and there is no `ModelSwitched`.
Check:
- `downgrade_not_offered_when_cache_loss_exceeds_saving`;
- `downgrade_offered_on_first_turn`;
- `routed_down_turn_keeps_history_thinking`: the next `code` request's prefix is byte-identical, and invariant 1 stays green;
- `route_never_offered_for_plan_job`;
- `model_override_disables_route_point`;
- `route_advice_never_routes_up` still green;
- `docs/config.md` drift test green.
Status: done 2026-09-26
Result: the `route` decision point is hygienic in the core.
- **Skipped** (static pick kept, advisor not asked) by `Session::route_turn` when:
  - the job is not `Job::Main` (subagents, Plan);
  - `/think` or `--deep` is set (`confirm_think`);
  - the static tier is `think`;
  - `/model` set `overrides.main_tier`.
- **Cache-aware filter.** `router::cheap_pays(cheap, code, prefix, margin)` is the J5.2 formula, with k = 5 turns and O = 3000 output tokens from its worked example:
  - code = cache_read·P·k + out·O
  - cheap = cache_write·P + cache_read·P·(k−1) + out·O
  - `cheap` is offered only when the saving beats `margin`. `P` is `last_context_tokens`; prices come from `PriceTable::embedded()` (a static `OnceLock`, the same table the ledger's `Priced` wrapper uses).
  - When only the static tier would remain, nothing is asked.
- **Knob.** `[plugins.decide] route_margin` (default 0.15, clamped to [0, 1]; NaN never offers cheap). Break-even at default prices is about 13k prefix tokens. `docs/config.jsonschema` and `docs/config.md` are regenerated.
- **Thinking strip.** `context::strip_thinking_before(messages, turn_start)` reuses `router::strip_thinking` on the request copy before the current turn and keeps the running turn's own blocks, which a thinking tool loop needs. `inner.history` is never rewritten and no `ModelSwitched` is emitted.
Deviations:
- `cox-core` now depends on the workspace crate `cox-models`. `deps.rs` already allowed this; no external crate was added.
- About 90 lines of non-test code (~300 in total with tests and regenerated docs) against ~170, across 8 source files. `advise.rs` did not exist when the card was written.
- A tier whose model has no catalog price (for example a local cheap tier) is never offered, because no saving can be shown.
- User price files are not considered; this matches what `Priced` uses today.
- Only `/model` (`main_tier`) disables the point; a `--tier <tier>=<model>` override alone leaves it on.
Check:
- `downgrade_not_offered_when_cache_loss_exceeds_saving`
- `downgrade_offered_on_first_turn`
- `routed_down_turn_keeps_history_thinking`
- `route_never_offered_for_plan_job`
- `model_override_disables_route_point`
- `cheap_pays_follows_the_j5_2_cache_formula`
- `route_advice_never_routes_up` (unchanged); config drift tests; invariant-1 prefix tests
- The real binary with a scratch `COX_HOME`: `cox config get plugins.decide.route_margin` prints 0.15, and `cox doctor` runs.
- In the worktree: nextest 1193 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (T33.41, T33.40.8, T33.21, T35.9, T33.29 and T33.39 together): nextest 1247 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.21 Decision points: `risk`, `approve_hint`, `compact`, `rank`, `salience`

Depends: T33.20 · Size: ~190 · Files: `crates/cox-core/src/turn.rs`, `crates/cox-core/src/compact.rs`, `crates/cox-tools/src/tool_search.rs`
Goal: the monotone rules from PL§4:
- risk only raises, and (amended 2026-09-26, R§4.3.6 J§5.1) the core asks only when raising the call to `Destructive` would change the engine's outcome from `Allow` to `Ask` or `Deny` — this filter belongs here so every `risk`-capable plugin gets it, not only Jev;
- `approve_hint` is warning-only (amended 2026-09-26, `docs/design/plugins.md` §14 decision 10): a plugin may add a caution note, never say a call "looks safe" — monotone the same direction as `risk`, never used to grant quiet approval;
- compaction only happens earlier, never skipped when mandatory;
- rank reorders or filters cox's own candidates.
`salience` is wired only if it fits the size; otherwise it is a follow-up card noted here.
Check: `risk_advice_cannot_lower_risk`, `risk_not_asked_when_outcome_would_not_change`, `approve_hint_cannot_say_looks_safe`, `compact_advice_cannot_skip_mandatory_compaction`, `rank_advice_cannot_add_tools`.
Status: done 2026-09-26
Result: four monotone decision points, in `crates/cox-core/src/monotone.rs`, can only move toward caution.
- **`risk`**
  - Asked only when the point is configured, the call is neither `ReadOnly` nor `Destructive`, `Engine::decide(call)` is `Allow`, and the same call marked `Destructive` would not be `Allow`. So it is never asked in bypass mode or for calls that already ask.
  - The `Answer::Score` uses J5.1's 0–3 scale; ≥ 2.5 means `Destructive`, and `confidence >= min_confidence` is required.
  - The result is `max(builtin, advised)`. The Engine still decides; the advice only changes its input.
  - The question state is `{tool, subject, input, classifier_risk}`, scrubbed and clipped to 2k chars. Tool output is never sent.
  - It runs in `turn::gate` after PreToolUse. A call the user edited is recomputed from `tool.risk` and not re-advised.
- **`approve_hint`**
  - `Answer::Noul` counts only when `p_yes >= min_confidence`.
  - The core then frames its own Warn `Event::Notice`: `caution from plugin <id>: <sanitized note ≤200>`, just before `ApprovalRequired`.
  - No answer can yield a "looks safe" message.
- **`compact`**
  - A due compaction runs without asking.
  - Otherwise the advisor is asked only when there are more turns than `keep_turns`; a yes compacts earlier.
  - State: `{context_tokens, max_context, compact_at, turns}`.
- **`rank`**
  - The options are the names in `tool_search`'s `structured.discovered`.
  - A confident `Answer::Choice` keeps the valid indices once each, in the plugin's order. It can reorder or filter the discoveries, never add to them.
- **All points**
  - `[plugins.decide]` gains `risk`/`risk_ms`=200, `approve_hint`/`approve_hint_ms`=200, `compact`/`compact_ms`=500 and `rank`/`rank_ms`=300, sharing `min_confidence`. `default.toml`, `docs/config.jsonschema` and `docs/config.md` are regenerated.
  - Every answer emits `Event::Advised`, and `note` is kept only on an applied `approve_hint`. Silence, lateness or low confidence keeps the static behaviour.
Deviations:
- `salience` is split into the new card T33.21.1.
- A new `monotone.rs` plus one-line hunks in `turn.rs`/`session.rs` replace the card's `compact.rs` and `tool_search.rs`. That is about 285 lines outside tests (doc comments included) plus about 35 lines of wiring and config, against ~190.
- `rank` filters only which tools join the next request. The `tool_search` text the model reads still lists every hit. Skill-match ranking is not wired.
- The caution arrives as a `Notice`, not as a field on `ApprovalRequired`.
- `risk` asks once per call; `Question` has no batch `items` yet (T33.40.1).
- The test-only fake advisor is duplicated from `advise.rs` tests. `route_turn` could reuse the shared `ask_point` helper later.
Check:
- `risk_advice_cannot_lower_risk`
- `risk_not_asked_when_outcome_would_not_change`
- `approve_hint_cannot_say_looks_safe`
- `compact_advice_cannot_skip_mandatory_compaction`
- `rank_advice_cannot_add_tools`
- The real binary with a scratch `COX_HOME`: `cox config get plugins.decide` shows the new keys.
- In the worktree: nextest 1208 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (T33.41, T33.40.8, T33.21, T35.9, T33.29 and T33.39 together): nextest 1247 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T35.9 User guide: the Cursor plugin

Depends: T35.7 · Size: ~130 · Files: `docs/plugins/cursor.md`, `docs/plugins.md` (link), `crates/cox/tests/doc_examples.rs`
Goal: install and grant the plugin, set `CURSOR_API_KEY`, dispatch it with `agent(preset: "cursor")`, read `cox doctor`'s row when something is missing — the same shape `docs/plugins/jev.md` (T33.40.11) already gives Jev.
Check: the doc's commands are checked against the real binary the way `doc_examples.rs` already checks other pages.
Status: done 2026-09-26
Result: `docs/plugins/cursor.md` is the user guide for the Cursor plugin:
- install and grant (`cox plugin install plugins/cursor --yes`);
- set the dashboard-issued `CURSOR_API_KEY`;
- dispatch with `agent(preset: "cursor")`;
- ACP (default, `args = ["acp"]`) versus the stream-json mode;
- `cox doctor`'s row for a missing CLI or key.
The limits are stated: ACP permission requests that would need the user are refused, each turn starts a fresh CLI process, and no usage is reported. `docs/plugins.md` (the authoring guide) now points to it at the top.
Deviations:
- `docs/plugins/jev.md`, the shape the card cites, does not exist yet (T33.40.11). The page is written from the real behaviour instead: plugin.toml, `external_agents.rs`, `doctor.rs`'s messages and EA§1–§8.
- `crates/cox/tests/doc_examples.rs` did not exist, so it is created here, narrowly, for this page.
- 67 doc lines plus 78 test lines, against ~130.
Check:
- `cursor_doc_commands_match_the_real_binary`: the real binary in a scratch `COX_HOME`/`HOME`, with `CURSOR_API_KEY` unset and an empty PATH, installs a scratch copy of the repo's `plugins/cursor/plugin.toml` (with a stub wasm) using the doc's literal command. It asserts the printed capability line and the `cox doctor` "external agent cursor" row verbatim against the doc. There is no network, no key and no keychain.
- In the worktree: nextest 1210 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (T33.41, T33.40.8, T33.21, T35.9, T33.29 and T33.39 together): nextest 1247 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.29 `cox plugin new` and the Rust template

Depends: T33.28 · Size: ~200 · Files: `crates/cox/src/plugin_new.rs`, `crates/cox/src/cli.rs`, `plugins/templates/rust/*.tmpl` (templates do not count)
Goal: `cox plugin new <name> [--lang] [--dir] [--with …]` from PL§13:
- one pure module maps (name, lang, with) to a list of files;
- `plugin.toml` capabilities match `--with`;
- only the chosen stub exports are written, plus a `justfile`, a README and a smoke test;
- the name is validated and an existing directory is never overwritten;
- headless defaults to `rust`.
Check: e2e `cox plugin new demo --lang rust --with status,hook` in a scratch `COX_HOME` asserts the file tree and that the manifest validates against `docs/plugin.schema.json`; it then builds `--offline` with the SDK patched to the in-repo path and runs the smoke test. `new_refuses_existing_dir`, `new_rejects_invalid_name`.
Status: done 2026-09-26
Result: `cox plugin new <name> [--lang rust] [--dir <dir>] [--with <caps>]` scaffolds the PL§13 tree: `Cargo.toml`, `plugin.toml`, `src/lib.rs`, `tests/smoke.rs`, `justfile`, `README.md` and `.gitignore`.
- **Templates.** They live in `plugins/templates/rust/*.tmpl`. `cox:with=<caps>` … `cox:end` blocks are kept only for the chosen capabilities, and a skip stack resolves nested blocks.
- **Manifest.** `[capabilities]` matches `--with` exactly.
- **Id rule.** It is reused from `cox_plugin_api::is_plugin_id`, which is now public and re-exported; there is no second regex.
- **Existing directories.** They are never overwritten.
- **Reuse.** `plugin_new::scaffold` and `write` are plain functions, so T33.30's TUI command calls the same code.
- **Other languages.** Only `rust` has a template; the others answer `UnsupportedLang`, naming PL§13.
- **Dependency.** `thiserror` becomes a direct dependency of `crates/cox` for `PluginNewError`. It is already a workspace dependency with a `toolchain.md` row.
Deviations:
- The SDK dependency is a git dependency. Cargo's `[patch]` cannot resolve a fresh git source `--offline`, so the offline e2e rewrites that line to a `path` dependency on `plugins/sdk` before building.
- A nested-block bug was caught only by the real build; it is guarded by `render_keeps_only_its_chosen_nested_arm` and a brace-balance assertion.
Check:
- `new_rejects_invalid_name`
- `new_refuses_existing_dir`
- `new_writes_the_pl13_tree_and_a_manifest_that_validates`: the exact 7-file tree, and `discover::load_manifest` validates it.
- `new_scaffold_builds_offline_and_its_smoke_test_passes`: `cox plugin new demo --lang rust --with status,hook`, then an offline wasm32 build and the scaffold's own smoke test.
- 9 unit tests in `plugin_new.rs`.
- In the worktree: nextest 1216 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (T33.41, T33.40.8, T33.21, T35.9, T33.29 and T33.39 together): nextest 1247 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.39 `cox doctor` and `cox ext` plugin reporting

Depends: T33.28 · Size: ~150 · Files: `crates/cox/src/doctor.rs`, `crates/cox/src/ext_cmd.rs`
Goal: a doctor plugins row listing, for each plugin:
- loaded, skipped (with reason), not granted, or dev;
- exports disabled by the three-failure breaker;
- catalog price conflicts (T33.16);
- the wasmtime cache directory size.
`cox ext list` shows the same state.
Check: doctor snapshots in a scratch `COX_HOME` with one healthy, one broken and one ungranted plugin; `disabled_export_is_visible_in_doctor`.
Status: done 2026-09-26
Result: `cox doctor` reports plugins.
- **`doctor::check_plugins`** gives one row per discovered plugin: `loaded`, `disabled`, `not granted` or `skipped: <reason>`.
  - A linked plugin (`cox plugin link`, T33.41) also carries `dev`, the same word `cox plugin list` shows.
  - It reuses `discover::discover` and `plugin_cmd::verdict_for`, which is now `pub(crate)` and keyed by `grant_digest()`.
  - It is only ever `ok` or `warn`, never `fail`: extensions fail open.
- **Price conflicts.** A granted plugin's `[[models]]` feed `cox_models::Catalog::load`, and that plugin's catalog warnings are appended to its row.
- **Cache size.** `check_plugin_cache` reports the size of `<COX_HOME>/cache/wasmtime`; an absent directory is 0 bytes and `ok`.
- **`cox ext list`** gains a `plugins` section built from the same walk and row shape (human and JSON), so the two surfaces cannot disagree.
Deviations:
- The three-failure export breaker does not exist yet (T33.40.4), and its state lives per session in memory. So `check_plugins_with(.., disabled_exports)` is the reporting slot, and `check_plugins` passes an empty map. Whoever lands the breaker threads the map in.
- The wasmtime cache is still disabled (`with_cache_disabled()`), so the cache row reads 0 bytes today.
- The `dev` word was wired while landing, after T33.41 reached main.
- About 204 lines of code plus about 155 lines of tests, against ~150.
Check:
- `doctor_plugin_rows_report_healthy_broken_and_ungranted`: a healthy granted plugin with a price conflict, a broken one and an ungranted one.
- `disabled_export_is_visible_in_doctor`
- `plugin_cache_size_is_zero_when_not_yet_created`
- `plugin_cache_size_sums_files_recursively`
- The real binary with a scratch `COX_HOME`: `cox doctor` exits 0 and prints the plugin cache row.
- In the worktree: nextest 1207 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (T33.41, T33.40.8, T33.21, T35.9, T33.29 and T33.39 together): nextest 1247 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.35 Kotlin: feasibility spike

Depends: T33.28 · Size: ~80 (spike; a throwaway branch of files under `plugins/spikes/kotlin`, not merged) · Files: `research.md` §4.3.5, `docs/design/plugins.md` §13
Goal: prove or refute that a Kotlin/Wasm `wasmWasi` module (the latest Kotlin, R§4.3.5 P32) with `@WasmImport("extism:host/env", …)` and `@WasmExport` loads in extism 1.30.0 and round-trips `cox_init`, with and without extism's `wasmtime-exceptions` feature (P33–P34).
Falsifier: the module fails to instantiate under the wasmtime 43 extism pins, or needs a feature cox will not enable → Kotlin stays out of `--lang` and PL§13 records why. If it passes only with `wasmtime-exceptions`, ask the creator before enabling it.
Check: R§4.3.5 gains the spike's facts with versions; the PL§13 row says "proven" or "refuted".
Status: done 2026-09-26
Result: refuted. Kotlin works only with extism's `wasmtime-exceptions` feature, which cox does not enable.
- **Toolchain:** Kotlin 2.4.20, JDK Temurin 21.0.12 and Gradle 9.8.0, in a throwaway `plugins/spikes/kotlin` (deleted).
- **Module:** a minimal `wasmWasi` module with `@WasmExport("cox_init")` and `@WasmImport("extism:host/env", "alloc")`. Its only import is `extism:host/env::alloc`; it has no WASI imports.
- **Real host** (`PluginHost::load`, WASI off per A55) and raw extism with WASI on: both fail at parse time with "exceptions proposal not enabled". Kotlin 2.4.20's `wasmWasi` output always uses the exception-handling proposal, and extism 1.30.0 enables it only behind its non-default `wasmtime-exceptions` feature (P33/P34).
- **With `wasmtime-exceptions` on** (scratch only, reverted): the module instantiates and `cox_init` runs through the real host with WASI still off. WASI (A55/T33.43) is therefore not what blocks Kotlin.
- **Docs:** `research.md` §4.3.5 gains the spike's rows and a result paragraph. `docs/design/plugins.md` §13's Kotlin row and §14 decision 3 say "refuted (T33.35)". Kotlin stays out of `--lang`, and T33.36 proceeds only if the creator enables `wasmtime-exceptions` for the whole workspace, which is a decision for every plugin.
Check:
- R§4.3.5 records the versions, the exact errors and the sources: https://api.github.com/repos/JetBrains/kotlin/releases/latest and extism-1.30.0 `src/plugin.rs`, both checked 2026-09-26.
- The spike dir, the scratch example and the Cargo.toml flip are all removed. In the worktree: nextest 1247 passed, 3 skipped; fmt and clippy clean.
- On main: docs-only change (research.md and docs/design/plugins.md); both spikes' conflicting rows were resolved on landing.

#### T33.37 Dart: WASI re-check spike

Depends: T33.28 · Size: ~60 · Files: `research.md` §4.3.5, `docs/design/plugins.md` §13
Goal: re-check whether the latest Dart can emit a module that runs outside JS (dart-lang/sdk#56366, R§4.3.5 P35) and load it in extism.
Falsifier: `dart compile wasm` output still needs a JS bootstrap → Dart stays an MCP-server-only exception (T33.38) and the spike is repeated when #56366 closes.
Check: R§4.3.5 records the Dart version tried and the result.
Status: done 2026-09-26
Result: refuted again. Dart 3.13.4 (stable, macOS arm64) still needs a JS bootstrap.
- `dart compile wasm` has no WASI or non-JS target. It emits `main.wasm` plus a `main.mjs` that compiles with `builtins: ['js-string']` and supplies a `dart2wasm` JS import object.
- Loading `main.wasm` through the workspace's extism 1.30.0 / wasmtime 43.0.2 with WASI on fails: "failed to parse WebAssembly module: exceptions proposal not enabled". With extism's `wasmtime-exceptions` feature it would still need `wasm:js-string` and the JS import object, which the extism ABI does not supply.
- dart-lang/sdk#56366 is still open (last activity 2026-06-21). A third-party `wasm_tools` shim outside the SDK was not tried.
- Dart stays the MCP-server-only exception (T33.38). The spike is repeated when #56366 closes.
- `research.md` §4.3.5 gains row P44 (renumbered on landing: T33.35 took P40–P43), and the Dart lines of `docs/design/plugins.md` §13/§14 are updated.
Check:
- R§4.3.5 P44 records Dart 3.13.4, the commands, the exact instantiate error and the issue state. Sources, checked 2026-09-26: https://api.github.com/repos/dart-lang/sdk/issues/56366 and https://storage.googleapis.com/dart-archive/channels/stable/release/latest/VERSION
- The throwaway SDK, spike files and test were deleted; nothing but the two docs was committed.
- On main: docs-only change (research.md and docs/design/plugins.md); both spikes' conflicting rows were resolved on landing.

#### T33.30 `/plugin new` in the TUI

Depends: T33.29 · Size: ~120 · Files: `crates/cox-tui/src/commands.rs`, `crates/cox-tui/src/state.rs`, `crates/cox/src/session.rs`
Goal: the palette's `/plugin new <name>` asks for the language with `Modal::Picker` when it is not given, then sends a `Cmd` to the same `plugin_new` function. There is no second implementation.
Check: insta snapshot of the picker; `tui_plugin_new_calls_shared_scaffold` (a fake executor records one call with the chosen language).
Status: done 2026-09-26
Result: `/plugin new <name> [--lang <lang>] [--with <caps>]` in the TUI.
- **Parsing.** `commands.rs` parses the command into `Action::PluginNew { name, lang: Option<String>, with }` and lists it in `/help`.
- **Language picker.** Without `--lang`, `State` stashes the request and opens `Modal::Picker` (`Kind::PluginLang`). Its options come from `picker::PLUGIN_LANGS`, which lists only the languages that have a template (`rust` today); that list is extended when each template lands. `Pick::Chosen` sends `Cmd::PluginNew`.
- **No second implementation.** `app.rs` forwards a `PluginNewRequest` of plain strings over a channel. `session::run_plugin_new` is the only place that maps them back to `plugin_new::Lang` and `Capability` (clap `ValueEnum`), and it calls the same `plugin_new::scaffold` and `write` that `cox plugin new` calls.
- **Result.** Success or failure comes back as `Msg::PluginNew` and shows as a notice.
- An explicit `--lang go` (or another language without a template) reaches `scaffold`, and its "no template yet" error shows as a notice.
Deviations:
- 8 files. The extra ones are `picker.rs` (the new picker kind), `app.rs` (the channel), `kitty_probe.rs` (the new `run` signature) and the updated `/help` screenshot snapshot.
- About 300 lines, including tests, against ~120.
Check:
- insta snapshot `picker_plugin_lang_snapshot`
- `tui_plugin_new_calls_shared_scaffold`: a fake executor records one call with the chosen language.
- `plugin_new_parses_name_lang_and_with`
- `screen_help_overlay` snapshot updated for the new help row.
- The real binary with a scratch `COX_HOME`: `cox plugin new demo` scaffolds, and `cox doctor` runs.
- In the worktree: nextest 1250 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing: nextest 1250 passed, 3 skipped; fmt, clippy and the slim build clean.

#### T33.21.1 Decision point: `salience`

Depends: T33.21 · Size: ~120 · Files: `crates/cox-core/src/memory_extract.rs`, `crates/cox-core/src/monotone.rs`, `crates/cox-protocol/src/config.rs`
Goal: PL§4 `salience`, split out of T33.21. Memory extraction asks the `[plugins.decide] salience` plugin (within `salience_ms`, default 300) for a `Score` per extracted item; items are scrubbed and one question carries every item of the extraction. The score only orders or drops items against the `[memory]` thresholds; it never adds or edits an item and never lowers a threshold. Silence, lateness or low confidence keeps every item. Every answer emits `Event::Advised { point: salience }`. Regenerate `docs/config.jsonschema`, `docs/config.md` and `default.toml` through their drift tests.
Check: `salience_advice_cannot_add_memory_items`, `salience_thresholds_stay_in_config`, `late_salience_keeps_every_item`.
Status: done 2026-09-26
Result: the `salience` decision point, which can only drop memory items.
- **Where it runs.** `extract_memory` passes the parsed facts through `Session::advise_salience` before dedup and save. It reuses T33.21's shared `ask_point`, `advised` and `confident` helpers in `monotone.rs`.
- **Question and answer.** One question carries every item in `Question.state["items"]` (`{name, kind, body}`, scrubbed and clipped). The answer is the new additive `Answer::Scores { values }`, one score per item by index.
- **Monotone rule** (`keep_salient`):
  - An item is dropped only if its score is below `[memory].salience_min` (default 0.3). That value comes from config, and the plugin cannot change it.
  - Scores are applied only when the batch confidence is at least `min_confidence` and there is exactly one value per item.
  - A shape mismatch, low or absent confidence, silence or lateness (`salience_ms`, default 300) keeps every item.
  - An empty extraction is never asked about.
  - Every answer emits `Event::Advised { point: salience }`.
- **Generated docs.** `docs/config.jsonschema`, `docs/config.md`, `default.toml`, `docs/protocol.jsonschema` and `docs/plugin-abi.schema.json` are regenerated through their drift tests.
Deviations:
- `cox-plugin-api/src/abi.rs` gains `Answer::Scores`. The change is additive and was needed because `Question` has no batch `items` yet (T33.40.1). The `state["items"]` shape is scoped to `salience` and is not the final batched ABI.
- `router.rs` gets a one-line match arm for the new variant.
- `[memory].salience_min` is one threshold, not a table.
Check:
- `salience_advice_cannot_add_memory_items`
- `salience_thresholds_stay_in_config`
- `late_salience_keeps_every_item`: covers lateness, no plugin configured, and low confidence.
- The real binary with a scratch `COX_HOME`: `cox config get plugins.decide` shows `salience` and `salience_ms`, and `cox config get memory` shows `salience_min`.
- In the worktree: nextest 1250 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing: nextest 1253 run, 1252 passed, 3 skipped; headless_run_does_not_wait_for_a_background_shell flaked under load and passed on rerun. fmt, clippy and the slim build clean.
