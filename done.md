
#### T35.14 Sandboxed plugin and external-agent programs may live under `/tmp`

Model: Cursor / grok 4.7 · Status: done 2026-09-27 · Depends: none · Size: ~200 · Priority: P2 · Complexity: 3
Goal: on Linux, bwrap gives a wrapped program a private `/tmp`, so a plugin's `[[mcp]]` server, an external agent or a PATH directory under `/tmp` cannot be found inside the sandbox. The directory that holds the spawned program (the plugin package or `COX_HOME` when the program lives there) is bound read-only, and the rest of the host `/tmp` stays hidden. Landlock and Seatbelt are unchanged.
Files: `crates/cox-sandbox/src/sandbox/bwrap.rs`, `crates/cox/src/session.rs`, `AGENTS.md`.
What landed: `expose_under_private_tmp` inserts one `--ro-bind` before `--` for the highest directory under `/tmp` that contains the program. It never binds `/tmp` itself. A program already inside a writable `--bind` is left alone, and the climb stops before a wider mount would hide another writable bind. `sandboxed_argv` resolves a bare PATH name and passes that file in when the backend is bwrap. `AGENTS.md` notes that a `COX_HOME` or plugin program under `/tmp` is mounted back that way.
Check:
```text
$ mise exec -- cargo nextest run -p cox-sandbox -E 'test(expose_) or test(bwrap_)'
6 tests run: 6 passed, 8 skipped
$ mise exec -- cargo clippy -p cox-sandbox -p cox --all-targets -- -D warnings
Finished `dev` profile
```
The Linux exec test `bwrap_runs_a_program_from_a_private_tmp_dir_and_hides_siblings` is `#[cfg(target_os = "linux")]` and was not run on this Mac.

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

#### T33.33 `/plugin update | remove | list | reload` in the TUI

Depends: T33.30, T33.32 · Size: ~140 · Files: `crates/cox-tui/src/commands.rs`, `crates/cox-tui/src/state.rs`, `crates/cox/src/session.rs`
Goal: the palette entries reach the same `plugin_cmd` functions through `Cmd`. Remove asks through a modal. `/plugin reload` means `/clear` with a notice that the cache prefix restarts. In-session remove stops the instance; its frozen tools answer `Denied { why: "plugin removed" }`.
Check: insta snapshot of the remove confirmation; `removed_plugin_tool_is_denied_until_next_session`.
Status: done 2026-09-26
Result: `/plugin update | remove | list | reload` in the TUI.
- **One channel.** T33.30's channel now carries `PluginMgmtRequest` (`New`, `Update`, `Remove`, `List`) through `Cmd::PluginMgmt` and `Msg::PluginMgmt`, and `session::run_plugin_mgmt` dispatches it to the same `plugin_cmd` code the CLI uses.
- **`plugin_cmd` refactor.** `update` and `remove` (and `install`/`link`/`enable`, which shared the pattern) have a stdin-free core that collects lines and takes a `confirm` callback. The CLI wrappers print the same lines in the same order, so CLI output is unchanged. `update_for_tui` and `remove_for_tui` return one string.
- **Remove.** It asks through the new `RemoveConfirm` modal (`Modal::PluginRemove`, y/n/Esc). On success `PluginRequest::Stop` goes through the existing plugin UI channel and sets `PluginHost::stop()`, a liveness flag only; the worker thread ends at session end as before. `WasmTool::call` then answers `ToolError::Denied { why: "plugin removed" }` until the next session. This is a liveness answer; the Engine still decides every call first.
- **Reload.** `/plugin reload` is `/clear` plus a notice that the cache prefix restarts.
Deviations:
- 13 files, beyond the card's three: `modal.rs` and `view.rs` (the new modal), `app.rs` and `kitty_probe.rs` (the renamed channel), `plugin_cmd.rs` (the refactor), `plugin_ui.rs` (routing `Stop`), and `cox-plugin` `host.rs`/`tool.rs` (the liveness flag).
- While landing, `Stop` was routed through `answer()` and replies with a plain `Redraw`, replacing an `unreachable!()` arm (no panics outside tests).
- Mid-session remove does not reclaim the plugin's thread or memory; they are released at session end.
Check:
- insta `plugin_remove_confirm_snapshot`
- `plugin_remove_confirm_shows_keep_data`
- `removed_plugin_tool_is_denied_until_next_session`
- `stop_flags_is_stopped_without_closing_the_worker`
- 4 parse tests; the `screen_help_overlay` snapshot is updated for the widened `/plugin` row.
- The real binary with a scratch `COX_HOME`: `cox plugin list --json`, `cox plugin --help` and `cox doctor` run.
- In the worktree: nextest 1257 passed, 3 skipped; fmt, clippy and the slim build clean.
- On main after landing (T33.33 and T33.38 together): nextest 1268 passed, 4 skipped; fmt, clippy and the slim build clean.

#### T33.38 Dart: MCP-server plugin template and example

Depends: T33.19, T33.29, T33.37 · Size: ~170 · Files: `plugins/examples/dart/bin/server.dart`, `plugins/templates/dart/*.tmpl`, `crates/cox/src/plugin_new.rs`; `plugins/mise.toml` gets dart
Goal: `--lang dart` scaffolds a package whose only capability is an `[[mcp]]` stdio server (`dart compile exe`, `dart_mcp` 0.5.2), with no `plugin.wasm`. `--with` accepts only `tool` and `mcp` for Dart and says why for anything else. The example serves one `count` tool.
Check: `plugin_example_dart` is ignored with its reason locally and runs in the CI job (the tool is callable through `mcp__<id>-count__count` and runs under the sandbox); `new_dart_rejects_status_with_reason`.
Status: done 2026-09-26
Result: `cox plugin new --lang dart` and the Dart example.
- **Scaffold.** The package's only capability is an `[[mcp]]` stdio server built with `dart compile exe` on `dart_mcp` 0.5.2, and it has no `plugin.wasm`.
  - `plugin_new.rs` extends T33.29's `Lang`, `Capability` and renderer with `scaffold_dart`.
  - `--with` accepts only `tool` and `mcp`; both scaffold the same server. Anything else is rejected with an error naming PL§13/§14 and R§4.3.5 P44.
  - Templates are in `plugins/templates/dart/*.tmpl`.
- **Example.** `plugins/examples/dart` serves one `count` tool.
- **Wasm-less packages.** `PluginManifest.wasm` is now `Option<String>`. `validate()` requires it unless the package is `[[mcp]]`-only; otherwise it returns `ManifestError::MissingWasm`. `docs/plugin.schema.json` is regenerated.
  - `load_plugins` creates no extism instance for such a package, and its granted `[[mcp]]` servers register through the same grant check and sandbox path as a wasm plugin's.
  - `discover`, doctor and the fixtures handle `None`.
- **Toolchain and CI.**
  - `plugins/mise.toml` pins Dart 3.13.4, with `toolchain.md` rows for dart and dart_mcp.
  - `just plugin-examples <lang>` runs the examples.
  - A new CI job `plugin-examples` installs Dart through `dart-lang/setup-dart@v1` and fails when the toolchain is missing. It is part of `revert-on-failure`'s `needs`, like `audit` and `deny`.
- **Picker.** `dart` is added to the TUI picker's `PLUGIN_LANGS` while landing.
Deviations:
- The card assumed a wasm-less package already loads; it did not, so the host support is part of this task (second commit).
- `dart compile exe -o build/X` does not create `build/`, so every call site does `mkdir -p build` first.
- The e2e grants `mcp__example-dart-count__*` in the scratch config, because a headless run has no approver. It calls `count` twice in one turn, since each `cox run` starts a fresh server.
- About 1000 lines in total, mostly templates and the example, against ~170.
- **Incident.** During a manual repro, one `cox plugin install` ran without a scratch `COX_HOME` and wrote to the real `~/.cox`. The installed files were removed, but a `plugin_grants` row for `example-dart` is left in the real `~/.cox/cox.db` for the creator to delete.
Check:
- `plugin_example_dart` (ignored locally, runs in the `plugin-examples` CI job) passed locally with Dart 3.13.4: the tool is callable as `mcp__example-dart-count__count` under the sandbox.
- `new_dart_rejects_status_with_reason`
- `wasm_less_mcp_only_plugin_installs_grants_and_registers_its_server`
- `doctor_reports_a_wasm_less_mcp_only_plugin_as_loaded`
- 3 manifest validation tests.
- In the worktree: nextest 1255 passed, 4 skipped; fmt, clippy and the slim build clean.
- On main after landing (T33.33 and T33.38 together): nextest 1268 passed, 4 skipped; fmt, clippy and the slim build clean.

#### T36.1 Permission rules match every segment of a compound bash command

Depends: none · Size: ~200 · Files: `crates/cox-permission/src/lib.rs`, `src/rules.rs`, `crates/cox-protocol/src/types.rs` (`ToolCall`), `crates/cox-tools/src/bash/mod.rs` (+ `bash/classify.rs` for the shared tree-sitter walk), `crates/cox-core/src/turn.rs` (where `subject` is filled)
Goal: close the gap where a prefix rule or grant matches the whole command line as one string. Today `allow = ["Bash(git:*)"]` allows `git status; rm -rf ~` because `Subject::Prefix` checks only the start of the subject (`rules.rs` `Rule::matches`), and a session grant ("always allow this session") does the same with `call.subject.starts_with` (`lib.rs` `decide`). The mirror gap is worse: `deny = ["Bash(rm:*)"]` does not deny `git status && rm -rf x`. Done means:
- the bash tool splits its command with the tree-sitter parse it already runs for `classify` (`cox_syntax::parse_bash`) into simple-command segments across `;`, `&&`, `||`, `|`, `&` and newlines, and hands them to the engine next to the display `subject` (for example `ToolCall.segments`; other tools keep one segment, their subject). `cox-permission` stays pure: it gets strings, never a parser;
- a deny rule that matches **any** segment denies the call; an allow rule or a session grant allows it only when **every** segment is covered; otherwise the usual ask path applies;
- a command the split cannot see through — command substitution `$(…)` or backticks, process substitution, `eval`/`bash -c`/`sh -c`, a parse error, or an output redirect to a path — is never allowed by a prefix rule or grant: it asks (deny still wins), even when its first word matches;
- an exact (non-prefix) rule keeps matching the whole command string as today; `ReadOnly` auto-allow and bypass mode are unchanged;
- `docs/how-it-works.md` (the rule grammar section) and the permission docs say how compound commands are matched; research.md cites Claude Code's documented behaviour for `Bash(cmd:*)` with shell operators (primary source, checked date) so `.claude/settings.json` rules keep the meaning users expect.
Check: `prefix_rule_does_not_cover_chained_command` (`git status; rm -rf x`, `git log && curl … | sh`), `deny_rule_matches_any_segment`, `session_grant_does_not_cover_chained_command`, `substitution_asks_even_when_prefix_matches` (`git log $(rm x)`, backticks, `bash -c`), `every_segment_allowed_runs_without_asking` (`git status && git diff`), `exact_rule_still_matches_whole_command`; the real binary in a scratch `COX_HOME` with `allow = ["Bash(git:*)"]` asks for `git status; touch x` in `cox run -p` (headless denies what would ask).
Status: done 2026-09-26
Result: a prefix rule or session grant covers exactly the commands it names, never a command chained after them.
- **Split.** `classify.rs` does one tree-sitter walk that returns the risk and the simple commands across `;`, `&&`, `||`, `|`, `&` and newlines, including commands nested in a subshell, `$(…)` or a loop body. Each command's text starts at its name, so deny sees past a leading `FOO=1`.
- **Interface.** `ToolCall.segments: Option<Segments { commands, opaque }>` (cox-protocol, `serde(default)`, skipped when `None`, so old rollouts load; `docs/protocol.jsonschema` regenerated). `Tool::segments(&input)` defaults to `None`; `BashTool` fills it from `cox_tools::bash::segments`. `turn.rs` `rate()` updates risk, subject and segments together after a hook rewrite or a user edit. Grants are recorded through `cox_permission::grants_for(&call)`, one per command.
- **Engine.** Deny and ask match the whole line or any command. Allow and grants need every command, possibly by different rules. An opaque line (`$(…)`, backticks, `<(…)`, `eval`, `sh -c`/`bash -c` also behind `env`/`nohup`, an assignment or `export`/`declare`/`unset`, an output redirect to a path, a parse error, no command) is never allowed by a prefix rule; it takes the ask path (headless denies), and deny still wins. Bare `Bash` and exact rules still match the whole line; read-only auto-allow and bypass are unchanged. `cox-permission` stays pure.
- **Other surfaces.** The ACP client (`request_permission` judged as bash and `terminal/create`) and `cox mcp` fill the segments; the ACP grant list and grants rebuilt from a rollout use `grants_for`.
- **Claude Code.** https://code.claude.com/docs/en/permissions (checked 2026-09-26), research.md §8.5 row 38. Same separators, any-command deny/ask, every-command allow, nested commands count, one grant per command. cox is stricter: no wrapper stripping (so `nohup rm …` passes a `Bash(rm:*)` deny in cox, but only via the ask path), any assignment blocks allow, an output redirect asks instead of checking `Edit` rules, deny does not look inside `sh -c` strings.
Deviations:
- An opaque line can still be covered by a session grant for that exact line (same trust as an exact rule).
- 32 files, about 573 lines added and 62 removed; most are mechanical `segments: None` in existing `ToolCall` literals, plus tests and docs.
- `risk()` and `segments()` each parse the line once (one shared walk, parsed twice).
Not done:
- The read-only auto-allow still lets through `GIT_PAGER='rm x' git log` and `export PATH=/tmp/evil; git status`: `classify` drops assignments as if they did not change what runs. Outside this card; the sandbox still applies.
- Wrapper stripping and deny inside `sh -c` strings.
Check:
- `prefix_rule_does_not_cover_chained_command`, `deny_rule_matches_any_segment`, `session_grant_does_not_cover_chained_command`, `substitution_asks_even_when_prefix_matches`, `every_segment_allowed_runs_without_asking`, `exact_rule_still_matches_whole_command` (crates/cox-core/tests/permission.rs, through the real `BashTool`)
- `bash_segments_split_every_operator_and_keep_nested_commands`
- e2e `a_prefix_rule_does_not_allow_a_command_chained_after_it` (scenario `bash_git_then_touch.toml`): with `allow = ["Bash(git:*)"]` the run exits 2 with `denied:1` and the file is not created; adding `Bash(touch:*)` runs it.
- The real binary with a scratch `COX_HOME`: `git status; touch x` shows `segments: ["git status","touch x"]`, is denied headless, exit 2, no `x`.
- In the worktree: nextest 1298 passed, 4 skipped; fmt, clippy and the slim build clean.
- On main after landing: nextest 1298 passed, 4 skipped; fmt, clippy and the slim build clean.

#### T36.2 The read-only rating and deny see through assignments, wrappers and `sh -c`

Depends: T36.1 · Size: ~150 · Files: `crates/cox-tools/src/bash/classify.rs`, `crates/cox-tools/src/bash/mod.rs`, `crates/cox-tools/tests/bash.rs`, `crates/cox-core/tests/permission.rs`, `docs/how-it-works.md`
Goal: close the gaps T36.1 left (its done.md card, "Not done"). `classify` drops variable assignments as if they did not change what runs, so `GIT_PAGER='rm x' git log` and `export PATH=/tmp/evil; git status` are rated `ReadOnly` and run without asking (only the sandbox applies). Done means:
- a command with an assignment prefix, or a line with `export`/`declare`/`unset`/a bare assignment, is never rated `ReadOnly`; at least `Exec`, so it takes the normal ask path. A short allow-list of assignments known not to change what runs (for example `LC_ALL`, `LANG`, `TZ`, `NO_COLOR`) may stay read-only, with its source in research.md;
- deny and ask matching strips the wrappers Claude Code strips (`timeout`, `nice`, `nohup`, `time`, bare `xargs`, per research.md §8.5 row 38) before matching a command, so `nohup rm -rf x` is caught by `Bash(rm:*)` in `deny`;
- deny looks inside `sh -c '…'`/`bash -c '…'` string arguments (split with the same walk); allow still never covers them (T36.1);
- read-only commands without such a prefix still auto-allow as today.
Check: `assignment_prefix_is_not_read_only` (`GIT_PAGER='rm x' git log`, `PAGER=… man ls`, `export PATH=/tmp/evil; git status`), `safe_locale_assignment_stays_read_only`, `deny_sees_through_wrappers` (`nohup rm -rf x`, `timeout 5 rm x`), `deny_looks_inside_sh_c`; the real binary in a scratch `COX_HOME` asks (headless: denies) `GIT_PAGER='touch x' git log` and `x` is not created.
Status: done 2026-09-27
Result: the gaps T36.1 left are closed in the bash tool's shared tree-sitter walk (`crates/cox-tools/src/bash/classify.rs`); `cox_permission::Engine` is unchanged and stays pure.
- **Assignments.** A variable-assignment prefix rates the command at least `Exec` (and marks the line opaque) unless the variable is on `SAFE_ASSIGNMENTS` (`LC_ALL`, `LANG`, `TZ`, `NO_COLOR`). `export`/`declare`/`typeset`/`readonly`/`local` and `unset` always do, with no safe-list exception. So `GIT_PAGER='rm x' git log` now asks.
- **Wrappers.** `env`, `command`, `builtin`, `noglob`, `nohup`, `time`, `nice`, `timeout` (its duration is skipped), `stdbuf`, bare `xargs` and `exec` are stripped before deny and ask match a command, so `nohup rm -rf x` is caught by `Bash(rm:*)`. One `wrapper_args` helper drives both the risk and the segment path.
- **Inner code.** `eval`, `sh -c` and `bash -c` strings are re-parsed with the same walk (`MAX_INNER_DEPTH` = 4), so deny and ask see the commands inside; allow still never covers them (opaque, T36.1). This is stricter than Claude Code, whose own docs say its deny does not stop `bash -c 'rm -rf build/'`.
- Docs: `docs/how-it-works.md` (compound matching); research.md §8.5 row 38 updated and row 39 added (https://code.claude.com/docs/en/permissions, checked 2026-09-26: `NODE_ENV` is its only safe-variable example, the bare-`xargs` rule, the wrappers it never strips).
Deviations:
- `export`/`declare`/`unset` get no safe-list exception, even `export LC_ALL=C`.
- The wrapper list is Claude Code's full documented list, wider than the card's examples.
Check:
- `assignment_prefix_is_not_read_only`, `safe_locale_assignment_stays_read_only` (crates/cox-tools/tests/bash.rs)
- `deny_sees_through_wrappers` (nohup, timeout with a duration, stacked wrappers), `deny_looks_inside_sh_c` (sh -c, compound bash -c, sh -c behind a wrapper) through the real `BashTool` and `Engine`
- e2e `an_assignment_prefix_asks_instead_of_auto_allowing` (scenario `bash_assignment_prefix.toml`): the real binary in a scratch `COX_HOME` denies `GIT_PAGER='touch x' git log` headless, exit 2, `x` not created.
- In the worktree: nextest 1307 passed, 4 skipped; fmt, clippy and the slim build clean.
- On main after landing: nextest 1307 passed, 4 skipped; fmt, clippy and the slim build clean.

#### T38.1 OpenAI Chat wire emits `ToolUseEnd`

Model: Claude Code / opus-5.5 · Status: done 2026-09-28 · Depends: — · Size: ~150 · Files: `crates/cox-provider-openai/src/chat.rs` (+ a fixture under its tests)

Goal: a tool call streamed over the Chat Completions wire (OpenAI Chat, Ollama, vLLM, LM Studio, OpenRouter) reaches the core. Today `chat.rs` emits `ToolUseStart` and input deltas but never `ToolUseEnd`, and `turn::consume_provider` commits a call only on `ToolUseEnd` — the bug T30.6 fixed for Anthropic. Chat interleaves parallel calls by `index`, so each call's start, deltas and end must come out in order (buffer per index, flush on `finish_reason`).

Check: a scripted Chat SSE stream with two interleaved parallel tool calls yields, per call, `ToolUseStart` → its deltas → `ToolUseEnd`, and a core-level test commits both calls; the regression test fails without the fix.

Execution plan:

1. Tests first. `chat.rs`: `chat_stream_parallel_tool_calls_by_index` asserts the exact order Start(0) → delta(0, whole arguments) → End → Start(1) → delta(1) → End → Stop over the existing interleaved fixture `fixtures/openai-chat/parallel_tool_calls.sse`. New `crates/cox-core/tests/chat_wire.rs`: a `Provider` that feeds an inline Chat SSE body with two interleaved `echo` calls through `cox_provider::openai::chat::OpenAiChatStream` (then a plain-text reply), and asserts both calls reach `ToolCallDone` with their own input. Both fail on `main`.
2. Fix in `chat.rs`: `on_tool_call_chunk` only accumulates per wire index (no events); a `flush` drains the accumulators in index order as `ToolUseStart` → one `ToolUseInputDelta` (the whole arguments, when non-empty) → `ToolUseEnd`. It runs before `Stop` on any `finish_reason`, and once more after the SSE body ends (`finish`, called from `stream_once`) for a server that closes without a `finish_reason`. The `started` flag goes away.
3. Accept the changed `chat_stream_one_tool_call`/`chat_stream_parallel_tool_calls` snapshots; run fmt, clippy, nextest.

Done when: the Check passes and the three AGENTS.md commands are clean.

Out of scope: live recording against a paid key; the Responses wire (already correct).
Status: done 2026-09-28
Result: `OpenAiChatStream` (`crates/cox-provider-openai/src/chat.rs`) now only accumulates tool-call chunks per wire index; `flush` emits the batch in index order, each call as `ToolUseStart` → one `ToolUseInputDelta` with its whole arguments → `ToolUseEnd`, before `Stop` on any `finish_reason`, and `finish` flushes once more when the SSE body ends without one (`stream_once` calls it). The `started` flag is gone. No new dependency.
Check:
- `chat_stream_parallel_tool_calls_come_out_whole_each_ending_before_the_next` (interleaved fixture `fixtures/openai-chat/parallel_tool_calls.sse`), `chat_stream_calls_left_open_by_a_body_without_finish_reason_end_on_finish` (chat.rs); snapshots `chat_stream_one_tool_call`/`chat_stream_parallel_tool_calls` gained `tool_use_end`.
- Core level: `chat_wire_parallel_calls_both_commit_and_run` (`crates/cox-core/tests/chat_wire.rs`) runs two interleaved `echo` calls through the real `OpenAiChatStream` into the loop; both reach `ToolCallDone`.
- Without the fix all three failed (core test: no `ToolCallDone` at all).
- In the worktree: nextest 1313 passed, 4 skipped; fmt and clippy clean.

#### T38.2 Detached `bash` from an older turn is killed on quit

Model: Claude Code / opus-5.5 · Depends: — · Size: ~180 · Files: `crates/cox-tools` (bash spawn/cancel), `crates/cox-core` (session-scoped token), `crates/cox` or `crates/cox-tui` (quit path)

Goal: no orphaned shell after cox exits. Cancellation is turn-scoped (T34.11 follow-up), so once the user sends another prompt, `interrupt()` at TUI quit no longer reaches a detached shell's `ToolCx::cancel`; `wait_tasks_cleared` gives up after `SHELL_CANCEL_GRACE` and the process is orphaned (reproduced with `sleep 4003`, ppid 1). A session-scoped token that detached shell tasks also watch closes it for TUI quit, headless `--loop`, `/clear`, fork and handoff alike.

Check: a test starts a detached `bash` in turn 1, runs turn 2, ends the session, and asserts the shell's process group is gone within the grace period; it fails without the fix. Manual: the `sleep 4003` repro against a `COX_HOME` scratch tree leaves no process with ppid 1.

Done when: the Check passes and the three AGENTS.md commands are clean.

Out of scope: changing turn-scoped cancellation for foreground tools.

Plan:
1. `crates/cox-core/src/session.rs`: a session-scoped `CancellationToken` (`ended`) next to the turn token. Every turn token becomes `ended.child_token()` (at build and at each reset: `run_turn_inner`, `user_shell`, and `ToolInvoker::invoke` in `plugin_model.rs`, through one `renew_cancel` helper), so a detached shell's `ToolCx::cancel` clone from any older turn is still a descendant of `ended`. `pub fn end()` cancels it; `interrupt()` stays turn-scoped. `spawn_child` roots a child's `ended` under the parent's, so a subagent's detached shell dies too.
2. `crates/cox/src/run.rs` and `crates/cox/src/session.rs`: the two session-exit sites (headless `run`/`--loop`; TUI quit, `/clear`, fork, handoff) call `end()` instead of `interrupt()` before `wait_tasks_cleared(SHELL_CANCEL_GRACE)`; comments that describe the old limit are corrected. The bash kill-group path (`cox-tools` `bash::run`) is reused as is: it already SIGTERM→SIGKILLs the group when its token fires.
3. Regression test `ending_the_session_kills_a_shell_detached_in_an_older_turn` in `crates/cox-core/tests/bash_tasks.rs` (real `BashTool`, scripted provider): turn 1 detaches `sleep 4011.<test pid>` (a command line unique to the run), turn 2 runs, `end()` + `wait_tasks_cleared`, then `pgrep -f` is polled with a deadline of the grace period; any leftover is killed before the assert. Run once with `end()` aliased to `interrupt()` to see it fail.
4. Verify: the Check test, the manual `sleep 4003` repro with the real binary against a scratch `COX_HOME`, then fmt, clippy, nextest.
Status: done 2026-09-28
Result: a session-scoped `CancellationToken` (`Session::ended`, `crates/cox-core/src/session.rs`) is now the parent of every turn token (`renew_cancel` replaces the three `CancellationToken::new()` resets: `run_turn_inner`, `user_shell`, `ToolInvoker::invoke`). `Session::end()` cancels it, so the `ToolCx::cancel` a shell detached in any older turn cloned fires too and the bash tool's own SIGTERM→SIGKILL of the process group runs; `interrupt()` stays turn-scoped. `spawn_child` roots a subagent's token under its parent's. Headless `run`/`--loop` and the TUI exit path (quit, `/clear`, fork, handoff) call `end()` instead of `interrupt()` before `wait_tasks_cleared(SHELL_CANCEL_GRACE)`. No new kill path; sandbox and permission guards untouched.
Deviations: 6 files instead of ≤3, all small: the core token (`session.rs`), the one reset in `plugin_model.rs` (else a plugin-invoked detached shell would escape `end`), doc-only corrections in `tasks.rs`, one call site each in `crates/cox/src/run.rs` and `crates/cox/src/session.rs`, and the test. About 80 LOC without the test.
Check output:
- `ending_the_session_kills_a_shell_detached_in_an_older_turn` (`crates/cox-core/tests/bash_tasks.rs`): failed with `end()` aliased to `interrupt()` (`sleep 4011.<pid>` outlived the session after 10 s), passes with the fix.
- Manual, real binary, `COX_HOME=/tmp/cox-t38.2`, scripted provider, `sleep 4003` detached in turn 1: headless `run --loop 1s --max-iterations 2` exits about 2 s later and leaves no `sleep 4003`; TUI (PTY) with two prompts then Ctrl+C ×2 exits in 0.6 s and leaves no `sleep 4003` (before the fix it survived with ppid 1, T34.11).
- nextest 1312 passed, 4 skipped; fmt and clippy clean.

#### T37.0 Land the desktop design docs and tokens

Depends: — · Size: ~0 (docs and data) · Files: `docs/design/desktop.md`, `desktop/design/DESIGN.md`, `desktop/design/tokens/*.json`
Goal: the design doc, the design-system guide, the DTCG token files and the HTML mockups are in the tree, so every later card can cite DT§n and DS§n.
Check: `docs/design/desktop.md` and `desktop/design/DESIGN.md` exist; each token file parses as JSON; every DS§6 CSS class named in the catalogue occurs in `desktop/design/mockups/mockups.html`.
Status: done 2026-09-28
Result: `docs/design/desktop.md` (DT§n), `desktop/design/DESIGN.md` (DS§n), the DTCG 2025.10 token files in `desktop/design/tokens/` with their generator `desktop/design/build_tokens.py` (temporary until T37.17), and the HTML mockups with `render.sh` in `desktop/design/mockups/` are in the tree. Rendered PNGs stay out of git (`mockups/.gitignore`); `render.sh` recreates them.
Check:
- `python3 -c 'import json,glob; [json.load(open(f)) for f in glob.glob("desktop/design/tokens/*.json")]'` exits 0 (3 files).
- All 61 mockup CSS classes named in DS§6 occur in `mockups.html` (the other 6 names the check picked up are SF Symbol names from DS§3.7).
- `build_tokens.py` regenerates the token files with no diff.

#### T37.7 `cox-render`: a neutral `StyledDoc` for markdown and highlighting

Depends: — · Size: ~200 · Files: `crates/cox-render/src/doc.rs`, `crates/cox-render/src/markdown.rs`, `crates/cox-render/Cargo.toml`
Goal: markdown and syntax highlighting produce runs tagged with `StyleToken` roles that both ratatui and SwiftUI can draw; ratatui sits behind a feature.
Check: the TUI transcript snapshots are unchanged; `StyledDoc` snapshots exist for a markdown fixture with code, lists and links.
Status: done 2026-09-28
Result: `crates/cox-render/src/doc.rs` adds `StyledDoc` — blocks (`Text{Paragraph|Heading(n)|List|Quote}`, `Code{lang}`, `Table`, `Rule`) of lines of `StyledSpan{text, token: StyleToken, rgb, bold, italic, strike, underline, link}`. `markdown::parse` and `highlight_runs` build it with no terminal; the ratatui `render`/`highlight` are thin adapters over them. A default `ratatui` feature gates color, diff, link, svg, theme and `Look`, so a non-TUI consumer (cox-app) can depend on cox-render without ratatui.

Deviations: `StyledSpan` also carries `rgb` (syntect's per-run colour, which a `StyleToken` cannot hold), `strike` and `underline`; 5 files instead of 3 (`lib.rs` gating, `AGENTS.md` row, `Cargo.lock` for insta dev-dep); `deps.rs` unchanged (no ratatui rule there).

Check: `cargo nextest run -p cox-render -p cox-tui` 295/295, TUI snapshots unchanged; new snapshot `markdown_parses_into_tagged_blocks_without_a_terminal`; `cargo check`/`nextest -p cox-render --no-default-features` 6/6; clippy clean with and without default features; fmt clean. Commit a30e0f8.

#### T37.6 Honor `UserTurn.attachments` for images and files

Depends: — · Size: ~180 · Files: `crates/cox-core/src/context.rs`, `crates/cox-provider-anthropic/src/…`, `crates/cox-provider-openai/src/…` (G2)
Goal: an attached image or file reaches the model on every wire that supports it; an unsupported wire gets a clear notice.
Check: request snapshots for Anthropic and OpenAI Responses contain the image block; a Chat-only local model gets the notice.
Status: done 2026-09-28
Result: `cox-core/src/context.rs` `user_content` builds the user message from text, hook context and each `UserTurn.attachments` entry: png/jpeg/gif/webp → `Content::Image` when the model takes images; other UTF-8 files → a `<attachment name=… media_type=…>` text block; anything else is held back with one Warn notice. `Provider::accepts_images(model)` (default false) is true on Anthropic and OpenAI Responses, and on Chat only for a `models` entry with `images = true` (new `ProviderModel.images`, `Capabilities.images` in the catalog). Attachments ride on `ItemStarted` (`UserMessage.attachments`).

Deviations: more than 3 files (trait, catalog flag, config field and schema, three wires, `Priced`); new dependency `base64 0.23` in cox-core (already in Cargo.lock) to decode attached text files, listed in §1.1 and `toolchain.md`.

Check: `cargo nextest run -p cox-core -p cox-models -p cox-provider-anthropic -p cox-provider-openai -p cox-provider -p cox-protocol` 467 passed, 1 skipped — snapshots `anthropic_request_user_image`, `responses_request_user_image`; `chat_accepts_images_only_where_a_model_declares_them`; `image_on_a_text_only_wire_is_held_back_with_a_notice`; config schema drift test green; clippy and fmt clean. Commit 3b6ee4c.

Not done (follow-ups): PDFs and other binary files are held back (needs a document `Content` variant); resume rebuilds history without attachments; an external-agent child turn does not forward them.

#### T37.13 `[desktop.appearance]` config section

Depends: — · Size: ~100 · Files: `crates/cox-config/src/…`, `docs/config.jsonschema`
Goal: `[desktop.appearance]` — `material` (frosted | glossy | solid), `opacity`, `blur`, `depth`, `tint` with defaults from `desktop/design/tokens/base.json` (DS§3.5) — and `[desktop.transcript] cross_block_selection` (default `true`, A67), owned by `cox-config` like every other setting.
Check: the config-schema drift test passes; `cox config set desktop.appearance.material glossy` round-trips; an out-of-range value is rejected.
Status: done 2026-09-28
Result: `Config.desktop` (`DesktopConfig`, `cox-protocol/src/config.rs`) adds `[desktop.appearance]` — `material` (`frosted`|`glossy`|`solid`), `opacity` (default 0.42 = `material.frosted.windowOpacity`), `blur` (34 = `material.frosted.blur`, max 60 pt), `depth` (1.0: elevation tokens as designed, DS§3.4), `tint` (true) — and `[desktop.transcript] cross_block_selection` (true). Ranges (opacity and depth 0..=1, blur 0..=60, NaN rejected) are enforced while deserializing, so a bad value fails `load` with the usual `CoreError::Config { key, .. }`; the schema carries minimum/maximum. `default.toml`, `docs/config.jsonschema` and `docs/config.md` regenerated.

Deviations: the types live in `cox-protocol/src/config.rs` with every other section (cox-config owns loading and editing only); 5 files, two of them generated docs.

Check: `cargo nextest run -p cox-protocol -p cox-config` 84/84, including the schema and `config.md` drift tests, `config_set_desktop_material_round_trips` and `config_rejects_out_of_range_desktop_appearance`; `-p cox --test docs` 1/1; `-p cox --bin cox -E 'test(config) | test(doctor)'` 33/33; clippy and fmt clean. Real binary (`COX_HOME` scratch): `config set desktop.appearance.material glossy` → `config get` prints `glossy`, `show --sources` marks it `# user`; opacity 1.5 → exit 1, `1.5 is out of range 0..=1`. Re-checked after merging with T37.6's schema change: 84/84. Commit 97e9f77.

Not done: `cox config set` does not validate ranges before writing (no key does today); the error appears on the next load.

#### T37.35 Write transactions are IMMEDIATE; cross-process change feed

Depends: — · Size: ~150 · Files: `crates/cox-store/src/lib.rs`, `crates/cox-store/src/queries.rs`, `crates/cox-store/src/watch.rs`
Goal: every write that reads first runs in Diesel's `SqliteConnection::immediate_transaction`, so a concurrent commit makes it wait for `busy_timeout` instead of failing with `SQLITE_BUSY_SNAPSHOT`; a `Store::changes()` feed polls `PRAGMA data_version` (raw SQL: Diesel cannot model a PRAGMA; kept in `cox-store` with that comment) and reports "sessions/ledger changed by another process", which `cox-app` turns into sidebar and cost refreshes (T37.10).
Check: `concurrent_writers_never_fail_busy` — two processes each append 500 ledger rows and create sessions; all rows land, no busy error; `change_feed_sees_other_process_commit`.
Status: done 2026-09-28
Result: `cox-store` `write_tx` runs a body under Diesel's `SqliteConnection::immediate_transaction`; it wraps every read-then-write or multi-statement path (`memory_upsert`, `grant_put`, `kv_put` with its quota read, `finish_session_turn`'s ledger SUM + counter update, the open-time migration run). Single-statement writes stay in autocommit. `crates/cox-store/src/watch.rs` adds `Store::change_token()` and `Store::changes(&mut ChangeToken)`, polling `PRAGMA data_version` on the store's own connection — it moves only when another connection commits, so the store's own writes never report; pull-based, no thread.

Deviations: `Store::open` sets `busy_timeout` before `journal_mode = WAL` and retries the WAL switch on "database is locked" for up to 5 s (two processes opening a fresh file at once skip the busy handler); `queries.rs` has no write path and is unchanged; cross-process tests re-execute the test binary (`current_exe()` + an ignored `writer_process` test) instead of a test-only bin.

Check: `cargo nextest run -p cox-store` 21 passed, 1 skipped (child-only), 3 runs; `concurrent_writers_never_fail_busy` (2 processes × 500 rows + 5 sessions each, all land; fails with a deferred transaction); `change_feed_sees_other_process_commit`; clippy and fmt clean. Commit 640ef26.

#### T37.36 An older binary refuses a newer `cox.db`

Depends: — · Size: ~80 · Files: `crates/cox-store/src/lib.rs`, `crates/cox/src/doctor.rs`
Goal: on open, if `__diesel_schema_migrations` holds a version this binary does not embed (`MigrationHarness::applied_migrations`), `Store::open` fails with `StoreError::SchemaNewer { db, binary }` and the CLI says which `cox` is newer and how to update; `cox doctor` reports the mismatch. Covers the app's bundled `cox` next to a Homebrew `cox` of another version.
Check: `older_binary_refuses_newer_schema` (a test inserts a future migration version); doctor snapshot shows the mismatch line.
Status: done 2026-09-28
Result: `Store::open` runs `refuse_newer_schema` inside the same IMMEDIATE transaction before migrating: an applied version unknown to the embedded `MIGRATIONS` gives `StoreError::SchemaNewer { db, binary }`. The CLI message names both versions and says to update cox or run the newer one; `cox doctor` has its own `db` row for it whose fix keeps `cox.db` (the generic "remove cox.db" would throw away the newer cox's data).

Deviations: the variant lives in `cox-protocol/src/errors.rs`, so `docs/protocol.jsonschema` was regenerated (two files beyond the card). `doctor_human_output` unchanged; new snapshot `doctor_reports_a_newer_schema`.

Check: `older_binary_refuses_newer_schema`; nextest `-p cox-store -p cox-protocol` 91 passed (re-run after merge: 91 passed, 1 skipped); `-p cox` doctor tests 24; clippy and fmt clean. Real binary with a scratch `COX_HOME`: doctor ✓, then ✗ with the new line after a future migration row was inserted; `cox stats` printed the SchemaNewer error. Commit a6e839e.

#### T37.1 Extract `cox-session` from `crates/cox/src/session.rs`

Depends: — · Size: ~200 · Files: `crates/cox-session/src/lib.rs`, `crates/cox/src/session.rs`, `crates/cox/tests/deps.rs`
Goal: session assembly (provider, tools, MCP, skills, hooks, plugins, checkpointer, worktrees) is a library with no `Cli`, no `anyhow`, no `eprintln!`; warnings are returned as data (DT§4.2).
Check: `cargo nextest run --workspace` green; `deps.rs` asserts `cox-session` does not depend on `clap` or `anyhow`; the TUI and `run -p` e2e snapshots are unchanged.
Status: done 2026-09-28
Result: new library crate `crates/cox-session`: `open(SessionSpec) -> Result<Opened, SessionError>` takes a loaded `Config` and returns the session, the effective config and a typed `Vec<Warning>` (`Skill`, `Agent`, `Mcp`) — no clap, no anyhow, no printing. Modules `provider`, `tools`, `plugins`, `mcp`, `sandbox`, `lineage` (fork, handoff, resume) and `testing` (feature `test-util`); `external_agents.rs` and `write_grant` moved in unchanged. `crates/cox/src/session.rs` went from 3714 to 1119 lines (flag layer, TUI loop, `cox init`, `--worktree`, plugin dialogs, printing) and re-exports the old `session::` paths. The `plugins` feature is forwarded, so the slim build still drops the WASM runtime. `deps.rs` rule `session_has_no_cli_or_terminal`; AGENTS.md layout row.

Deviations: more than 3 files and ~300–500 lines of new code (spec, error and warning types, wrapper, headers) beyond the moved code; warnings now print after `open` returns, so after an MCP browser-login prompt rather than before, and not at all if `open` fails after discovery; `async-trait`, `tokio-util` and `agent-client-protocol` moved from `crates/cox` to `cox-session` with the external-agent code.

Check: clippy `-p cox-session -p cox --all-targets` clean (also `--no-default-features --features cox/otel`); `nextest -p cox-session` 33/33 incl. `open_returns_skill_warnings_as_data`; `-p cox --bin cox --test tui_e2e --test run_cli --test plain --test deps --test no_real_keychain_in_tests --test plugins --test external_agents_cursor` 112/112, no snapshot changed; `--test ide --test mcp_serve --test subagent_messaging` 6/6; real binary with a scratch `COX_HOME` and a broken skill printed `cox: warning: skill … skipped` and the scripted reply. After merging with T37.6/T37.13/T37.36: clippy clean, `cox-session` 33/33, `deps` + `run_cli` 23/23. The workspace-wide nextest is left to CI. Commit d26b6d0.

Not done: ~13 comments in other crates still name `crates/cox/src/session.rs`.

#### T37.11 Login-shell environment resolution in `cox-session`

Depends: T37.1 · Size: ~80 · Files: `crates/cox-session/src/env.rs`
Goal: an app launched from Finder sees the user's login-shell `PATH` and env, like a terminal launch.
Check: a test with a fake shell script returns its exported `PATH`; a timeout falls back to the process env with a warning.
Status: done 2026-09-28
Result: `crates/cox-session/src/env.rs`: `login_env(timeout)` runs `$SHELL` (or `/bin/zsh` on macOS, `/bin/sh` elsewhere) as `-l -i -c` with a fixed script printing `env -0` between two markers; `resolve(shell, timeout)` takes the shell path so tests pass a fake one. Returns `(Env, Option<Warning>)`, `Env` a sorted map of `OsString`s. On timeout the shell's process group gets SIGKILL. `parse(&[u8])` is pure and ignores rc-file noise around the markers. Any failure (spawn, timeout, no markers) falls back to the process env with the new `Warning::Env`. Library only; the CLI's startup is unchanged; the app wires it in through cox-ffi.

Deviations: `-l -i` as DT§4.8 says (PATH is often set in `.zshrc`, which only an interactive shell reads; stdin is empty so it cannot wait for input); `nix` (`signal`, workspace version) added to cox-session for the group kill, as `cox-ext` hooks do (§1.1 row). The timeout became 10 s instead of 3 s after merge: a real `zsh -l -i -c` took ~2.0 s on a loaded machine (DT§4.8 updated).

Check: `cargo nextest run -p cox-session` 38/38 (5 new: a fake shell's exported PATH comes back through junk output; a 30 s sleeper against a 200 ms timeout falls back with a "timed out" warning; a missing shell falls back; `parse` keeps multi-line values and ignores junk; `parse` returns nothing without markers); re-run after the timeout change 38/38; clippy and fmt clean. Commits 8cda296, 57783af.

#### T37.3 `ToolResult.structured`; TUI and ACP drop their todo re-parsers

Depends: — · Size: ~150 · Files: `crates/cox-protocol/src/lib.rs`, `crates/cox-tools/src/todo.rs`, `crates/cox-tui/src/…` (DT gap G3)
Goal: the todo list crosses the event stream as data, not text a surface re-parses.
Check: protocol schema regenerated; a todo scenario snapshot shows identical TUI output; no todo text parser remains (`rg` finds none).
Status: done 2026-09-28
Result: `ToolResult.structured: Option<Box<Value>>` (serde default, so old rollouts load); `cox-protocol` gains `TodoItem`, `TodoState` and `ToolResult::todo_list`. The core passes `ToolOutput.structured` through (`turn.rs`); `todo.rs` builds its payload from those types; the TUI panel (`state.rs`, `status.rs`) and the ACP plan (`map.rs`) read the list as data. `parse_todo` and the text-parsing `plan_from` are deleted.

Deviations: the payload is boxed (unboxed, it tripped `large_enum_variant` on the TUI enums); ~20 files that build a `ToolResult` (mostly tests) gained `structured: None`; `docs/compat.md` lost the T5.5 leftover row this resolves.

Check: `docs/protocol.jsonschema` regenerated; `todo_panel` screenshot and `/todo` status snapshots unchanged; no todo text parser left (grep); new `todo_plan_comes_from_the_structured_list_not_the_text` (ACP), an old-rollout load test and a `todo_list` test; nextest for protocol, tools, core, tui, acp green; clippy for those plus plugin and cox clean; fmt clean. Commit 8b6d281.

Not done: `cox-plugin/src/context.rs` still takes the todo list from the call input (not a text parser).

#### T37.5 `StateChanged` and `TitleSet` events; fix `protocol.md` counts

Depends: — · Size: ~120 · Files: `crates/cox-protocol/src/lib.rs`, `crates/cox-core/src/lib.rs`, `docs/protocol.md` (G5–G7)
Goal: effort and permission-mode changes and the session title arrive as typed events, not a `Notice`.
Check: a scenario that changes mode and effort emits both events; `docs/protocol.md` counts match the enum.
Status: done 2026-09-28
Result: `Event::StateChanged { mode, effort }` replaces the notice text `SetEffort` and `SetPermissionMode` sent; the TUI takes mode and effort from it. `Event::TitleSet { title }` added. `docs/design/protocol.md` counts fixed (after T37.4: 26 `Event`, 16 `Submission` variants).

Deviations: the change is in `cox-core/src/session.rs`, not `lib.rs`; the card's `docs/protocol.md` is `docs/design/protocol.md`.

Check: new `set_mode_and_effort_each_emit_state_changed_with_both_values` (router.rs), `router_set_effort…` updated, a TUI test for `StateChanged`; schema regenerated; 580 tests in protocol, core and tui pass; clippy clean. Commit 287daaa.

Not done: nothing emits `TitleSet` yet — generating a title (a low-cost `Job::Title` call after the first turn, and a store column for it) adds one model request per session and changes every scripted scenario and cost; when it runs and whether it is opt-in is a creator decision (see `ideas.md`).

#### T37.4 `QuestionAsked` / `Answer` replace the `ask_user` side channel

Depends: — · Size: ~180 · Files: `crates/cox-protocol/src/lib.rs`, `crates/cox-core/src/turn.rs`, `crates/cox-tools/src/ask_user.rs` (G4)
Goal: a question to the user is an `Event` and its answer a `Submission`, so every surface — and a replay — sees it.
Check: a scripted scenario asks and answers a question headless; the rollout contains both.
Status: done 2026-09-28
Result: a question is `Event::QuestionAsked { call_id, question, options, source }`, the reply `Submission::Answer { call_id, text: Option<String> }`. `ask_user` asks through the session via a new `Relay::ask` (default: deny): the session parks the call, emits the event and waits. A subagent's question is raised on the parent's stream and the answer passed back to the child. The TUI modal and `--plain` work from the event and answer with a submission; `cox-session`'s `questions` setting is a plain flag; the old channel plumbing is gone from `crates/cox`, the TUI main loop and `kitty_probe`.

Deviations: no separate question id (the call id is unique, as for approvals); `text` optional so Esc or end of input dismisses; 13 source files, +366/−271 including in-file tests (~95 net lines of code), because the old channel ran through every surface.

Check: new `crates/cox-core/tests/question.rs` — a scripted scenario asks and answers headless and the rollout holds the question and answered result; a subagent's question is answered through the parent (fails without the relay); a stray answer gives a warning notice. 733 tests across protocol, tools, core, session and tui; `-p cox` 139; clippy and fmt clean. Real binary `--plain` with a scripted scenario: question shown, "2" answered, `prod` returned, `question_asked` in the rollout. After merge with T37.11: clippy `-p cox-session -p cox` clean, `cox-session` + `cox-protocol` 117/117. Commit 69a63d5.

Not done: headless `--answer` still answers inside the tool (no `QuestionAsked` there); the `Notification` hook fires on the `ask_user` call, not on `QuestionAsked`.

#### T37.2 Route `cox acp` through `cox-session`

Depends: T37.1 · Size: ~80 · Files: `crates/cox-acp/src/lib.rs`, `crates/cox/src/main.rs`
Goal: ACP sessions get the same tools, MCP servers, hooks and plugins as the TUI.
Check: an ACP e2e against the scripted provider lists the same tool names as `cox run -p` for the same `COX_HOME`.
Status: done 2026-09-28
Result: `AcpFactory::create` (`crates/cox/src/acp_cmd.rs`) opens sessions through `cox_session::open`, so ACP sessions get the same MCP servers, skills, subagent definitions, hooks, plugins, checkpointer and worktrees as the TUI. `SessionSpec.client: Option<ClientTools>` (link, fs, terminal) swaps in the client-backed `read`/`edit`/`write`/`bash`. `open`'s warnings reach the ACP client as `Notice` events, never stdout. `cox_acp::SessionFactory::create` is async (async-trait). The scripted provider gained a turn field `echo_tools = true` that replies with the request's sorted tool names. Also: 12 comments in other crates now point at `crates/cox-session` (commit 533595e).

Deviations: the factory lives in `crates/cox/src/acp_cmd.rs`, not `cox-acp/src/lib.rs`; 10 files, ~200 changed LOC with the test; `async-trait` is a new edge for `crates/cox` (already a workspace dependency); ACP's default workspace root is now the git root of `cwd` (else `cwd`), as on the other surfaces, with the client's extra roots appended. Merged after T37.4 and T37.34: `questions: false`, `surface: "acp"`.

Check: e2e `acp_session_offers_the_same_tools_as_run_p` (`crates/cox/tests/ide.rs`) — one MCP server (`cox mcp`, `mcp.deferred = false`) in `COX_HOME`; `cox run -p` and a JSON-RPC `cox acp` run list identical tools including `mcp__self__read`. clippy `-p cox-acp -p cox-session -p cox-provider -p cox-provider-testkit -p cox` clean; nextest of those four 83/83; `-p cox --test ide --test run_cli --test deps` 25/25; fmt clean. After the merge: `-p cox-session -p cox-store -p cox-acp` 79 passed, 2 skipped; `-p cox --test ide --test tui_e2e --test run_cli --test plain --test deps` 33/33. Commit a131f2d.

Not done: ACP `session/new` still ignores the client's own `mcpServers` list.

#### T37.34 One process drives a session: session lock, read-only follow, fork

Depends: T37.1 · Size: ~180 · Files: `crates/cox-store/src/lock.rs`, `crates/cox-session/src/lib.rs`, `crates/cox-store/src/lib.rs`
Goal: the process that runs a session holds an OS advisory lock on `sessions/<id>.lock` (`std::fs::File::try_lock`, stable since Rust 1.89, no new dependency); the kernel drops it when the process exits or crashes, so no lease goes stale. A second process — another TUI, the app, `cox resume` — that opens the same session gets a typed `SessionBusy { holder }` and may follow it read-only (tail the rollout and fold it, the D2 replay path), fork it into a new session, or ask to take it over. Never two writers on one rollout (A67, DT§11 Q6).
Check: `second_opener_gets_session_busy` and `lock_released_when_holder_exits` (child process holds then exits); a follow test sees events the holder appends; the TUI e2e prints the busy notice instead of resuming.
Status: done 2026-09-28
Result: `crates/cox-store/src/lock.rs`: `lock::claim` takes an OS advisory lock (`File::try_lock`) on `sessions/<id>.lock` next to the rollout and writes `Holder { pid, surface, since }` as JSON; a held lock returns that holder (unreadable → "another cox process"). Claims inside one process share the lock through a static registry, so a failed `/fork` that falls back to its parent is not refused by its own lock. `Store::claim_session` keeps the lock in the `Store`, which the `Session` holds through an `Arc`; the kernel drops it on exit or crash. `cox_session::open` claims the id (new or resumed) before building anything; a held lock → `SessionError::SessionBusy { id, holder }`. `SessionSpec.surface` (`tui`, `plain`, `headless`, `acp`). `cox_session::Follow` (`lineage.rs`) re-reads the holder's rollout and returns only new events; fork is the existing `lineage::fork`. AGENTS.md `cox-store` row and DT§4.5 "Sessions open elsewhere" updated.

Deviations: five source files (`lineage.rs` holds `Follow`; `crates/cox/src/session.rs` passes the surface); ~180 lines of non-test code; no new dependency. The lock file stays on disk after release on purpose (deleting it could race another opener).

Check: `cargo nextest run -p cox-store -p cox-session` 59 passed, 2 skipped (child-process helpers) — `second_opener_gets_session_busy`, `lock_released_when_holder_exits` (child exits via `process::exit` without dropping the lock), `claims_in_one_process_share_the_lock`, `follow_sees_events_the_holder_appends`; `-p cox --test tui_e2e` 6 incl. `tui_resume_of_a_driven_session_prints_busy_notice`; `--test run_cli --test plain` 19; `--bin cox` 75; clippy and fmt clean. Real binary (scratch `COX_HOME`): lock file written, `run --resume` after exit works; with the lock held from outside (`flock`), `run --resume` exits 1 naming the holder and the follow/fork options. Re-checked after merging with T37.2 and T37.4 (see T37.2). Commit 33133b8.

Not done: "take over" is only notice text; no CLI/TUI command yet to follow or fork a busy session (`Follow` and `fork` are library functions for the app); on Windows the holder cannot be read while locked, so the notice says "another cox process".

#### T37.8 `cox-app`: timeline fold with snapshots per scripted scenario

Depends: T37.3–T37.7 · Size: ~200 · Files: `crates/cox-app/src/timeline.rs`, `crates/cox-app/src/patch.rs`, `crates/cox-app/src/lib.rs`
Goal: `Event` → keyed blocks → `TimelinePatch` (DT§4.3); replaying a rollout produces the same patches as the live run.
Check: `insta` snapshot per scripted scenario; `replay_equals_live` holds for all of them.
Status: done 2026-09-28
Result: new pure crate `crates/cox-app` (depends on `cox-protocol` and `cox-render` without default features). `patch.rs`: `Block`, `BlockId`, `BlockKind` (User, Assistant, Thinking, Tool, Approval, Question, Task, Compaction, Checkpoint, Notice, Error, TurnMeta) and the serde `TimelinePatch` (`Reset`, `Upsert { block, after }`, `AppendText`, `DocTail { id, from, blocks }`, `Remove`). `timeline.rs`: `Timeline::apply(&Event) -> Vec<TimelinePatch>` and `reset()`; keys come from the events' own ids (`item:`/`call:`/`approval:`/`question:`/`turn:`/`task:`), id-less notices, errors and checkpoints are keyed by event position — both stable on replay. Assistant text streams as `StyledDoc`: each delta re-parses the reply and `DocTail` sends only the blocks from the first changed one; `ItemDone` re-sends the whole reply with its source. `StyledDoc` and its types gained serde derives in `cox-render`. `deps.rs` rule `app_has_no_terminal_or_cli` (`cox_tree` generalised to `tree(package, …)`); AGENTS.md row.

Deviations: ~450 non-test lines against ~200 (mostly rustfmt-expanded code for 12 block kinds); files outside the card: `cox-render/src/doc.rs` and `Cargo.toml` (serde), `deps.rs`, `AGENTS.md`, `Cargo.lock`. No new external crate; dev-deps cox-core, cox-provider, tokio, async-trait, serde_json, insta.

Check: `tests/scenarios.rs` runs 10 cox-core scripted scenarios live (text_only, one_tool, three_parallel, big_tool_output, provider_error, max_turns, interrupt, ask_then_approve, ask_then_deny, allow_for_session) with one insta snapshot each (one JSON line per patch, ULIDs numbered, timings zeroed) and `replay_equals_live` over the JSONL round-trip of the rollout; 3 unit tests (DocTail freezing, tool tail, rewind `Remove`). `cargo nextest run -p cox-app -p cox-render` 51/51 (snapshots stable over 3 more runs with `INSTA_UPDATE=no`); `-p cox --test deps` 7/7; clippy and fmt clean. Re-run after merge: same. Commit ca4a0f2.

Not done: DT§4.3 Rust-made tool summaries ("Ran `x` — exit 0 · 4.2 s") and grouping consecutive read/grep/glob/outline calls into "Explored N files" (in `roadmap.md`); the `Status` patch is T37.10's; Compaction block has no summary text; attachments carried by name only; the whole reply is re-parsed on each delta; subagent, checkpoint and rewind scenarios not covered.

#### T37.9 `cox-app`: drain task, coalescer, never-stall

Depends: T37.8 · Size: ~150 · Files: `crates/cox-app/src/controller.rs`, `crates/cox-app/src/patch.rs`
Goal: the core never blocks on a slow UI: events drain into the fold continuously and patches coalesce while the consumer is behind.
Check: `slow_consumer_never_stalls_the_core` — a consumer that sleeps 2 s per pull still sees the turn finish on time and a coalesced final state.
Status: done 2026-09-28
Result: `crates/cox-app/src/controller.rs`: `Controller::spawn(Timeline, mpsc::Receiver<Event>)` starts a tokio task that drains the core's event channel continuously, folds each event and queues the patches. `next_patches()` is an async pull (`Option<Vec<TimelinePatch>>`) that waits on a `Notify`, returns at most one batch per 16 ms frame unless 64 patches are queued, and `None` once the stream is closed and drained; `snapshot()` returns the whole block list and drops the queue; `close()`/`Drop` stop the drain, not the session. `coalesce.rs`: `push` merges patches for the same block so the queue is bounded by live blocks, not events; `apply` is a reference consumer the tests prove coalescing against. AGENTS.md `cox-app` row updated.

Deviations: ~285 non-test lines against ~150; the coalescer is its own `coalesce.rs` (not `patch.rs`) to keep merges with T37.38 easy; tokio is a regular dependency of cox-app (already in the workspace; dev-dep gains `test-util`); the scenario-file read in `tests/scenarios.rs` became a shared `scenario()` helper.

Check: `slow_consumer_never_stalls_the_core` (paused time) over the six scenarios that need no person plus an inline 60-round "flood" (>256 events, the core channel's capacity): a consumer sleeping 2 s per pull still sees the turn finish in under 2 s, no batch exceeds the block count, and the coalesced final state equals applying every uncoalesced patch. Deliberate breakages caught: a drain that waited on the consumer took 8 s; turning coalescing off broke the queue bound. 5 coalescing unit tests + 1 controller test. `nextest -p cox-app` 12/12 (re-run after merge 12/12), `-p cox --test deps` 7/7, clippy and fmt clean. Commit 019733c.

Not done: approval and interrupt scenarios are not in the slow-consumer test (the consumer sees patches, not events); the 64-patch early return is checked only when a pull starts.

#### T37.38 `cox-app`: tool summaries, `ToolGroup`, compaction summary

Depends: T37.8 · Size: ~150 · Files: `crates/cox-app/src/summary.rs`, `crates/cox-app/src/timeline.rs`, `crates/cox-app/src/patch.rs`
Goal: the DT§4.3 rows T37.8 left out. Rust writes each tool's one-line summary ("Ran `cargo test` — exit 0 · 4.2 s", "Edited `crates/x.rs` +12 −3", "Read `a.rs` · 120 lines") with its icon key and duration, so Swift never parses tool output; consecutive read/grep/glob/outline calls fold into one `ToolGroup` block ("Explored 7 files") with the calls as children; the `Compaction` block carries before → after tokens, reason and the summary text. Also covers the scenarios T37.8 skipped: subagent, checkpoint and rewind.
Check: the T37.8 scenario snapshots updated in one reviewed change; new scenarios for subagent, checkpoint/rewind and a read-grep-read run that folds into one group; `replay_equals_live` holds for all of them.
Status: done 2026-09-28
Result: `crates/cox-app/src/summary.rs` (pure) writes each tool's one-line summary from the call's input and the result's `structured` data — running calls in the present ("Reading `a.rs`"), finished ones in the past: bash "Ran `cmd` — exit N · 4.2 s", edit/apply_patch "Edited `p` +A −D", write "Wrote `p` · N lines", read "Read `p` · N lines" / "· lines 3-5", grep/glob "Searched `pat` · N matches / files", web_fetch, todo ("x/y done"), ask_user, agent ("Delegated to `explore`: task"), MCP ("Called `server:tool`"), a generic fallback — plus an `Icon` key and `explore()`. `Tool` blocks carry `icon`; the summary is re-sent when the call finishes. New `ToolGroup { summary, children, state }` ("Explored 2 files, 1 search"), keyed `group:<first call id>`, inserted in front of its first call once a second read/grep/glob/outline call arrives; children stay flat `Tool` blocks; any other block ends the group. `Compaction.summary` carries the `Summary` item emitted just before `Compacted` (`None` when absent). AGENTS.md `cox-app` row updated.

Deviations: ~370 non-test lines against ~150. `read`, `grep` and `glob` in cox-tools now return counts in `structured` (`lines`, `matches`, `files`); `diff::counts` moved to an ungated `cox_render::diffstat` (re-exported at its old path); `serde_json` is a normal dependency of cox-app. Bug fix in the T37.8 fold: `Rewound { to_turn }` also removes `to_turn`'s own blocks, as `Submission::Rewind` specifies.

Check: 8 existing snapshots regenerated in one pass (summary and icon fields only; text_only and provider_error unchanged); new snapshots subagent_explore, explore (read, grep, read → one group of 3) and checkpoint_rewind (fake checkpointer, second turn removed); `replay_equals_live` for all 13 scenarios; `cargo nextest run -p cox-tools` 111 passed, 1 skipped; `cargo check -p cox-tui` ok; clippy `-p cox-app -p cox-render -p cox-tools` clean. After merging T37.9 (shared `scenario()` helper, `slow_consumer_never_stalls_the_core` now also over subagent_explore and explore): `INSTA_UPDATE=no nextest -p cox-app -p cox-render` 63/63, clippy and fmt clean. Commits f5d1793, f171b56.

Not done: `Approval` blocks still use the subject's first line as their summary.

#### T37.10 `cox-app`: workspace, inbox, status, intents, completion

Depends: T37.1, T37.8, T37.35 · Size: ~200 · Files: `crates/cox-app/src/workspace.rs`, `crates/cox-app/src/inbox.rs`, `crates/cox-app/src/intent.rs`
Goal: projects, sessions, search, worktrees with disk size, the "needs you" inbox, and one intent enum the app sends (DT§4.3).
Check: unit tests per intent against a scratch `COX_HOME`; inbox ordering test.
Status: done 2026-09-28
Result: in `crates/cox-app`: `workspace.rs` — `projects()` groups sessions by git root (`cox_config::load::find_git_root`, as the TUI's `/resume`), `sessions(project)` with title, updated time, cost and `held_by` (the T37.34 lock holder), `search()` over the existing FTS5 `rollout_search`, `worktrees()` through a new `Worktrees` trait; `inbox.rs` — one fold over every session's events collecting approvals, questions, failed turns and finished background tasks by urgency then arrival, per-session `Activity` (idle, running, waiting on you, failed), `badge()`, `expire()`, `dismiss()`; `intent.rs` — `Intent` with the 16 DT§4.3 variants and `dispatch()` to a `Submission` (sent, spawned as a turn, or queued) or a fork/handoff for the controller; `complete.rs` — `Completer` for `/` commands and `@` files over the TUI's sources and cox-search's nucleo ranking. Shared instead of duplicated: the built-in command table moved to `cox-protocol/src/commands.rs` (cox-tui re-exports it); `Worktrees::list`/`WorktreeInfo` in `cox-protocol` implemented in `cox-tools/src/git.rs` with one porcelain parser shared with `worktree_remove`; `dir_size` moved from `crates/cox/src/doctor.rs` to cox-tools, off the async runtime; `cox_store::lock::holder` and `Store::session_holder` read a lock holder without claiming.

Deviations: cox-app does not depend on `cox-session` — it pulls `anyhow` transitively (tiktoken-rs, agent-client-protocol), which `app_has_no_terminal_or_cli` bans over the resolved tree — so `dispatch()` returns `Fork`/`Handoff` for the controller's owner (cox-ffi) to run; cox-session is a dev-dependency only. cox-app depends on `cox-ext` directly (not listed in DT§4.2). The inbox also holds failed turns and finished tasks, ranked below approvals and questions. `Command { line }` sends `/name args` as `Submission::Command` and `!`/`!!` as shell lines, without the TUI's full parser. ~580 non-test lines in 4 files plus ~120 in other crates.

Check: nextest over cox-app, `lock::` and the git worktree tests 29 passed — 7 intent tests against a real `cox.db` in a scratch `COX_HOME` (one per intent group plus a table test over all 16), 3 inbox tests including ordering, 1 completion test, `holder_names_another_process_without_claiming`, `worktree_list_reports_size_merge_and_lock`; `-p cox --test deps` 7/7; cox-tui command, help, palette and status tests 47; clippy on cox-app, cox-store, cox-tools, cox-protocol, cox-tui, cox, cox-core clean; fmt clean. After merging with T37.9/T37.38: clippy `-p cox-app -p cox-tui -p cox-tools -p cox-store -p cox` clean, `INSTA_UPDATE=no nextest -p cox-app` 28/28, `deps` 7/7. Commit e8c03c3.

Not done: no live test for Answer, Background, Compact, Queue or SwitchModel (the harness has no ask_user or bash tool; the table test covers their mapping).

#### T37.12 `cox-app`: usage and throughput view state

Depends: T37.8 · Size: ~150 · Files: `crates/cox-app/src/usage.rs`, `crates/cox-app/src/timeline.rs`
Goal: the token meter's data (DS§7): sent and received per turn and per session from the ledger, live tok/s estimated from output deltas over a rolling window and replaced by the exact figure when usage arrives, time to first token, and the context breakdown the TUI already shows (P28).
Check: a scripted stream with known timings yields the expected tok/s within 5 %; per-turn and session totals equal the ledger rows.
Status: done 2026-09-28
Result: `crates/cox-app/src/usage.rs` `Meter` folds an `Event` plus its arrival time into a `UsageView`: session and per-turn totals (sent, received, cache read, cache write, uncached, cost, calls, estimated), the last call's `context_tokens` (the TUI's `ctx`), and a `TurnUsage` with TTFT, tok/s, an `exact` flag, a 32-point sparkline and a thinking-token estimate. Totals are summed only from `Event::Usage`, which the core emits right after writing the same `Usage` to the ledger — no second set of books. Live tok/s is measured over a 2 s rolling window of text and thinking deltas and replaced by the exact rate when the call's usage arrives (output tokens over first delta → usage, or the ledger's `latency_ms` when that span is unknown). The clock is a parameter (the controller passes tokio's elapsed time), so the fold stays pure. The view reaches the UI as `TimelinePatch::Usage`; the coalescer keeps only the latest; `TurnMeta` uses the meter's `add_to` instead of its own sum. DT§4.3 patch list updated.

Deviations: seven files (patch.rs, coalesce.rs, controller.rs, lib.rs, desktop.md besides usage.rs and timeline.rs), ~210 non-test lines in usage.rs; the live estimate is bytes/4, not `cox_tokens::estimate` (it sizes a whole `Request` and would pull tiktoken-rs and reqwest into the app core); the per-segment context breakdown (P28 `cox_core::context::breakdown`) is not delivered — it is dead code and no event carries it; the meter carries `context_tokens` only.

Check: `live_rate_of_a_steady_stream_is_within_five_percent` (100 tok/s scripted stream, every sample within 5 %, TTFT 300 ms); `usage_replaces_the_estimate_with_the_exact_rate_and_sums_the_ledger`; `meter_totals_equal_the_ledger_rows` (two turns over a MemoryStore session, per-turn and session totals equal the ledger rows); `slow_consumer_never_stalls_the_core` also checks the last pulled usage patch equals the ledger across every scenario including flood. `nextest -p cox-app` 22/22; after merging with T37.10: 33/33, clippy and fmt clean. Commit a480491.

Not done: session totals cover this session's ledger rows only (a subagent's cost shows on its Task block); only the current or last turn keeps tok/s and TTFT, past turns keep totals in `TurnMeta`.

#### T37.14 `cox-ffi`: UniFFI exports, runtime, `Host`; fixture recorder

Depends: T37.9, T37.10, T37.34, T37.36 · Size: ~200 · Files: `crates/cox-ffi/src/lib.rs`, `crates/cox-ffi/src/session.rs`, `crates/cox-ffi/src/host.rs`
Goal: one tokio runtime; async `next_patches()`; foreign `Host` trait for notifications and secrets; a recorder that writes patch streams for Swift fixtures. New dependency `uniffi` (§1.1 row), only in `cox-ffi`.
Check: generated Swift bindings compile in a scratch package; `deps.rs` asserts only `cox-ffi` depends on `uniffi`.
Status: done 2026-09-28
Result: new crate `crates/cox-ffi` (`staticlib` + `lib`) on uniffi 0.32.2 with proc-macros, no UDL. One tokio runtime; async exports run on it and are aborted when Swift cancels the Task. `App`: workspace, inbox, badge and activity, open and resume. `SessionHandle`: snapshot, `next_patches`, `send` (runs `dispatch` including Queue, Fork and Handoff — the latter two return the child session), `complete`, `close`/`end`. `load_login_env`; a foreign `AppHost` trait for notify, open_url and secret. cox-app types cross as `#[uniffi::remote]` declarations (including `TimelinePatch::Usage` and `UsageView`), so there is no copying code and upstream drift breaks the build. `cox_session::open_with_keys`: the host's Keychain supplies the provider key, the env var still wins; `open()` unchanged. Recorder example writes `desktop/macos/Fixtures/read-and-reply.json` (README says how to regenerate); a `uniffi-bindgen` bin behind the `bindgen` feature. AGENTS.md and `docs/how-it-works.md` updated.

Deviations: the trait is `AppHost`, not `Host` (Foundation has a `Host` class); cox-ffi depends on cox-session, core, config, store, render and tools besides app and protocol (DT§4.2 allows only app and protocol) — see T37.39; size host 33, lib 263, session 203, types 591 lines, so the D11 300-line surface limit is exceeded (types.rs is one remote declaration per type; lib + session + host = 499); own runtime instead of uniffi's `tokio` feature; fixtures are the serde JSON of the cox-app types.

Check: the generated Swift built as a Swift 6 package on Swift 6.4 linked to the debug staticlib, `swift run` worked (only linker deployment-target warnings); `cargo nextest run -p cox-ffi` 4/4; `-p cox --test deps` 8/8 incl. `only_ffi_depends_on_uniffi`; clippy `-p cox-ffi -p cox-session` clean; fmt clean. Re-run after merge: same. Commit 3d33d78.

Not done: following a busy session read-only (returns `AppError::Busy`); worktree sessions; `store_key`/settings; the Claude-settings layer; the host key covers only the main provider (LM Studio and MCP tokens still use the keyring); no Swift test decodes the fixture JSON yet (T37.16; uniffi's `generate_codable_conformance` may help).

#### T37.39 Thin `cox-ffi`: session ownership moves into `cox-app`

Depends: T37.14 · Size: ~200 (mostly moved) · Files: `crates/cox-app/src/app.rs`, `crates/cox-ffi/src/lib.rs`, `crates/cox/tests/deps.rs`
Goal: `cox-ffi` back inside DT§4.2 and D11. T37.14 had to open, resume, fork and hand off sessions in `cox-ffi` because `cox-app` could not depend on `cox-session`: `deps.rs` `app_has_no_terminal_or_cli` bans `anyhow` over the whole resolved tree, while D1 only says `cox-app` does not *depend on* `anyhow` or `clap`, and `cox-session` pulls `anyhow` transitively through tiktoken-rs and agent-client-protocol. The rule checks direct dependencies for `anyhow`/`clap` and the resolved tree for ratatui/crossterm/cox-tui; `cox-app` gains an `App`/`SessionOwner` that owns sessions, runs `dispatch()`'s Fork/Handoff, takes host-supplied keys (`cox_session::open_with_keys`) and resolves the login env; `cox-ffi` depends only on `cox-app`, `cox-protocol` and `uniffi` and only forwards.
Check: `deps.rs` asserts `cox-ffi`'s direct workspace dependencies are exactly `cox-app` and `cox-protocol`; the moved behaviour is tested in `cox-app` against a scratch `COX_HOME`; `cox-ffi` exports unchanged (the generated Swift still compiles); `cox-ffi`'s `lib.rs` + `session.rs` + `host.rs` ≤ 300 lines.
Status: done 2026-09-28
Result: session ownership lives in `cox-app`: `app::App` (`crates/cox-app/src/app.rs`) holds the workspace, the cross-session inbox, a plain-Rust `Host` trait (`notify`, `open_url`, `secret`), `open` (new or resumed, via `cox_session::open_with_keys`), `load_login_env` and a structured `AppError` that keeps `Busy`; `live::LiveSession` (`live.rs`) holds the inbox tee, the `Controller` and `send`, which runs `dispatch` (queue, fork, handoff). `cox-ffi` only forwards: the runtime, `on_runtime`, the UniFFI exports and `AppHost`, adapted by a `Bridge` to `cox_app::app::Host`. `deps.rs`: `app_has_no_terminal_or_cli` bans ratatui/crossterm/cox-tui over the resolved tree and `clap`/`anyhow` on cox-app's direct dependencies only; new `ffi_depends_only_on_app_and_protocol`. AGENTS.md rows and DT§4.2 updated.

Deviations: the new code is split into `app.rs` and `live.rs` (card names `app.rs`). `cox-app` gains workspace deps cox-session, cox-core, cox-tools (no new external dep). `cox-ffi/fixtures/write-approved.toml` removed; its approval test moved to `cox-app` with the scenario inline. `cox-ffi` lib+session+host: 499 → 299 lines; `types.rs` (591, remote declarations) not counted — open question for the creator.

Check: Swift bindings regenerated in library mode before and after — `diff -r` byte-identical; `swiftc -emit-object` compiles them. `cargo nextest run -p cox-app -p cox-ffi` 41/41 (6 new in `cox-app/tests/app.rs`); `cargo nextest run -p cox --test deps` 9/9; clippy `-p cox-app -p cox-ffi` clean; fmt clean. Re-run after the merge into `p37-desktop`: same counts. Commit 9070d97.

Not done: no real-binary run (the `cox` binary is unchanged; the tests run real sessions against a tempdir `COX_HOME`).

#### T37.15 XCFramework script, `just` recipes, macOS CI job

Depends: T37.14 · Size: ~120 · Files: `scripts/desktop/xcframework.sh`, `justfile`, `.github/workflows/ci.yml`
Goal: `just desktop-xcframework` builds `CoxFFI.xcframework` for `aarch64-apple-darwin` only with `MACOSX_DEPLOYMENT_TARGET=26.0` (A67: no Intel, no universal slice); CI builds it and runs the Swift tests on an Apple Silicon macOS runner.
Check: the recipe exits 0 on a clean checkout; `lipo -archs` on the library prints `arm64` only; the CI job is green.
Status: done 2026-09-28
Result: `scripts/desktop/xcframework.sh` builds the `cox-ffi` static library for `aarch64-apple-darwin` only (`--profile dist`, `MACOSX_DEPLOYMENT_TARGET=26.0`), generates the Swift bindings with library-mode `uniffi-bindgen` (built separately, so its code never lands in the app's library) and runs `xcodebuild -create-xcframework` into `desktop/macos/build/CoxFFI.xcframework` (header + `module.modulemap`) and `desktop/macos/build/bindings/cox_ffi.swift`. `just desktop-xcframework` runs it. `ci.yml` job `desktop-macos` on `macos-26` (GA arm64, Xcode 26.x, macOS 26 SDK; runner-images `macos-26-arm64-Readme.md`, image 20260907.0351.1, checked 2026-09-28) runs the script, fails unless `lipo -archs` prints only `arm64`, then `swift test` for each `desktop/macos/Packages/*/Package.swift`.

Deviations: `.gitignore` gains `desktop/macos/build/` (a fourth file). No split debug info: the dist profile has none. `revert-on-failure` does not wait on the new job. CI calls the script directly (no `just` on the runner, as in `footprint`). Bindings go to `build/bindings/`; T37.16 decides their home in `CoxCore`.

Check: `CARGO_BUILD_JOBS=4 just desktop-xcframework` exit 0 (staticlib 4m44s, bindgen 4m49s); `lipo -archs` → `arm64`; `otool` minimum OS 26.0 on our objects (`compiler_builtins` and a few prebuilt `std` objects say 11.0, which links fine); `actionlint` 1.7.12 with shellcheck clean; `shellcheck` 0.11.0 clean. Commit ca495b5. The CI job first runs on the PR.

Not done: the static library is 393 MB (1250 members, fat LTO) — revisit when the app is packaged.

#### T37.17 Token pipeline: DTCG → `Tokens.swift`, `Colors.xcassets`, `tokens.css`

Depends: T37.0, T37.15 · Size: ~120 · Files: `desktop/design/style-dictionary.config.mjs`, `desktop/design/package.json`, `justfile`
Goal: Style Dictionary generates the Swift tokens, asset-catalog colorsets (Any, Dark, High Contrast) and the mockups' CSS from `desktop/design/tokens/` (DS§2). New dependency `style-dictionary` (§1.1 row).
Check: `just desktop-tokens` regenerates with no diff (drift test in CI); a generated colorset has a dark variant; `build_tokens.py` is deleted.
Status: done 2026-09-28
Result: Style Dictionary 5.5.5 (`desktop/design/style-dictionary.config.mjs`, pinned in `desktop/design/package.json` with a lockfile; custom Swift and CSS formats and a colorset action, since the built-in ones do not handle DTCG composites or asset catalogs) builds `desktop/design/tokens/*.json` into `desktop/macos/Packages/CoxUI/Sources/CoxUI/Tokens/Tokens.swift` (`Space`, `Radius`, `Size`, `Motion`, `MaterialToken`, `FontToken`, `ElevationToken`), `Tokens/Colors.xcassets` (56 colorsets, Any + Dark) and `desktop/design/tokens/tokens.css` (`:root` light, `.dark`). `just desktop-tokens` runs it; CI job `desktop-tokens` (ubuntu-24.04, node only) re-runs it and fails on a diff. `build_tokens.py` is deleted; DS§2 describes the new pipeline. node 24.21.0 (latest LTS) is pinned in `mise.toml`.

Deviations: more than three files (`.gitignore`, `ci.yml`, `mise.toml`, `toolchain.md`, `DESIGN.md`, token JSON). Tokens with child tokens (`accent`/`accent.soft`, `font.transcript`/`transcript.h3`), whose children Style Dictionary drops, became DTCG 2025.10 `$root` groups (§6.2) with the same values. Swift type names `FontToken`, `MaterialToken`, `ElevationToken` avoid shadowing SwiftUI's `Font`, `Material` and the planned `Elevation` modifier. Weight 650 maps to `.semibold` (DS§3.2).

Check: `just desktop-tokens` before and after the commit — no diff, no untracked files; an edited colour shows the diff; `surfaceWindow.colorset` has a `"luminosity": "dark"` entry; `xcrun actool` compiles the catalog (112 renditions); `swiftc -typecheck -swift-version 6 Tokens.swift` ok; a temporary `color.dark-hc.json` adds a contrast entry and an unknown mode file fails the build; `actionlint` clean. Commit addef79.

Not done: no High Contrast values yet — the pipeline adds them when `color.light-hc.json`/`color.dark-hc.json` exist, but the colours are a design choice for the creator. `mockups.html` still has its own inline variables (outside the card's files). `letterSpacing` is stored as `rem` but means em; it is emitted as em tracking.

#### T37.18 SwiftLint with the no-literal rules

Depends: T37.15 · Size: ~60 · Files: `desktop/macos/.swiftlint.yml`, `.github/workflows/ci.yml`
Goal: custom rules reject literal colours, sizes, fonts, radii, shadows and durations outside `Tokens/` and `Foundations/` (DS§9); SwiftLint runs through the SwiftLintPlugins build-tool plugin and formatting through the toolchain's own `swift-format` (R§9.5.4–9.5.5). New tool SwiftLint (§1.1 row).
Check: a fixture view with `.padding(12)` fails lint; the same view with `Space.l` passes.
Status: done 2026-09-28
Result: `desktop/macos/.swiftlint.yml` keeps SwiftLint's defaults and adds six error-severity custom rules — `no_literal_colour`, `no_literal_size`, `no_literal_font`, `no_literal_radius`, `no_literal_shadow`, `no_literal_duration` — exempt under `/(Tokens|Foundations)/` (literal `0`, comments and strings allowed); `trailing_comma` off (conflicts with swift-format); `identifier_name` allows the token step names. Its comments say how a package attaches SwiftLintPlugins 0.65.1 (`SwiftLintBuildToolPlugin` per target, plus a one-line `Packages/<Name>/.swiftlint.yml` with `parent_config: ../../.swiftlint.yml`, since the plugin reads config only inside the package). `desktop/macos/LintFixtures/` (`Accepted/`, `Rejected/<rule>.swift`, README; in no package). CI job `desktop-macos-lint` (macos-26, SwiftLint via `jdx/mise-action`): `Accepted` passes `--strict`, each `Rejected` file fails with its own rule, then `xcrun swift-format lint --strict --recursive` over `LintFixtures` and `Packages`. SwiftLint 0.65.1 (latest, 2026-08-21) pinned in `mise.toml` as `aqua:realm/SwiftLint`.

Deviations: `mise.toml` and `toolchain.md` edited beyond the card's two files; a separate CI job rather than a step in `desktop-macos`.

Check: from `desktop/macos`, `swiftlint lint --strict LintFixtures/Rejected/no_literal_size.swift` (`.padding(12)`) → `error: No literal size Violation (no_literal_size)`, exit 2; `LintFixtures/Accepted/PaddingView.swift` (`.padding(Space.l)`) → exit 0. The CI fixture step run locally: exit 0, all six Rejected files fail with their own rule; the Foundations fixture copied outside `Foundations/` fails four rules. `swift-format lint --strict --recursive LintFixtures` exit 0. `actionlint` 1.7.12 + shellcheck clean. The plugin's command (`BUILD_WORKSPACE_DIRECTORY=<pkg> swiftlint lint --quiet --force-exclude`) on a scratch package with the child config: `.padding(12)` exit 2, `Space.l` exit 0. Commit 7cd8ad7.

Not done: no real `swift build` through SwiftLintPlugins (SwiftPM stalled fetching the artifact bundle locally); no package lints yet — T37.16 and T37.19 attach the plugin.

#### T37.16 Swift: `CoxCore`, `CoreClient`, fixture client; `CoxModel` stores

Depends: T37.15 · Size: ~200 · Files: `desktop/macos/Packages/CoxCore/…`, `desktop/macos/Packages/CoxModel/…`
Goal: `CoxModel` turns patches into `@Observable` stores (timeline keyed by block id in swift-collections' `OrderedDictionary`, R§9.5.6; §1.1 row); a fixture client replays recorded streams so every view and test runs without Rust.
Check: `swift test` in `CoxModel` applies each recorded fixture and matches its expected final state.
Status: done 2026-09-28
Result: `desktop/macos/Packages/CoxModel` has two targets. `CoxClient`: the timeline and intent value types decoded from the fixtures' serde JSON (`Timeline.swift`, `TimelineDecoding.swift`, `Intent.swift`), the `CoreClient`/`SessionClient` protocols, and `Fixture` + `FixtureCoreClient`/`FixtureSession`, which replay `Fixtures/*.json` and record the intents sent. `CoxModel`: `SessionStore` (`@Observable @MainActor`) — the timeline in an `OrderedDictionary<BlockID, Block>`, the usage meter and the draft; `apply`/`send`/`run` follow `cox_app::coalesce::apply`, including the 5-line tool tail. `desktop/macos/Packages/CoxCore` declares the `CoxFFI` binary target (`../../build/CoxFFI.xcframework`) and `CoxFFIBindings` (a committed symlink to `build/bindings/cox_ffi.swift`, Swift 5 mode, links `SystemConfiguration` for hyper-util's proxy lookup); `LiveCoreClient`/`LiveSession` wrap the generated `App`/`SessionHandle`, and `Convert.swift` maps generated values to `CoxClient` values with exhaustive switches. Both packages carry SwiftLintPlugins 0.65.1 (not on the generated bindings) and a `.swiftlint.yml` with `parent_config`. DT§4.6 table updated; new dependency swift-collections 1.7.1.

Deviations: `CoreClient` and the fixture client live in `CoxModel`, not `CoxCore`: SwiftPM refuses to load a package that declares a missing local binary target, even for a dependent that uses another product, so this keeps `CoxModel` building and testing without Rust. ~1,080 non-test lines against ~200, mostly the Swift mirror of the patch types and the FFI conversions (DT§4.6). Package `.swiftlint.yml` files also set `cyclomatic_complexity: ignores_case_statements` (one case per enum variant).

Check: `swift test` in `CoxModel` 7/7, including `replayingAFixtureEndsAtItsSnapshot` for each `Fixtures/*.json` (blocks, keys and usage equal the recorded snapshot), upsert order, AppendText with `\r\n`, DocTail/Remove, send, the tail helper; `swift test` in `CoxCore` 3/3 conversion tests (xcframework built once, 4m39s); `swift build` clean; `swift-format lint --strict` and `swiftlint lint --strict` (0.65.1) pass on both packages, also with `build/` absent. Commits b0068ef, 79ade0f.

Not done: tests with the plugin attached did not run locally — SwiftPM stalls fetching the SwiftLintPlugins artifact bundle, so lint ran through the CLI and `swift test` once with the plugin lines removed; CI runs the real thing. `AppStore`/`SettingsStore` wait for T37.22, T37.27 and T37.30; mirroring `InboxItem` is left to the inbox card.

#### T37.19 `CoxUI` Foundations

Depends: T37.17, T37.18 · Size: split at claim · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Foundations/*`
Goal: every DS§6.1 modifier and style — `elevation`, `glassPane`, `specular`, `hairline`, `insetWell`, `textStyle`, button, capsule, segmented, toggle and slider styles — with Depth scaling, the readable floor and Reduce Transparency/Motion handled once, here.
Check: snapshot per modifier × light/dark × Solid/Frosted; Reduce Transparency renders Solid.
Status: done 2026-09-28
Result: first part of the Foundations (split at claim; the button, capsule, segmented, toggle and slider styles moved to T37.19.1–T37.19.4). New package `desktop/macos/Packages/CoxUI` (tools 6.2, macOS 26, `Colors.xcassets` as a resource, SwiftLintPlugins, no other cox package). `Sources/CoxUI/Foundations/`: `Elevation`, `GlassPane`, `Specular`, `Hairline`, `InsetWell`, `TextStyle` and `Appearance.swift`, the one place settings resolve — Reduce Transparency forces Solid, the readable floor holds, Depth scales every level but e5, text scale applies, `coxTransition` cross-fades under Reduce Motion. Modifiers are `internal`; the public API is `Appearance`, `GlassMaterial` and the `coxAppearance` environment value. DESIGN.md §2/§5/§6.1/§9 updated. Also `T37.17: Tokens.swift passes swift-format lint`: the generator emits `// swift-format-ignore-file` and a scoped `swiftlint:disable line_length`.

Deviations: ~380 source lines in 8 files. Swift headers use `//` (SwiftLint's `comment_spacing` rejects `//!`). SwiftPM generates `ColorResource` symbols, so views write `Color(.surfaceWindow)` (DESIGN.md says so). New dependency swift-snapshot-testing 1.19.6 (latest, named by A67). `ImageRenderer` drops glass, so snapshots render through a window-hosted `NSHostingView` at 2×.

Check: `swift build` ok; `swift test` first run recorded 24 snapshots (6 modifiers × light/dark × Solid/Frosted), second run 10 tests / 34 cases pass with nothing re-recorded, including Frosted under Reduce Transparency pixel-identical to Solid; `swiftlint lint --strict` and `swift-format lint --strict --recursive` (LintFixtures, CoxUI) pass. Built and tested without the plugin lines locally (SwiftPM stalls fetching the plugin's artifact bundle); the committed manifest keeps the plugin. Commits 112cb7e, 5786619.

Not done: no `#Preview`s (snapshots cover the Foundations). Snapshots were recorded on macOS 27 / Xcode 27 while CI runs macos-26; glass and font rendering may exceed the 0.98 tolerance there and need re-recording on CI's OS.

#### T37.37 Spike: the cross-block selection engine

Depends: T37.16 · Size: ~200 (throwaway spike plus a result note) · Files: `desktop/macos/Spikes/Selection/…`, `research.md`
Goal: decide how the transcript selects text across blocks (A67). Build the same 2 000-block fixture (prose, code, diffs, tool cards) twice: with Textual 0.5.0 (MIT, R§9.5.10) and with our own TextKit 2 view — one `NSTextView` over the whole transcript with the cards as view-backed attachments. Measure: one continuous drag selects across blocks, copy keeps block order as Markdown, clamping to one block when `cross_block_selection = false`, first frame and scroll frame time against the DT§9 budget. If Textual passes, it is taken (§1.1 row); if not, our view becomes its own package `desktop/macos/Packages/CoxTranscriptText` with its own cards. STTextView is out (A68).
Check: the result table with both measurements is in `research.md` §9.5; T37.23's card names the chosen engine.
Status: done 2026-09-28
Result: `desktop/macos/Spikes/Selection/` — a standalone SwiftPM package (Textual pinned exact 0.5.0) with the same 2 000-block fixture built twice: Textual (one view per block, and the whole transcript as one `StructuredText`) and one TextKit 2 `NSTextView` with tool cards as view-backed attachments; 8 headless Swift Testing tests (offscreen window, real `NSEvent` drags through `NSWindow.sendEvent`, frame timing with `CACurrentMediaTime`). `research.md` §9.5.13 holds the table, method, risks and verdict (Textual tag 0.5.0, commit `01b51875`, MIT, released 2026-06-15, checked 2026-09-28); row 9.5.10 marked rejected. Textual fails three of four: per-block views keep a drag in block 0; copy gives plain text and HTML, not Markdown; no clamp API; its one-document shape takes 5.5 s to a first frame with a 640 % hitch ratio. TextKit 2 passes all four: the drag selects across blocks including a tool card; copy gives Markdown in block order; the setting off clamps to the start block both ways; first frame 131–133 ms, no scroll frame over 16.7 ms at 2 000 and 10 000 blocks. Our view becomes `CoxTranscriptText` (T37.40–T37.43, A87); T37.23 names the engine. `Spikes` excluded in `.swiftlint.yml`.

Deviations: ~890 lines in 11 files (throwaway spike plus tests); also measured at 10 000 blocks, the DT§1 scroll budget's size.

Check: `swift test --no-parallel --package-path desktop/macos/Spikes/Selection` twice, 8/8 each, stable numbers; `swiftlint lint` reports no `Spikes/` files; swift-format applied. Commit 419edd6.

Not done: not measurable headlessly — hand drag with autoscroll, trackpad momentum, GPU/compositing time, VoiceOver, reliable memory, scroller jumps on estimated heights. Numbers come from a shared M3 Max, not the M1 Air 8 GB the budgets target; T37.23's gate still runs there.

#### T37.19.1 `CoxButtonStyle`

Depends: T37.19 · Size: ~100 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Foundations/*`, its tests and snapshots
Goal: primary, secondary, danger and plain buttons in each DS§6.1 size, built on the Foundations (`elevation`, `specular`, `textStyle`) with hover, pressed and disabled states.
Check: snapshot per role × size × light/dark × Solid/Frosted; the disabled state keeps the readable floor.
Status: done 2026-09-28
Result: `Foundations/CoxButtonStyle.swift` — primary, secondary, danger and plain roles in regular and small sizes, built on `elevation` (e1), `specular`, `hairline` and `textStyle`. New shared `Foundations/ControlState.swift`: rest, hovered, pressed, disabled and the view that resolves them (hover adds `fill.primary`, press adds `fill.secondary` and drops to e0). A disabled button keeps its face at the readable floor and a `text.secondary` label (4.5:1, DS§8) instead of fading; a disabled primary shows the secondary face. DESIGN.md §5 and the §6.1 button row updated.

Deviations: `ControlState.swift` is a new shared file; the primary label is `Color.white` as a named constant (no on-accent colour token exists); the snapshot helper `Tests/CoxUITests/StyleSnapshot.swift` copies the render code of `FoundationsTests.swift`.

Check: 32 snapshots (4 roles × 2 sizes × light/dark × Solid/Frosted, each showing all four states); unit tests: a disabled face stays at or above the readable floor at window opacity 0, which state wins, the pressed/disabled lift; second run passes with nothing re-recorded; `swiftlint --strict` and `swift-format lint --strict` clean. After merging with T37.19.3–T37.19.4: `swift test` in CoxUI 25 tests in 7 suites pass, both linters clean. Commit 7b240e3.

Not done: no `#Preview`s. Built and tested without the SwiftLintPlugins lines (SwiftPM stalls fetching the plugin locally).

#### T37.19.2 `CapsuleStyle`

Depends: T37.19 · Size: ~100 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Foundations/*`, its tests and snapshots
Goal: the plain and active capsules of DS§6.1 (chips, filters), built on the Foundations.
Check: snapshot per state × light/dark × Solid/Frosted.
Status: done 2026-09-28
Result: `Foundations/CapsuleStyle.swift` — plain and active capsules: a `glassPane` face with `surface.capsule` in the readable role, a hairline, e1 and `.control` text, with `ControlState`'s four states; active takes `surface.window`, an `accent` label and the mockup's 3 pt `accent.soft` halo, drawn as a filled pill behind the face. DESIGN.md §6.1 capsule row updated.

Deviations: the border is the Foundations `hairline` (`separator`), so `surface.capsuleBorder` stays unused; the halo width is a named constant.

Check: 8 snapshots (plain/active × light/dark × Solid/Frosted); two runs pass; both linters clean; merged run as in T37.19.1. Commit 387b44a.

Not done: no icon-only (`.cap.icon`) variant. A stroked pill shows stray vertical bars at its ends in the 2× window capture (why the halo is a fill; the `hairline` capsule snapshot shows a faint bar too) — not checked on screen.

#### T37.19.3 `SegmentedStyle`

Depends: T37.19 · Size: ~100 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Foundations/*`, its tests and snapshots
Goal: the segmented control of DS§6.1 with the lifted e1 selection, built on the Foundations; selection moves with `coxTransition`.
Check: snapshot per selection × light/dark × Solid/Frosted; Reduce Motion cross-fades.
Status: done 2026-09-28
Result: `Foundations/CoxSegmented.swift` — `CoxSegmented(_ label:, selection:, options:, title:)`: a glass capsule (`glassPane` readable, capsule border, e1) with an e1-lifted pill on the selected segment that slides between segments and cross-fades under Reduce Motion; VoiceOver sees a real segmented `Picker`. `Appearance.swift` gains `coxMatchedGeometry(id:in:)` (matched geometry unless Reduce Motion), keeping the Reduce Motion decision in that one file. DESIGN.md §5 and §6.1 updated.

Deviations: a view, not a `SegmentedStyle` — SwiftUI cannot restyle a segmented picker's segments on macOS. The selection moves with `coxMatchedGeometry`, not `coxTransition` (a transition cannot carry one view between segments). `Tests/CoxUITests/ControlSnapshots.swift` repeats the window-hosted 2× renderer.

Check: 12 snapshots (selection × light/dark × Solid/Frosted); `selectionSlidesThroughTheMiddleSegment` and `reduceMotionCrossFadesTheSelection` sample frames mid-animation (the latter fails with the gate removed); three consecutive passes; merged run as in T37.19.1. Commit e9dc3f0.

Not done: nothing from the card.

#### T37.19.4 `CoxToggleStyle` and `CoxSliderStyle`

Depends: T37.19 · Size: ~100 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Foundations/*`, its tests and snapshots
Goal: the DS§6.1 toggle and slider with the 3D knob, built on the Foundations.
Check: snapshot per on/off and slider value × light/dark × Solid/Frosted.
Status: done 2026-09-28
Result: `Foundations/Knob.swift` — the shared 3D knob (white disc shaded towards the shadow tint, hairline rim, e1). `CoxToggleStyle.swift` — the label, then an `insetWell` track filled with `accent` when on; the knob moves with `coxMatchedGeometry` and cross-fades under Reduce Motion. `CoxSlider.swift` — `CoxSlider(_ label:, value:, in:)` with an `insetWell` track, an `accent` gradient fill up to the value, the knob, a drag gesture and clamping. VoiceOver sees a real switch and slider. DESIGN.md §5 and §6.1 updated.

Deviations: `CoxSlider` is a view, not a `CoxSliderStyle` (macOS has no public `SliderStyle`); the toggle's on colour is `accent` per DS§6.1, not the mockup's green; four files (the knob has its own); track and knob sizes are private constants citing the mockup, as `InsetWell` does.

Check: 20 snapshots (on/off and slider 0/50/100 % × light/dark × Solid/Frosted); the knob sits at the value's share of the range; out-of-range values clamp; merged run as in T37.19.1. Commit fb412f5.

Not done: no disabled-state visuals for toggle or slider (the card does not ask).

#### T37.20 `CoxUI` Atoms

Depends: T37.19.1–T37.19.4 · Size: split at claim · Files: `…/CoxUI/Atoms/*`, `…/CoxUI/Previews/PreviewState.swift`
Goal: every DS§6.2 atom, one file each, with previews and snapshots for every variant.
Check: snapshot suite green; lint green; each atom's file names a DS§6.2 row.
Status: done 2026-09-28
Result: first part of the atoms (split at claim; the rest moved to T37.20.1–T37.20.4). `Sources/CoxUI/Atoms/`: StatusDot, IconTile, KeyCap, Badge, CountBadge, RiskChip (drawn as a Badge), DiffStat, SectionHeader, InlineCode — one file each, the header naming its DS§6.2 row, a `#Preview` per variant over light/dark × Solid/Frosted. `Sources/CoxUI/Previews/PreviewState.swift`: fixtures plus `PreviewBackdrop`, `PreviewPane`, `PreviewMatrix`. `Tests/CoxUITests/Snapshot.swift` is now the one snapshot harness (`Variant`, `SnapshotHost`, `assertCoxSnapshot`); the three earlier copies are gone and every suite uses it with its references unchanged. DESIGN.md §6.2 IconTile and RiskChip rows name their parameters.

Deviations: ~510 source lines in 10 files. Mockup values with no token are named private constants (Badge/InlineCode radius 5 and 1 pt vertical padding, InlineCode 5 pt horizontal padding, CountBadge height 16, StatusDot halo 3 and idle ring 1.5, project tint 0.13, CountBadge text `Color.white`). The project badge uses `status.plan` (blue), not the mockup's purple — no purple role token. RiskChip maps low→neutral, medium→warning, high→danger. KeyCap has a `surface.capsule` face at e1 (DS§3.4).

Check: helper merge alone — 25 tests in 7 suites pass against the existing references; then the first run recorded 100 atom snapshots, second and third runs 37 tests in 9 suites pass with nothing re-recorded; `swiftlint --strict` and `swift-format lint --strict` clean. Built and tested without the SwiftLintPlugins lines (plugin fetch stalls locally); `Package.swift`/`Package.resolved` unchanged. Commits 0cf1ff9, 9bc39d3.

Not done: Spinner, ProgressRing, Sparkline, StackedBar, Thumbnail and the Hairline atom (T37.20.1–T37.20.4).

#### T37.40 `CoxTranscriptText`: the TextKit 2 transcript view

Depends: T37.37 · Size: ~200 · Files: `desktop/macos/Packages/CoxTranscriptText/…`
Goal: a new package with `TranscriptTextView`, one TextKit 2 `NSTextView` over the whole transcript built from timeline blocks, each block a tracked text range; SwiftLint and `swift test` wired like the other packages (research.md §9.5.13).
Check: a test builds the view from a fixture and maps every block id to its range and back; `swift test` and both linters pass.
Status: done 2026-09-28
Result: new package `desktop/macos/Packages/CoxTranscriptText` (tools 6.2, macOS 26, Swift 6; SwiftLintPlugins 0.65.1; depends only on `CoxModel`'s `CoxClient`). `TranscriptTextView` — one read-only, selectable TextKit 2 `NSTextView` over the whole transcript (`make(style:)`, `inScrollView(frame:)`, `load(_ blocks:)`, `range(of:)`, `blockID(at:)`). `BlockRanges` maps a block id to its range and a location back to its id by binary search (a caret right after a block's last character is inside it; a block with no text gets a zero-length range and no line). `TranscriptText.swift` builds one attributed string, each kind as plain text for now; `TranscriptStyle` carries fonts, colour, spacing and inset so the package spells out no design values. DT§4.6 table and §6 layout tree updated.

Deviations: no dependency on `CoxUI` — the transcript organism (T37.23) lives in `CoxUI` and imports this package, so `CoxUI` builds the `TranscriptStyle` from its tokens. 262 LOC in 3 source files.

Check: `swift test` 6/6 (temporary manifests without the plugin lines), including `everyFixtureBlockMapsToItsRangeAndBack` — builds the view from `Fixtures/read-and-reply.json`, maps every block id to its range and back and every location to its block, and asserts TextKit 2 stays on; `swiftlint lint --strict` and `swift-format lint --strict` clean; `swift package resolve` with the real manifest wrote `Package.resolved`. Commit 3401cf9.

Not done: cards (T37.41), copy and clamp (T37.42), `StyledDoc` styling and incremental building (T37.43). For T37.43: `CoxUI`'s colour assets are internal, so mapping `StyleToken` colours into `TranscriptStyle` needs a public accessor in `CoxUI`.

#### T37.30 Settings from the schema with provenance; Keychain keys; MCP OAuth

Depends: T37.16 · Size: split at claim · Files: `…/Screens/SettingsScreen.swift`, `desktop/macos/Packages/CoxPlatform/Secrets.swift`
Goal: settings rendered from `docs/config.jsonschema` with the layer each value came from; keys stored through the `Host` trait on the Security framework directly, no wrapper package (R§9.5.8). Tests never touch the real keychain (A49).
Check: snapshot of a setting overridden by the project layer; secrets tests use an in-memory store.
Status: done 2026-09-28
Result: the non-view core (split at claim; the screen, the app's `AppHost` and MCP login moved to T37.30.1–T37.30.4). `crates/cox-app/src/settings.rs`: per config leaf the value, the layer it came from, the control kind and the schema's help text, and whether it is editable — read-only once a project, Claude-settings, env or flag layer sets it (DT§5.7); `set` writes through cox-config's comment-preserving `set` and restores the user file if the config no longer loads; `App::settings`/`App::set_setting`. cox-config gains `schema()`, `cmd::leaves()`, `cmd::set_json_in()` (typed JSON, no bare-string fallback) and `load::load_in()`. cox-ffi exports `settings` and `set_setting` (mirrors in `types.rs`; lib+session+host still 299 lines). Swift: `CoxClient` settings values, `SettingsClient` with a fixture client, `SecretStore` with an in-memory store; `CoxModel` `SettingsStore`; new package `CoxPlatform` with `KeychainSecretStore` on the Security framework (generic password, service `cox`, account = provider section — the item the CLI's keyring uses; keychain calls injectable); `CoxCore`'s `LiveCoreClient` serves settings.

Deviations: 9 Rust and ~14 Swift files. `cox-app` sessions load `<app home>/config.toml` (`load_in`), so a session and the Settings screen read the same file. `Project`/`OpenRequest` moved from cox-ffi `lib.rs` to `types.rs`. No Rust `store_key`: keys are written from Swift through `SecretStore` and read through `Host::secret` (DT§4.4, §5.7 updated). `schemars` is a normal dependency of cox-config (already in the build through cox-protocol).

Check: insta snapshot `a_setting_the_project_overrides_is_read_only_with_its_layer` (`tiers.code.model` from the project layer, read-only; the project's budget raise dropped by the guard list); secrets tests use in-memory stores only; real binary against a scratch `COX_HOME`: `config set`, `config show --sources`; `swift test` CoxModel 12/12, CoxPlatform 5/5, CoxCore 5/5 (incl. a live round trip through the Rust core; xcframework rebuilt once); `swiftlint --strict` and `swift-format lint --strict` clean on the three packages (tests without the plugin lines locally). After the merge into `p37-desktop`: `nextest -p cox-config -p cox-app -p cox-ffi` 61/61, `-p cox --test deps` 9/9, clippy and fmt clean. Commit 190c199.

Not done: T37.30.1–T37.30.4.

#### T37.20.3 `Thumbnail`

Depends: T37.20 · Size: ~100 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Atoms/*`, its tests and snapshots
Goal: `Thumbnail(attachment)` in its image and file variants (DS§6.2).
Check: snapshot per variant × light/dark × Solid/Frosted.
Status: done 2026-09-28
Result: `Atoms/Thumbnail.swift` — the DS§6.2 attachment tile (mockup `.thumb`), `Thumbnail(name, image: Image? = nil)`: the image variant fills a 92×60 tile; the file variant shows `doc.text` over the file name, truncated in the middle so the extension stays visible; `Radius.m` corners, a `.hairline(in:)` rim, VoiceOver reads the name. Plain values, since `CoxUI` must not depend on `CoxModel`. Fixtures in `Previews/PreviewState+Thumbnail.swift`; tests in `Tests/CoxUITests/ThumbnailTests.swift`.

Deviations: fixtures in a new Previews file (avoids a conflict with T37.20.1–T37.20.2); the image variant has no name label (text over an arbitrary picture cannot meet DS§8 contrast); tile size as named private constants; the 10 px label uses `.micro`; the file face uses `fillPrimary` instead of the mockup's placeholder gradient.

Check: 8 snapshots recorded, then a full `swift test` 38 tests in 10 suites pass without re-recording; `swiftlint --strict` and `swift-format lint --strict` clean (tests without the plugin lines locally). Commit ba19224.

Not done: nothing.

#### T37.20.4 `Hairline` atom

Depends: T37.20 · Size: ~100 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Atoms/*`, its tests and snapshots
Goal: the DS§6.2 hairline atom. SwiftPM rejects two files of the same name in one target and `Foundations/Hairline.swift` exists, so the atom's file (or the Foundations file) takes another name; the type names must not clash either.
Check: snapshot per orientation × light/dark; the Foundations `hairline` references stay unchanged.
Status: done 2026-09-28
Result: `Atoms/Hairline.swift` — `Hairline(.horizontal | .vertical)`, a free-standing 0.5 pt `separator` rule across its container (mockup `.divider:before`, `.popover .sep`), drawn through the existing `.hairline` modifier so rules and edges share one width and colour. `Foundations/Hairline.swift` renamed to `Foundations/HairlineModifier.swift` (content unchanged) to free the file name; no type names clash. DESIGN.md §5 and §6.2 updated.

Deviations: snapshots cover orientation × light/dark × Solid/Frosted (`Variant.all`), a superset of the card's light/dark.

Check: 8 snapshots recorded, then a full `swift test` 39 tests in 11 suites pass, the Foundations `hairline` references unchanged; `swiftlint --strict` and `swift-format lint --strict` clean. Commit ffb4282.

Not done: nothing.

#### T37.42 Copy as Markdown and the one-block clamp

Depends: T37.40 · Size: ~150 · Files: `…/CoxTranscriptText/…`
Goal: copy of a selection writes Markdown in block order (cards as their summary lines) next to plain text; with `cross_block_selection = false` a drag is clamped to the block it started in, in both directions (A67).
Check: a test drags across three blocks and the pasteboard holds their Markdown in order; with the setting off the same drag stays in the first block.
Status: done 2026-09-28
Result: `CoxTranscriptText/MarkdownCopy.swift` — copying a selection writes Markdown (new `NSPasteboard.PasteboardType.markdown`, `net.daringfireball.markdown`) and plain text (`.string`), blocks in transcript order, from each block's timeline value rather than the drawn text: a whole reply copies as its source (rebuilt from the `StyledDoc` when empty); tool, tool group, approval, question and task blocks copy as their summary line however they are drawn; a cut code block stays fenced. `Selection.swift` — with `crossBlockSelection` off, `setSelectedRanges` keeps a drag inside the block it started in, both directions; a settled change touching the current block (⇧-arrow, select all) stays in it, a change elsewhere (find) moves to that block. `TranscriptTextView` gains `crossBlockSelection` (passed in by the app), a `blocks` map filled in `load`, and `dragAnchor` (+11 lines). DT§5.2 updated.

Deviations: Markdown goes under its own pasteboard type next to plain text in `.string` (the spike put Markdown in `.string`); ~210 lines in 3 files.

Check: `swift test --no-parallel` 11/11 (5 new in `SelectionTests.swift`, real `NSEvent` drags in an offscreen window, a private named `NSPasteboard`): `dragAcrossThreeBlocksCopiesTheirMarkdownInOrder`, `withTheSettingOffTheDragStaysInItsFirstBlockBothWays`; both clamp tests fail with the clamp disabled; `swift build --build-tests` no warnings; `swift-format lint --strict` and `swiftlint --strict` clean (scratch manifests without the plugin). Commit 8ba9d64.

Not done: the "Copy as Markdown" context-menu item and ⇧-click gutter selection (DT§5.2) are not in this card. Mid-stream, `Block.assistant.text` can lag because `docTail` updates only the doc, so a whole-reply copy while streaming may return an older source (fix belongs in `SessionStore`). Partial-reply copy assumes the doc-block layout `TranscriptText.run` produces now.

#### T37.20.1 `Spinner` and `ProgressRing`

Depends: T37.20 · Size: ~100 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Atoms/*`, its tests and snapshots
Goal: the DS§6.2 indeterminate spinner and `ProgressRing(fraction)`, with the Reduce Motion fallback resolved in `Appearance`.
Check: snapshot per variant × light/dark × Solid/Frosted; under Reduce Motion the spinner does not rotate.
Status: done 2026-09-28
Result: `Atoms/Spinner.swift` — the mockup's `.spin`: a `fill.secondary` ring with an `accent` quarter arc, turned by a new `coxSpin(period:)` modifier in `Foundations/Appearance.swift` that holds it still under Reduce Motion (Reduce Motion stays read only in Foundations). `Atoms/ProgressRing.swift` — `ProgressRing(fraction)`, the mockup's `.ring`: an `accent` arc clockwise from twelve o'clock over `fill.secondary`; the fraction clamps to 0…1 (NaN as 0) and VoiceOver reads a percent. Headers name their DS§6.2 rows; `#Preview`s through `PreviewMatrix`; fixtures in `Previews/PreviewState+Meter.swift`; tests `ProgressAtomTests.swift` (16 snapshots).

Deviations: the spinner snapshot is taken with Reduce Motion on (the still pose), so the image does not depend on first-frame timing. Named private constants: spinner 12 pt, line 2 pt, arc 0.25; ring 14 pt, line 3 pt; spin period 1 s.

Check: recorded once, two runs pass without re-recording (5 tests); `spinnerTurns` sees more than one distinct frame, `spinnerHoldsStillUnderReduceMotion` exactly one; `swiftlint --strict` and `swift-format lint --strict` clean. Commit 282f6e3.

Not done: nothing.

#### T37.20.2 `Sparkline` and `StackedBar`

Depends: T37.20 · Size: ~100 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Atoms/*`, its tests and snapshots
Goal: the token meter's data graphics: `Sparkline(samples)` (tint, gradient fill) and `StackedBar(segments)` (segment colours from tokens).
Check: snapshots for empty, one-sample and full series and for a bar of each segment mix × light/dark.
Status: done 2026-09-28
Result: `Atoms/Sparkline.swift` — `Sparkline(samples, tint:)`: a tinted line over a fill fading to nothing (tint defaults to `meter.received`); the largest sample at the top, negative or non-finite samples drawn as zero, one sample a flat line; fills the caller's frame; hidden from VoiceOver (the meter reads its numbers, DS§8). `Atoms/StackedBar.swift` — `StackedBar(segments)`: one segment per context part (system, tools, instruction files, history) in `context.*` colours over a `fill.secondary` capsule; an overrunning share is cut; VoiceOver reads each part with its share. Fixtures and preview wrappers in `PreviewState+Meter.swift`; tests `MeterAtomTests.swift` (32 snapshots: empty, one-sample, full and sent-tint series; empty, history-only, the mockup's turn and full bars; × light/dark × Solid/Frosted).

Deviations: snapshots also cover Solid/Frosted. Named private constants: line 1.5 pt, fill opacity 0.35, insets 2 pt top and 1 pt bottom, bar height 12.

Check: recorded once, two runs pass (6 tests); both linters clean. The whole CoxUI suite (48 tests, 11 suites) failed once on the older timing test `SegmentedTests.reduceMotionCrossFadesTheSelection` (missed its mid-fade frame under load) and passed alone and in the next two full runs. Commit ec3f05e.

Not done: nothing.

#### T37.21 `CoxUI` Molecules

Depends: T37.20.1–T37.20.4 · Size: split at claim · Files: `…/CoxUI/Molecules/*`
Goal: every DS§6.3 molecule built only from atoms and foundations.
Check: snapshot suite green; no molecule imports `CoxCore`; no styling modifier is applied to an atom from outside it except through the atom's own parameters.
Status: done 2026-09-28
Result: first set of molecules (split at claim; the rest moved to T37.21.1–T37.21.10) — the ones the window shell (T37.22), Settings (T37.30.1) and the token popover (T37.25) need. `Sources/CoxUI/Molecules/`: SessionRow, SessionFilter, Breadcrumb, ModelCapsule, CostCapsule, ModeSegmented, StopButton, LabeledToggle, LabeledSlider, KeyValueGrid — headers naming their DS§6.3 rows, a `#Preview` per variant, SwiftUI-only imports; fixtures in `Previews/PreviewState+Shell.swift` and `+Settings.swift`; 84 snapshots in `ShellMoleculeTests.swift` and `SettingMoleculeTests.swift`. Foundations: `.symbolStyle(_:)` draws an SF Symbol per DS§3.7 (IconTile and Thumbnail use it, images unchanged); `CoxSegmented` gains `look:`. Timing fix: the segmented slide/cross-fade and spinner tests hold the animation half-way and wait for the first changed frame (`SnapshotHost.bitmap(until:limit:)`), so load delays but cannot change what they see. DESIGN.md §3.7, §6.1, §6.3 updated.

Deviations: ~590 source lines in 17 files. SessionRow cost uses `text.secondary` (readable on frosted glass, DS§8) instead of the mockup's tertiary; its title uses `.body` (no 13 pt medium token); the slider heading reuses SectionHeader; Bypass shows in the mode control only while on. Molecule snapshots render at their ideal size (`fixedSize`) because the harness sizes a sample up to half a point small — fixing the harness would re-record every reference. No new private constants: nearest tokens stand in (grid gaps `Space.xs` × `Space.l`, detail indent `Space.ml`, row gap `Space.m` for 9 px).

Check: the reworked Reduce Motion test fails with the gate removed from `coxMatchedGeometry`, the spinner test with it removed from `coxSpin`; timing tests pass three runs under 12 CPU burners; full `swift test` 62 tests in 16 suites pass twice with nothing re-recorded; `swiftlint --strict` and `swift-format lint --strict` clean (plugin-free manifest locally). Commits b9dacd8, 8b7d55c, be87348.

Not done: T37.21.1–T37.21.10; TokenMeter stays with T37.25.

#### T37.21.8 `MaterialPicker`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: the appearance popover's glass picker (frosted, glossy, solid) with depth preview. Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `Molecules/MaterialPicker.swift` — `MaterialPicker(selection: Binding<GlassMaterial>)`: three swatch tiles in the mockup's order (frosted, glossy, solid) over the existing `GlassMaterial` values; tiles are readable glass at e1 with a hairline, the selected one with an `accent` ring; each swatch shows a small pane in its own material over a wallpaper, lifted to e2 at the user's Depth (`Appearance.swatch(_:)` keeps depth and text size); VoiceOver sees a picker. Fixture `Previews/PreviewState+Appearance.swift`; tests `MaterialPickerTests.swift` (20 snapshots, 2 unit tests); DS§6.3 row updated.

Deviations: not built on `CoxSegmented` (capsule-high text segments cannot show glass); tile padding `Space.xs`, swatch radius `Radius.m` (concentric, DS§3.3); swatch height 40 pt as a private constant; a hairline rim keeps tiles visible at Flat.

Check: snapshot per selected material plus Flat × light/dark × Solid/Frosted; SwiftUI-only imports; full suite 69 tests in 20 suites pass twice without re-recording; `swiftlint --strict`, `swift-format lint --strict` clean (plugin-free manifest locally). Commit 3eb1f80.

Not done: nothing.

#### T37.21.9 `ChangedFileRow` and `CheckpointRow`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: a changed file with diff stat and actions, and a rewind checkpoint row. Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `Molecules/ChangedFileRow.swift` and `Molecules/CheckpointRow.swift` over a shared `Molecules/InspectorRow.swift` (also `RowAction`: title, symbol, `@MainActor` perform). The changed-file row shows `pencil`/`doc.text` for edited/created, the path with its folder dimmed so the file name survives truncation, and a `DiffStat`; the checkpoint row a clock, label and time. Selected rows sit on `accent.soft` at e1 like `SessionRow`; action icons appear on hover or selection with tooltips and as VoiceOver actions. Fixtures `Previews/PreviewState+Inspector.swift`; tests `InspectorRowTests.swift` (16 snapshots, 2 path-split unit tests); DS§6.3 row updated.

Deviations: the shared layout is a third file; the time uses `text.secondary` (readable on frosted glass, DS§8); `DiffStat` shows "−0" for add-only files (atom unchanged); the clock is SF Symbol `clock`, not yet in DS§3.7.

Check: snapshot per variant × light/dark × Solid/Frosted; SwiftUI-only imports; full suite as in T37.21.8. Commit a0a83a6.

Not done: `clock` in the DS§3.7 symbol table; `SessionRow` could reuse the shared selected-row styling.

#### T37.22 Window shell: split view, sidebar, toolbar, inspector frame

Depends: T37.16, T37.21 · Size: ~200 · Files: `…/CoxUI/Organisms/Sidebar.swift`, `…/Organisms/SessionToolbar.swift`, `…/Screens/MainScreen.swift`
Goal: DS§4 layout with floating glass panes, collapsible sidebar and inspector, the window material from `[desktop.appearance]`.
Check: snapshots of the main screen in Solid, Frosted and Glossy match `desktop/design/mockups` screens 28–29 in structure.
Status: done 2026-09-28
Result: `CoxUI/Screens/MainScreen.swift` composes the window: sidebar, toolbar, transcript column and inspector on the window glass. It takes `MainScreenState`, emits `MainScreenIntent`, and has slots for the transcript (T37.23/T37.24) and inspector tab content (T37.29). The side panes fold with `Motion.durationSlow`. The organisms in `Organisms/`: `ShellPane` draws the window (e5), side (e2) and column (flat) layers from `coxAppearance`; `Sidebar` has the filter, status sections with counts, foldable projects and a footer; `SessionToolbar`; `Inspector` has five tabs and an empty slot. Fixtures from mockup screen 28 are in `Previews/PreviewState+Window.swift`. DESIGN.md §4, §6.1, §6.4 and §6.5 are updated.
Deviations:
- The screen lays out its own panes instead of using `NavigationSplitView`: the system split view draws its own glass and toolbar, ignores `[desktop.appearance]` and cannot be snapshotted (DS§4 says why).
- 7 source files instead of 3 (adds `ShellPane`, `Inspector`, the fixtures and an `isIcon` variant in `CapsuleStyle`).
- Main-screen snapshots are recorded at 1×; organism snapshots are 2×.
- Everything is `internal`; the app target will need a public surface.
- One new named constant: `windowButtonsWidth = 68`.
Check: `swift test`, run twice against the committed snapshots: 69/69 passed both times (19 new MainScreenTests snapshots). `swiftlint lint --strict` and `swift-format lint --strict` are clean. Mockup screens 28–29 were compared by eye.
Not done:
- Shortcuts: DS§4 says ⌘0/⌘⌥0 but DT§5 says ⌘⌃S/⌥⌘I, so neither is bound.
- Bypass strip: DS puts it at the window top, DT under the toolbar, so it is not drawn.
- The inspector overlay below 1280 pt and the app window setup (hidden title bar, behind-window blur) are in ideas.md.

#### T37.21.1 `ToolHeader`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: the tool card header: icon tile, summary line, state and duration, the disclosure chevron. Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `CoxUI/Molecules/ToolHeader.swift` shows the tool icon, a summary with the subject in bold (monospaced for a command), lines added and removed, the risk chip, the state (spinner, check or cross) with its duration, and the disclosure chevron. A header that can open is a button with hover and press states; when expanded it sits on `fill.primary` over a hairline. Fixtures are in `Previews/PreviewState+Tool.swift`; `ToolMoleculeTests` has 20 snapshots (edited, expanded, running, explored, failed). DESIGN.md has the ToolHeader row.
Deviations: ~181 source lines against a ~150 estimate. A small refactor of the state glyph (no visual change) landed in the T37.21.4 commit.
Check: `swift test -j 4`, second run against the committed snapshots: 68 tests in 19 suites passed. `swift-format lint --strict` and `swiftlint lint --strict` are clean. After merging into `p37-desktop`, `swift build --build-tests` succeeded.
Not done: none.

#### T37.21.2 `DiffLineView` and `DiffHunkView`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: diff lines (added, removed, context, gutter numbers) and a hunk with its header, from plain values. Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `CoxUI/Molecules/CodeRun.swift` is one value type for code text plus its syntax role. CodeBlockView shares it, and the app maps Rust `StyledDoc` spans to it. `DiffLineView` shows the gutter number, the sign and highlighted code on the added or removed colours. `DiffHunkView` shows the `@@` header with one gutter width for the whole hunk, on `surface.code`. Fixtures are in `PreviewState+Code.swift`. `CodeMoleculeTests` has 16 snapshots plus a unit test of how runs become text. DESIGN.md row updated.
Deviations: context line numbers use `text.secondary` (the mockup's tertiary misses 4.5:1, DS§8). The hunk header uses `font.mono.code` because there is no 11 pt mono token. About 187 lines over 3 source files.
Check: the same runs as T37.21.1: 68/68 passed with no re-recording; lints clean; the merged build succeeded.
Not done: none.

#### T37.21.3 `CodeBlockView`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: a highlighted code block with language label and copy button, taking pre-styled runs as plain values. Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `CoxUI/Molecules/CodeBlockView.swift` is a `radius.l` card. It has a language label and an icon-only copy button (`doc.on.doc`, with a tooltip and an accessibility label) over a hairline, and highlighted code that scrolls sideways instead of wrapping. Copy is a closure; the app owns the pasteboard. 8 snapshots, with and without a language. DESIGN.md row updated.
Deviations: none.
Check: the same runs as T37.21.1: 68/68 passed with no re-recording; lints clean; the merged build succeeded.
Not done: none.

#### T37.21.4 `TerminalTail`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: the last lines of a running command in an inset well, monospaced, with the exit state. Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `CoxUI/Molecules/TerminalTail.swift` shows lines in an inset well on `surface.terminal`, each cut with an ellipsis, in `font.mono.terminal`. Lines and the exit status arrive as ready strings. While running there is no exit line. Success shows a check with the status in `text.terminalOk`. Failure shows a `status.danger` cross with the status in `text.terminal`, because danger-coloured text misses 4.5:1 on the well. 12 snapshots. DESIGN.md row updated.
Deviations: carries the small ToolHeader state-glyph refactor (no visual change).
Check: the same runs as T37.21.1: 68/68 passed with no re-recording; lints clean; the merged build succeeded.
Not done: none.

#### T37.21.5 `UserBubble` and `ThinkingDisclosure`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: the user turn bubble with attachments row, and the collapsible thinking block. Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `CoxUI/Molecules/UserBubble.swift` shows the prompt on a readable face at e2, with a row of `Thumbnail`s built from `UserBubble.Attachment`. `ThinkingDisclosure.swift` shows a chevron and the summary; opened, it shows the reasoning in italics beside a hairline, and the open state is the view's own `@State` animated with `Motion.durationBase`. Fixtures are in `Previews/PreviewState+Turn.swift` and snapshots in `TurnMoleculeTests`. DS§6.3 rows give the signatures.
Deviations: the bubble draws its glass face in `.background`, because the specular sweep over the content washed out the prompt text (DS§8). The nearest tokens stand in for the mockup's 14 px sides (`Space.l`) and its 2 px thinking rule (the hairline).
Check: each new suite recorded its snapshots once and then passed without re-recording. After the last commit the full `swift test` ran twice: 71 tests in 21 suites passed both times. `swiftlint lint --strict` and `swift-format lint --strict` are clean. After merging into `p37-desktop`, `swift build --build-tests` succeeded.
Not done: none.

#### T37.21.6 `NoticeRow`, `TurnDivider` and `TurnMeta`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: notices (info, warning, error), the divider between turns, and the per-turn meta line (tokens, cost, duration, stop reason). Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `NoticeRow` (info, warning, error, optional symbol), `TurnDivider` (`Hairline` atoms around an optional caption) and `TurnMeta` (a `Facts` value in the mockup's order) are in `CoxUI/Molecules/`, with fixtures and snapshots in the turn files and a unit test on the order of the meta facts.
Deviations: warning and error text is `text.primary`, with the colour on the symbol only; the meta line is `text.secondary` in `font.footnote`. The mockup's red and tertiary text miss DS§8.
Check: each new suite recorded its snapshots once and then passed without re-recording. After the last commit the full `swift test` ran twice: 71 tests in 21 suites passed both times. `swiftlint lint --strict` and `swift-format lint --strict` are clean. After merging into `p37-desktop`, `swift build --build-tests` succeeded.
Not done: none.

#### T37.21.7 `ComposerChip`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: the composer's mention, attachment and command chips with remove affordance. Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `CoxUI/Molecules/ComposerChip.swift`, `ComposerChip(label, kind:, shortcut:, onRemove:)`, handles mention, attachment and command chips. It shows a symbol, the label, an optional `KeyCap` and an `xmark` remove button on a readable capsule at e1; mention and command chips use the accent tint. Fixtures are in `PreviewState+Composer.swift` and snapshots in `ComposerMoleculeTests`.
Deviations: `Size.buttonHeightSmall` and `Space.xs` stand in for the 26 px height and 5 px gap. The symbols were picked in the task: `at`, `paperclip`, `bolt`.
Check: each new suite recorded its snapshots once and then passed without re-recording. After the last commit the full `swift test` ran twice: 71 tests in 21 suites passed both times. `swiftlint lint --strict` and `swift-format lint --strict` are clean. After merging into `p37-desktop`, `swift build --build-tests` succeeded.
Not done: none.

#### T37.21.10 `SettingRow`

Depends: T37.21 · Size: ~150 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Molecules/*`, its tests and snapshots
Goal: a Settings row — label, control and the source-layer badge (mockup `.group .gr`); DS§6.3 gains its row. Built only from atoms and foundations (DS§6.3).
Check: snapshot per variant × light/dark × Solid/Frosted; imports SwiftUI only.
Status: done 2026-09-28
Result: `CoxUI/Molecules/SettingRow.swift` shows a LabeledToggle, a LabeledSlider or a `SettingLabel` beside any control, followed by a Badge that names the source layer. `SettingSource` lists default, user, project, claude-settings, env and flag. A value from project, claude-settings, env or flag is read-only: the control is disabled and a lock with a tooltip sits before the badge. A unit test checks which layers lock a setting. DS§6.3 has a row for it.
Deviations: adds the `claude-settings` layer, which `cox-config` reports (D13). `SettingLabel.swift` is extracted from `LabeledToggle`; its snapshots are unchanged.
Check: each new suite recorded its snapshots once and then passed without re-recording. After the last commit the full `swift test` ran twice: 71 tests in 21 suites passed both times. `swiftlint lint --strict` and `swift-format lint --strict` are clean. After merging into `p37-desktop`, `swift build --build-tests` succeeded.
Not done: none.

#### T37.41 Cards as view-backed attachments

Depends: T37.40 · Size: ~150 · Files: `…/CoxTranscriptText/…`
Goal: tool, approval and subagent cards sit in the text as view-backed attachments hosting SwiftUI views; a card that collapses or expands keeps the text below it stable.
Check: a test expands and collapses a card and the range and frame of the next block stay correct; a drag across a card selects the card as one unit.
Status: done 2026-09-28
Result: in `CoxTranscriptText`, tool, toolGroup, approval, question and task blocks are each one `CardAttachment` character. The character hosts the caller's SwiftUI view: `TranscriptCards` via `TranscriptTextView.cards`, default `.summary`. The package still depends only on CoxClient. A card is as wide as the line and as tall as SwiftUI makes it at that width. When its height changes, only the card's range is invalidated and the viewport lays out again: text below moves, and every block range stays the same. New file `TranscriptCards.swift`.
Deviations:
- Question cards are included, to match T37.42's `MarkdownCopy.card`.
- TextKit creates a card's view only when it draws the line, so one extra viewport layout on the next run-loop turn places it.
- SwiftUI reports a shrink only through `layout()`, so both size paths are hooked.
- About 147 source lines in 3 source files, plus a test file and desktop.md.
Check: `TranscriptCardsTests`:
- expand and collapse move the next block by exactly 160 pt and back, with its range and the string unchanged;
- a real mouse drag across a card selects it whole;
- the expand test fails when the invalidation is removed.
`swift test` passed 3 runs. `swiftlint lint --strict` and `swift-format lint --strict` are clean.
Not done: none.

#### T37.43 Incremental text from `StyledDoc` spans

Depends: T37.40 · Size: ~200 · Files: `…/CoxTranscriptText/…`
Goal: the text storage is built from Rust `StyledDoc` spans (T37.7) and appended as patches arrive instead of rebuilt; the spike took ~630 ms to build 10 000 blocks at once, over the 400 ms launch budget (research.md §9.5.13).
Check: building the 10 000-block fixture incrementally stays within the DT§1 launch budget; a streamed `AppendText` patch edits only its block's range.
Status: done 2026-09-28
Result: a reply is built from its `StyledDoc` spans. Each `StyleToken` maps to `TranscriptStyle.colors`, falling back to `text`. `TranscriptTextView.apply(_:current:)` puts each patch (upsert, AppendText, DocTail, remove, reset) into its own block's range, one storage edit per batch. Separators keep the attributes of the block before them, so patched text matches a fresh load character for character. The build makes one attributed string per block from merged runs, and fonts are made once per style. `docStarts` keeps `MarkdownCopy.partial`'s layout. New file `TranscriptPatches.swift`. Also fixed: tearing down a view with 2 000 cards took about 220 s, so card relayout now skips views with no window.
Deviations:
- About 360 source lines in 5 files, against 200 lines and 3 files.
- Span `rgb` is ignored; syntax colours come from tokens. Using the real rgb is left to the creator (ideas.md).
Check: `swift test` passed 3 runs: 19 tests in 3 suites.
- `streamedAppendTextEditsOnlyItsBlocksRange` and `docTailEditsOnlyItsTail`: every storage edit stays inside its block, or its tail.
- `tenThousandBlocksBuiltPatchByPatchFitTheLaunchBudget`: 10 000 blocks in batches of 64 reach the first frame in 293–374 ms against 400 ms. Measured on the shared M3 Max at load 39–51; a whole `load` takes about 200 ms.
Both linters are clean.
Not done: the ≤400 ms budget was not measured on the M1 Air.

#### T37.17.1 High Contrast palette

Depends: — · Size: ~80 plus generated files · Files: `desktop/design/tokens/color.light-hc.json`, `desktop/design/tokens/color.dark-hc.json`, the generated outputs
Goal: the High Contrast appearances (A89) derived from the light and dark palettes by one rule. Text is at least 7:1 on its surface, hairlines and borders are solid and at least 3:1, glass opacity is raised and the specular sweep is off. The pipeline emits the HC variants into `Colors.xcassets` and `tokens.css`.
Check: `just desktop-tokens` emits both HC appearances; a test or script checks every HC text/surface pair at ≥7:1 and every border at ≥3:1; the `desktop-tokens` drift job is clean.
Status: done 2026-09-28
Result: `desktop/design/high-contrast.mjs` (node) derives `tokens/color.light-hc.json` and `tokens/color.dark-hc.json` from the light and dark palettes using A89's rule, then reads them back and checks them. It covers 166 declared pairs per mode:
- text at least 7:1;
- `separator` and `surface.capsuleBorder` solid and at least 3:1;
- context-bar segments and tile glyphs at least 3:1;
- glass keeps a quarter of its transparency.
A failing colour moves the smallest step toward black or white; a passing one is kept. A colour token no rule names fails the build.
`npm run build`, and with it `just desktop-tokens` and the CI drift job, runs the script before Style Dictionary; `npm run check` checks without writing. All 56 colorsets in `Colors.xcassets` get a High Contrast entry; `tokens.css` gets `.hc` and `.dark.hc` blocks; the DESIGN.md §8 line is extended.
Examples: light `text.tertiary` on window goes from 2.57 to 9.11; dark `syntax.comment` on `diff.del` from 3.68 to 7.97.
Deviations:
- A translucent surface is judged composited over its palette's opaque `surface.window`.
- Secondary and tertiary text end up almost identical in HC (light #48484b, dark #d2d2d5), because 7:1 applies to every text role.
- The white glyphs on the edit, search and write tiles cannot get lighter, so the tile tops darken instead.
Check: `just desktop-tokens` exits 0 and prints "166 pairs pass" per mode; a second run leaves no diff. A broken file fails the check with a named pair ("text.secondary on surface.window: 2.57:1 is below 7:1"). `Tokens.swift` is byte-identical; every Any and Dark colorset entry is JSON-identical to before; no snapshot is affected.
Not done: turning the specular sweep off and raising window opacity under Increase Contrast. These are `material.*` numbers in CoxUI's `Appearance`, not colour tokens, and move to T37.19.6.

#### T37.26 Appearance popover and live window material

Depends: T37.13, T37.22, T37.21.8 · Size: ~150 · Files: `…/Organisms/AppearancePopover.swift`, `…/Molecules/MaterialPicker.swift`
Goal: material, transparency, blur/reflection, depth and tint change the window live and persist through `[desktop.appearance]` (mockups 28–29); Reduce Transparency disables the controls and says why.
Check: snapshot per material; changing a slider writes the config through an intent; Reduce Transparency snapshot is Solid.
Status: done 2026-09-28
Result:
- `CoxUI/Organisms/AppearancePopover.swift` follows mockups 28–29. It sits on readable popover glass at e4 and holds:
  - a title with the ⌘⌥A key cap and a `MaterialPicker`;
  - sliders for transparency, blur ("Reflection" for Glossy) and Depth;
  - a wallpaper-tint toggle and a note.
- The popover takes plain `State` and reports one `Intent` per `[desktop.appearance]` key. `State.applied(to:)` and `State.apply(_:)` let the window follow a slider at once.
- `MainScreen` gains `appearance` state and an `.appearance(_)` intent, and shows the popover under the toolbar's Appearance button.
- Under Reduce Transparency the glass controls are disabled with a one-line reason, and the window renders Solid.
- `CoxModel/AppearanceSettings.swift` adds `AppearanceEdit`, which maps a change to its config key. `SettingsStore.apply(_:)` writes the change through `set`. `SettingsStore.appearance` reads the section back, including the blur range from the schema.
- Fixtures are in `Previews/PreviewState+AppearancePopover.swift`. DESIGN.md §6.4 has the row.
Deviations:
- The mockup's "Keep text panels readable" switch is left out: it has no config key, and DS§3.5 keeps text readable at every setting.
- Choosing Solid disables transparency and blur. Reduce Transparency leaves Depth enabled.
- The 1× window-snapshot helper moved into the shared `Snapshot.swift`, which gains a `reduceTransparency` flag.
- About 230 source lines in 5 files, against the card's ~150.
Check:
- There is no app target yet, so the config write is tested in CoxModel: `aSliderChangeWritesItsKeyThroughTheClient` round-trips `desktop.appearance.opacity=0.3` through the fixture client. CoxModel: 16/16.
- CoxUI has popover snapshots per material in each light/dark × Solid/Frosted cell, and main-screen snapshots per material. Reduce Transparency has its own snapshots, plus a pixel-equality test that a Frosted window under it draws exactly as Solid.
- The full CoxUI suite passed twice without re-recording: 98 tests in 32 suites.
- `swiftlint --strict` and `swift-format lint --strict` are clean.
Not done:
- Blur and tint are only saved. Drawing them, wiring intents to the store in the app, value texts, and dismissing the popover moved to T37.22.3.
- Binding ⌘⌥A moved to T37.22.2.
- The dimmed look for disabled controls is in T37.19.5.
- Disabling controls locked by a higher config layer moved to T37.22.3.

#### T37.30.1 Settings screen

Depends: T37.21.10 · Size: ~150 · Files: `…/Screens/SettingsScreen.swift`
Goal: the Settings screen from `CoxUI` molecules over `SettingsStore`: sidebar groups, a layer badge per value, a read-only project field that names the project file, secure fields for keys through `SecretStore`.
Check: snapshot of a setting overridden by the project layer (read-only, badge names the layer); editing a user value round-trips through the fixture client.
Status: done 2026-09-28
Result:
- `CoxUI/Screens/SettingsScreen.swift` is the Settings window.
  - A `SettingsSidebar` lists the DT§5.7 pages from General to Advanced. At its foot are the user and project files, each with its layer badge.
  - The main area has one `SettingsGroupBox` per config table of the selected page. Each setting is a `SettingRow` holding a LabeledToggle, LabeledSlider, CoxSegmented or the new `SettingField`.
  - A provider's box adds a secure key field that never shows a stored key.
  - The screen takes `SettingsScreenState` and emits `SettingsScreenIntent` (`select`, `set(key:, Edit)`, `storeKey`).
- `CoxModel/SettingsFields.swift` adds `SettingsStore.tables(in:)`. Each field gets a title, a detail line and a `SettingControl`. The detail names the project file when the project layer sets the value; otherwise it is the schema's help text.
- `SettingsStore.edit` types an edit by the field's kind.
- The mapping lives in CoxModel so it is unit-tested without views; CoxUI depends on no other cox package.
- DESIGN.md gains rows in §3.7, §6.3 (`SettingField`), §6.4 (`SettingsSidebar`, `SettingsGroupBox`) and §6.5.
Deviations:
- 8 source files instead of 1, because a screen may not style anything (DS§5).
- The sidebar uses plain SF Symbols, not the mockup's coloured tiles (no colour tokens for them).
- A slider sends an intent on every drag step, so the app coalesces the writes.
- The 1× window-snapshot helper moved into the shared `Snapshot.swift`. It was merged with T37.26's move into one signature: `size:` defaults to the mockup window, plus `reduceTransparency:` and `named:`.
Check:
- CoxModel `swift test`: 15/15, twice, including `aProjectValueIsReadOnlyAndNamesTheProjectFile` and `editingAUserValueRoundTripsThroughTheFixtureClient`. Only the in-memory `SecretStore` is used.
- CoxUI: `aSettingTheProjectOverridesIsReadOnlyWithItsLayer` × 4 cells. The full suite passed twice without re-recording, 97 tests.
- After merging with T37.26: CoxUI 104 tests in 34 suites and CoxModel 19 tests pass, nothing re-recorded.
- `swiftlint --strict` and `swift-format lint --strict` are clean.
Not done:
- Wiring into the app, in T37.22.3.
- List-shaped and open-shaped values are shown read-only.
- No Remove-key button yet (`SettingsStore.removeKey` exists).
- The coloured page tiles wait on a creator decision (ideas.md).

#### T37.30.2 The app's `AppHost` over the Keychain

Depends: T37.22 · Size: ~150 · Files: `desktop/macos/Packages/CoxPlatform/…`, the app target
Goal: a `CoxPlatform` `AppHost` whose `secret` reads `KeychainSecretStore` (and `notify`/`open_url` through AppKit), wired in the app target.
Check: a test with the injected in-memory keychain answers `secret` for a stored provider key and `nil` otherwise; no test touches the real keychain (A49).
Status: done 2026-09-28
Result:
- `CoxClient/Host.swift` adds the `PlatformHost` protocol and `HostNote`, an inbox item cut down to what a notification shows.
- `CoxPlatform/Host.swift` adds `MacHost`:
  - `secret` reads `KeychainSecretStore`; a Keychain error counts as no key.
  - `notify` posts through `UNUserNotificationCenter` and sets the Dock badge.
  - `open` goes through `NSWorkspace` for `http`/`https` only, because the URL comes from an MCP server or the model.
- `CoxCore/HostBridge.swift` adapts `PlatformHost` to the generated `AppHost` and maps `InboxItem` to `HostNote`. The app passes `HostBridge(MacHost())` to `LiveCoreClient`.
- With this layering CoxPlatform depends only on CoxModel and is tested without the XCFramework, and CoxCore never links AppKit (DT§4.4 bullet).
Deviations: the card's app-target wiring is deferred to T37.22.3, since the app target comes with T37.32. Three packages are touched, with new files only, plus one doc bullet.
Check:
- `secretAnswersTheStoredProviderKeyAndNilOtherwise` passes with the injected in-memory Keychain. No test touches the real Keychain.
- `swift test`: CoxPlatform 8/8, CoxModel 12/12, CoxCore 7/7. CoxCore ran against an XCFramework built once in the worktree and then deleted.
- `swiftlint --strict` and `swift-format lint --strict` are clean.
- After merging into `p37-desktop`, CoxModel's 19 tests pass.
Not done:
- `notify` and `open` are thin and untested: the notification centre needs an app bundle, and tests never post a notification or open a URL.
- Notification authorization is asked on the first `notify`. There is no delegate yet for clicks or foreground display.

#### T37.42.1 `SessionStore` keeps the reply text current while it streams

Depends: — · Size: ~60 · Files: `desktop/macos/Packages/CoxModel/Sources/CoxModel/SessionStore.swift`
Goal: `Block.assistant.text` follows every `docTail` patch, so a whole-reply copy mid-stream returns the current source.
Check: a test streams a scripted reply and copies it after every patch; each copy equals the text so far.
Status: done 2026-09-28
Result: after each `docTail`, `SessionStore` sets the reply's `text` to its doc rendered as Markdown. The text is always derived from the doc, so there is no second source of truth; the closing upsert still brings the real source. The doc-to-Markdown renderer moved from `MarkdownCopy` into `CoxModel/Sources/CoxClient/DocMarkdown.swift` (`StyledDoc.markdown`, `DocBlock.markdown`/`fence`, `Span.markdown`), and the store and Copy as Markdown share it.
Deviations: 3 source files because of the move. While the reply streams, its text is the doc's Markdown rather than the raw source, which Rust does not send until the end. The copy is checked in CoxModel through the store's reply text, which is what a whole-reply copy returns.
Check: `swift test` in CoxModel: 13/13. `aStreamedReplysTextIsTheTextSoFarAfterEveryDocTail` fails 3× without the fix. After merging into `p37-desktop`, CoxModel passes 20/20. `swiftlint --strict` and `swift-format lint --strict` are clean.
Not done: none.

#### T37.42.2 "Copy as Markdown" menu item and ⇧-click block selection

Depends: — · Size: ~100 · Files: `desktop/macos/Packages/CoxTranscriptText/…`
Goal: the transcript context menu offers "Copy as Markdown"; ⇧-click in the gutter selects whole blocks (DT§5.2).
Check: a test invokes the menu item and reads Markdown from the pasteboard; a ⇧-click from block 2 to block 4 selects exactly those three blocks.
Status: done 2026-09-28
Result:
- `MarkdownMenu.swift`: `menu(for:)` inserts "Copy as Markdown" after Copy. `copyAsMarkdown(_:)` writes the selection's `MarkdownCopy` Markdown as `.markdown` and `.string`. The item is disabled when nothing is selected.
- `BlockSelection.swift`: the gutter is the text container's leading inset.
  - A click in the gutter selects the block beside it.
  - A ⇧-click selects every block from the anchor to the clicked block. The anchor is the last gutter-clicked block if the selection still covers it, otherwise the block where the selection starts.
  - With `crossBlockSelection` off, the selection stays in the anchor block.
- `TranscriptTextView.swift` gains `gutterAnchor`, `markdownPasteboard` and one line in `make`.
Deviations: the gutter click is a gesture recognizer (`GutterClick`), not a `mouseDown` override: the override put `NSTextView` into its blocking tracking loop and hung the drag test. `SelectionTests`' `Host` and fixture are no longer private, so the new tests reuse them.
Check: `swift test --no-parallel` in CoxTranscriptText: 23/23. `BlockSelectionTests` send real `NSEvent` clicks:
- ⇧-click from block 2 to block 4 selects exactly those three blocks;
- ⇧-click from a caret extends to the clicked block;
- with the setting off, the selection clamps to one block;
- the menu item is reached through a real right-click and read back from a private named pasteboard.
The ⇧-click and menu tests fail with the extension disabled. `tenThousandBlocksBuiltPatchByPatchFitTheLaunchBudget` missed 400 ms once at load average 74, then passed alone and on a full rerun. Lints are clean.
Not done: DT§4.6's CoxTranscriptText row is not updated.

#### T37.22.1 Inspector as an overlay below 1280 pt

Depends: T37.26 · Size: ~80 · Files: `…/Screens/MainScreen.swift`, `…/Organisms/Inspector.swift`
Goal: below 1280 pt of window width the inspector floats over the transcript column instead of taking width from it.
Check: snapshots at 1440 and 1100 pt wide; the transcript column keeps its width at 1100 pt.
Status: done 2026-09-28
Result: when the window is narrower than 1280 pt, `MainScreen` draws the inspector over the transcript column, inset by `Size.paneGap` from the column's edges, instead of beside it. A `GeometryReader` around the shell supplies the window width. The 1280 pt threshold is a named constant because no token exists for it. DS§4 says the column keeps its width.
Deviations: none (`Inspector.swift` needed no change).
Check:
- New 1100 pt snapshot `mainScreenWithTheInspectorFloating`; the 1440 pt snapshots are unchanged.
- `InspectorOverlayTests` measures the column from inside its slot. At 1100 pt the width is the same with the inspector shown or hidden; at 1440 pt it shrinks by the inspector width plus one gap.
- CoxUI `swift test`: 107 tests in 35 suites pass. Lints clean.
Not done: none.

#### T37.22.2 Sidebar and inspector shortcuts; the bypass strip

Depends: T37.26 · Size: ~100 · Files: `…/Screens/MainScreen.swift`, `desktop/design/DESIGN.md`, `docs/design/desktop.md`
Goal: the sidebar and inspector toggles use the system `SidebarCommands`/`InspectorCommands` and their default shortcuts (A89), and DS§4 and DT§5 say the same; the bypass-mode strip sits under the toolbar (A89), and DS§4 says so. The Appearance popover's ⌘⌥A (DS) is bound too.
Check: snapshot with bypass on; the toolbar tooltips name the shortcuts; DS§4 and DT§5 agree.
Status: done 2026-09-28
Result:
- `ShellShortcut` (in `SessionToolbar.swift`) holds each key and its glyphs, so the app menu can reuse them.
- Shortcuts:
  - the sidebar buttons (toolbar and `Sidebar`) answer ⌃⌘S;
  - the inspector button answers ⌃⌘I;
  - the Appearance button answers ⌘⌥A, and its key cap now reads the same glyphs.
  Each tooltip names its key.
- ⌃⌘S and ⌃⌘I are the system defaults. Apple's `InspectorCommands` page gives ⌃⌘I. For the sidebar key, a test app with `SidebarCommands` and `InspectorCommands` was built against the macOS 27 SDK and showed both in its View menu.
- In Bypass mode the toolbar draws a 3 pt `status.danger` strip under the bar, lined up with the panes.
- DS§3.1, §4 and §6.4 and DT§5.1 and §5.5 now agree.
Deviations: the edits are in `SessionToolbar`, `Sidebar` and `AppearancePopover`, where the buttons are, not in `MainScreen`.
Check:
- New snapshots `mainScreenInBypass`: light-frosted, plus a folded dark-frosted one.
- `ShellShortcutTests` covers the keys and the tooltip strings.
- Full CoxUI run: 110 tests in 36 suites passed, with no existing snapshot re-recorded. Lints clean.
Not done: installing the menu commands. The system `SidebarCommands`/`InspectorCommands` act only on system-built panes, so the app replaces both menu groups with the same titles and keys through `ShellShortcut` (T37.22.3).

#### T37.23 Transcript view and the DT§9 benchmark gate

Depends: T37.22, T37.40–T37.43, T37.21.1–T37.21.6 · Size: split at claim · Files: `…/Organisms/TranscriptView.swift`, `…/Organisms/TurnView.swift`, `…/Organisms/ToolCard.swift`
Goal: lazy transcript from timeline patches with `UserBubble`, `ThinkingDisclosure`, `ToolCard`, `AssistantMessage` and `ApprovalCard` slots; text selection runs across blocks like a document (copy keeps block order and gives Markdown), and `cross_block_selection = false` clamps it to one block (A67); the DT§9 rendering bet is decided by its benchmark with selection on.
Selection engine (T37.37, `research.md` §9.5.13): our own TextKit 2 view — one `NSTextView` over the transcript with blocks as ranges and cards as view-backed attachments — from the package `CoxTranscriptText`; Textual was rejected.
Check: the benchmark in DT§9 passes its budget on a 2 000-block fixture; snapshots per block kind; a UI test drags a selection across three blocks and the pasteboard holds all three in order; with the setting off the same drag selects one block.
Status: done 2026-09-28
Result (split at claim into three parts, one commit each):
- T37.23.1 `ToolCard`: `CoxUI/Organisms/ToolCard.swift` is public and built from plain values. A running call shows its tail. A finished call folds its detail behind the chevron; opened, it shows a readable face. ToolHeader, IconTile, RiskChip, TerminalTail, DiffLineView and CodeRun are now public. 20 snapshots.
- T37.23.2 `TranscriptView`: the new package `Packages/CoxTranscript` joins CoxUI, CoxModel and CoxTranscriptText; CoxUI imports no cox package, and CoxTranscriptText knows nothing of CoxUI. It holds `TranscriptView(store:crossBlockSelection:approval:)` and `TranscriptCard`. CoxUI gains `TextColour` and `FontToken.nsFont`, and `SessionStore` gains `didApply`. Durations are formatted with the SwiftUI locale. DT§4.1, §4.6 and §6 and the DESIGN.md TurnView and TranscriptView rows are updated.
- T37.23.3 benchmark gate: DT§1 budgets are measured through `TranscriptView` on 2 000 blocks with real cards and a live cross-block selection:
  - scroll hitch time 0.00–0.04 % against 1 % (p99 about 16 ms);
  - streaming by `docTail` at 200 tok/s: 19–23 % busy against 25 %, max frame 6–8 ms against 16 ms.
Deviations:
- Fixed a bug: a card attachment with no image made TextKit draw its document placeholder under every card. The fix is an empty `image`; the snapshots fail without it.
- `TurnView` is not a separate view under A87.
- Approvals and questions use a caller slot until `ApprovalCard` (T37.27).
- The offscreen benchmark window is occluded, so the harness draws the text into a bitmap itself. While streaming it redraws only the growing paragraph; a full 800 pt redraw would add about 7 ms, about 40 % busy.
Check:
- CoxUI `swift test`: 92 tests in 31 suites, second run.
- CoxTranscript: 6 tests in 3 suites:
  - a drag across 3 blocks copies Markdown in order;
  - with `cross_block_selection` off the same drag selects 1 block;
  - store patches reach the text;
  - snapshots of every block kind, light and dark.
- CoxTranscriptText 19 and CoxModel 12 pass; linters are clean.
- `TranscriptBenchmarkTests` passed 5 of 6 runs at load average 35–108. The failure was one 645 ms frame at load 100.
- After merging into `p37-desktop`: CoxModel 20 and CoxTranscript 6 pass.
Not done: user bubble and thinking as TextKit fragments, structured diff hunks, restyle on text size, follow-tail scrolling and styled reply structure are T37.23.4–T37.23.8. The M1 Air XCTest run is in ideas.md.

#### T37.31 Onboarding and doctor checklist

Depends: T37.30.1, T37.30.2, T37.11 · Size: ~120 · Files: `…/Screens/OnboardingScreen.swift`
Goal: first run finds providers, checks the sandbox and shell env, and explains what is missing.
Check: snapshots for no-provider and all-green states.
Status: done 2026-09-28
Result:
- `crates/cox-session/src/doctor.rs` now holds the checks `cox doctor` and the app share: `CheckResult`, the provider-key check (with `check_api_keys_in` over the app's own key store), `check_sandbox` and `check_git`. They moved from `crates/cox/src/doctor.rs`, which imports them.
- `crates/cox-app/src/onboarding.rs` returns the checklist rows (provider key, git, sandbox, shell environment), each with ok/warn/fail and a one-line detail.
  - `App::checklist(cwd)` asks the host for the provider key.
  - `load_login_env` records its result for the shell row.
  - `App::config(cwd)` replaces the config loading `live.rs` did inline.
- `CoxUI/Screens/OnboardingScreen.swift` shows the "Open a project" step and the check rows, built from the new `Molecules/ChecklistRow.swift`. It emits `chooseFolder`, `openSettings` and `retry`.
- DESIGN.md §6.3 and §6.5, DT§5.8 and the AGENTS.md cox-session and cox-app rows are updated.
Deviations:
- About 11 files and 250 new lines (moved code not counted), against 1 file.
- `serde` (workspace version) is now a direct dependency of cox-session, because `CheckResult` is `cox doctor --json`'s row type.
Check:
- Snapshots `noProvider` and `allGreen` in all 4 cells, plus `checklistRows`, pass on a second run.
- Full CoxUI suite: 108/108.
- `cargo nextest run` on cox-session, cox-app and cox's doctor and deps tests: 120/120, then 21/21. Clippy `-D warnings` and `fmt --check` are clean.
- Swift lints are clean.
- `cox doctor` against a scratch `COX_HOME` prints the key, sandbox and git rows as before.
- After merging into `p37-desktop`, `swift build --build-tests` for CoxUI succeeds.
Not done:
- No `cox-ffi` export of `checklist`: its forwarding code is at 299 of D11's 300 lines. This goes to T37.22.3.
- The welcome header needs a title token and an app icon asset.
- The Claude-settings import row and dropping a folder onto the window are not included.

#### T45.1 A child inherits the parent's live permission mode


Model: Claude Code / opus-5.5 · Depends: - · Size: ~70 · Priority: P0 · Complexity: 3

Goal: a subagent is never wider than its parent at spawn time.

Files:
- `crates/cox-core/src/session.rs`
- `crates/cox-core/src/subagent.rs`

Steps:
1. `session.rs`: `pub(crate) async fn permission_mode(&self) -> PermissionMode` reading `Inner.permission_mode`.
2. `subagent.rs` `AgentTool::call`: after `let mut config = self.parent.config.clone();` set `config.permissions.mode = self.parent.permission_mode().await` (the child's `build` picks it up; grants are not inherited).
3. Tests: `child_inherits_parent_live_plan_mode`, `child_of_default_parent_does_not_run_auto`.

Check:
```bash
mise exec -- cargo nextest run -p cox-core child_inherits_parent_live_plan_mode child_of_default_parent
mise exec -- cargo nextest run --workspace
mise exec -- cargo clippy --workspace --all-targets -- -D warnings
mise exec -- cargo fmt --check
```

Done when: both tests pass (open question 9: this may deserve P0 outside P45).

Out of scope: per-agent overrides (T45.2).

Plan:
1. Tests first, both failing on `main`. `child_inherits_parent_live_plan_mode` (unit, `subagent.rs` `mod tests`): parent configured `auto`, `SetPermissionMode { Plan }`, one `agent` (explore) call; the `Recording` provider also keeps each request's volatile system block, and the child's (cheap-tier) request must say `permission_mode=Plan` (`context.rs` renders it from the child's `config.permissions.mode`, the same field `Session::build` seeds the engine mode from). `child_of_default_parent_does_not_run_auto` (`crates/cox-core/tests/subagent.rs`, inline scenario): parent configured `auto`, live `Default`, a `shell` child limited to the `touch` (`Risk::Write`) tool; after the parent's own `agent` approval the child's `touch` must raise `ApprovalRequired` labelled with the agent (under `auto` it ran unasked).
2. `session.rs`: `pub(crate) async fn permission_mode(&self)` reads `Inner.permission_mode`.
3. `subagent.rs` `AgentTool::call`: after `let mut config = self.parent.config.clone();` set `config.permissions.mode = self.parent.permission_mode().await`. Grants stay per session; `cox_permission::Engine` is untouched.
4. Verify: the two tests, then fmt, clippy, nextest.
Status: done 2026-09-28
Result: `AgentTool::call` (`crates/cox-core/src/subagent.rs`) now sets the child's `config.permissions.mode` from the parent's live mode (`Session::permission_mode`, new in `crates/cox-core/src/session.rs`) instead of copying the configured one, so `Session::build` seeds the child's engine mode and its volatile system block from what the parent runs under right now. Grants are not inherited; `cox_permission::Engine` is unchanged. No new dependency. 3 files, about 110 LOC including tests (6 in `session.rs`, 4 in `AgentTool::call`).
Check output:
- `child_inherits_parent_live_plan_mode` (unit, `subagent.rs`; the test `Recording` provider now also keeps each request's system blocks): failed before the fix (the child's request said `permission_mode=Auto`), passes after.
- `child_of_default_parent_does_not_run_auto` (`crates/cox-core/tests/subagent.rs`): failed before the fix ("the child's write ran without asking"), passes after.
- In the worktree: nextest 1316 passed, 4 skipped; fmt and clippy clean.
- Not run: the real binary. Headless runs cannot change the mode mid-session, so the bug needs the TUI's Shift+Tab; the core tests drive the same `SetPermissionMode` submission the TUI sends.
Follow-ups found (not in this card): a finished child woken by `TaskMessage` is restarted from its rollout (`subagent.rs` `restart`), and `History::from_events` always returns `PermissionMode::Default` because mode changes are not recorded, so a child of a plan-mode parent wakes in `Default`; the same applies to resuming any session. The parent's own volatile system block (`context.rs`) also renders `config.permissions.mode`, not the live mode.

#### T40.1 `cox_protocol::image`: sniff, cap and encode

- Model: Claude Code / opus-5.5 (card: sonnet)
- Depends: -
- Size: ~140
- Priority: P1
- Complexity: 2
- Goal: one pure helper decides whether bytes are an image cox accepts, and turns them into a checked `Attachment` or tool-output payload. Surfaces, `read` and the core share it, with no second check anywhere.
- Files: `crates/cox-protocol/src/image.rs` (new), `crates/cox-protocol/src/lib.rs`. Manifests: root `Cargo.toml`, `crates/cox-protocol/Cargo.toml`.
- Steps:
  1. `sniff(bytes) -> Option<&'static str>` by magic bytes: PNG `89 50 4E 47`, JPEG `FF D8 FF`, GIF `GIF87a`/`GIF89a`, WebP `RIFF....WEBP`.
  2. `pub const MAX_IMAGE_BYTES: usize = 3_750_000` (why: the smallest documented per-image limit, 5 MB base64; see the phase intro).
  3. `pub const IMAGE_TOKEN_ESTIMATE: u64 = 1600` (why: the standard-tier cap of 1568 visual tokens, rounded; provider-reported usage corrects it).
  4. `ImageError` (thiserror): `NotAnImage`, `TooLarge { bytes, cap }`, `MediaTypeMismatch { declared, sniffed }`, `BadBase64`.
  5. `attachment(name, bytes) -> Result<Attachment, ImageError>` and `validate(&Attachment) -> Result<(), ImageError>`. The latter decodes only enough to sniff, and checks the declared type and the decoded length.
  6. `to_structured(media_type, bytes) -> Value` and `take_structured(&mut ToolOutput) -> Option<(String, String)>`, keyed `structured["image"]`. `ToolOutput` has 69 literal constructions, so no new field.
  7. Base64: needs the new dependency `base64` (see Open questions). Alternative with no new dependency: move `base64_encode` out of `crates/cox-tui/src/term.rs:247` into this module, add a matching decoder, and have `term.rs` call it (3 files, ~40 LOC more).
- Check:
  ```bash
  mise exec -- cargo nextest run -p cox-protocol -E 'test(image)'
  ```
- Done when: there is a test per format, one for the cap, one for mismatch and one for bad base64. Any new dependency has its row in §1.1 and a reason in the commit.
- Out of scope: resizing or downscaling (no image crate; see open questions).
- Plan:
  1. Manifests: `base64 = "0.23"` in root `[workspace.dependencies]` (creator-approved, A81; already in `Cargo.lock` as 0.23.1, the latest on crates.io 2026-09-28), `base64 = { workspace = true }` in `cox-protocol`. Rows in §1.1 (`cox-protocol`, `cox-tui`) and `toolchain.md`; `rust.md` already lists `base64`.
  2. Tests first in `crates/cox-protocol/src/image.rs` against stub bodies, and watch them fail: one `sniff` case per format (PNG, JPEG, GIF87a, GIF89a, WebP), a non-image and a non-WebP RIFF, the cap (at the cap accepted, one byte over refused, an over-cap base64 refused before decoding), a declared/sniffed mismatch, bad base64, an `attachment` → `validate` round trip, and `take_structured` returning the pair and dropping an emptied payload while keeping other keys.
  3. Implement per steps 1–6. `validate` bounds the decoded length from the base64 length first, then decodes the whole string (allocation bounded by the cap), so bad base64 anywhere is caught, not only in the sniffed prefix. `pub mod image` plus its line in the `lib.rs` header.
  4. A81: a second small commit replaces `base64_encode` in `crates/cox-tui/src/term.rs` with `base64::engine::general_purpose::STANDARD` (+ `crates/cox-tui/Cargo.toml`); the known-vector test moves onto `copy` so the OSC 52 bytes stay identical. Separate because it would take the card past three files.
  5. Verify: the Check, then `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace`, `cargo deny check`.

Status: done 2026-09-28
Result: `crates/cox-protocol/src/image.rs` (new, 140 lines without tests): `sniff` by magic bytes, `MAX_IMAGE_BYTES`, `IMAGE_TOKEN_ESTIMATE`, `ImageError`, `attachment`, `validate`, `to_structured`, `take_structured` (keyed `structured["image"]`; an emptied payload becomes `None`). `validate` refuses an oversized image from the base64 length before allocating, then decodes the whole string, so bad base64 after a valid prefix is caught too; the declared type is compared case-insensitively. New workspace dependency `base64` 0.23 (0.23.1, already in `Cargo.lock` transitively; creator-approved in A81), rows in §1.1 and `toolchain.md`; `rust.md` already listed it. A81's replacement of the hand-rolled encoder in `crates/cox-tui/src/term.rs` is the next commit.
Deviations: step 5 — `validate` decodes the whole string instead of only the sniffed prefix (bounded by the cap), so a corrupt tail cannot reach a wire; same errors, same signature. The `term.rs` switch is a separate commit to keep this one within three code files.
Check output:
- `cargo nextest run -p cox-protocol -E 'test(image)'`: 18 passed — `sniff_names_each_accepted_format` (png, jpeg, gif87a, gif89a, webp), `sniff_refuses_anything_else` (text, empty, riff_wave, truncated_png), `attachment_at_the_cap_is_accepted_and_one_byte_over_is_too_large`, `attachment_refuses_bytes_that_are_not_an_image`, `attachment_round_trips_through_validate`, `validate_refuses_over_cap_base64`, `validate_refuses_a_declared_type_the_bytes_contradict`, `validate_refuses_bad_base64_even_after_a_valid_prefix`, `validate_refuses_encoded_bytes_that_are_not_an_image`, `take_structured_returns_the_image_and_drops_the_emptied_payload`, `take_structured_keeps_other_keys_and_ignores_outputs_without_an_image`. Against stub bodies 11 of them failed first.
- Workspace (with the `term.rs` switch applied): fmt and clippy `-D warnings` clean; `cargo deny check`: advisories, bans, licenses, sources ok; nextest 1331 passed, 1 failed, 4 skipped — the failure, `cox::subagent_messaging headless_run_does_not_wait_for_a_background_shell`, touches neither base64 nor images and passed 3 of 3 runs alone (timing under full-workspace load).

#### T40.4 `read` returns an image instead of refusing it

- Model: Claude Code / opus-5.5 (card: sonnet)
- Depends: T40.1
- Size: ~90
- Priority: P1
- Complexity: 2
- Goal: `read` on a confined path whose bytes sniff as an accepted image returns a short text line (`image/png, 48.2 KiB`) plus the image in `structured["image"]`. Over the cap it returns `ToolError::TooLarge { bytes, cap }`. Other binary files still return `ToolError::Binary`.
- Files: `crates/cox-tools/src/read.rs`. Docs: `docs/tools.md`, and the plan.md §1.11 `read` row ("images v0.2") is updated when the card closes.
- Steps:
  1. In `read.rs`, run `image::sniff` before the NUL-byte sniff. The path has already been confined by the existing `path::confine` call; no new guard.
  2. Build the output with `image::to_structured`. `mode`, `offset` and `limit` are ignored for images and said so in the text line.
  3. Update the tool description so the model knows images are readable.
  4. Tests: `read_png_returns_structured_image`, `read_oversized_image_is_too_large`, and keep `read_binary_file_is_rejected_with_binary_error`.
- Check:
  ```bash
  mise exec -- cargo nextest run -p cox-tools -E 'test(read_)'
  ```
- Done when: the tests pass. `docs/tools.md` states the cap and the four formats.
- Plan:
  1. Tests first in `crates/cox-tools/src/read.rs`: `read_png_returns_structured_image` (a tiny PNG; text line `image/png, …`, `image::take_structured` yields the same media type and base64 of the file), `read_oversized_image_is_too_large` (a PNG header padded to `MAX_IMAGE_BYTES + 1` → `TooLarge { bytes, cap }`), and keep `read_binary_file_is_rejected_with_binary_error`. Watch the first two fail on current code (they hit `Binary`/text).
  2. In `call`, after `confine` and the read, `image::sniff` the bytes before the NUL sniff. An image over `MAX_IMAGE_BYTES` → `ToolError::TooLarge`; otherwise text `<media_type>, <size>` (plus a note when `lines`/`mode` were passed, since they do not apply) and `structured = image::to_structured(..)`. No new path handling.
  3. Tool description: images (PNG, JPEG, GIF, WebP, up to the cap) are returned as images; other binaries are still refused.
  4. `docs/tools.md`: the `read` row and a line with the cap and the four formats. On close, the plan.md §1.11 `read` row drops "images v0.2".
  5. Verify: the Check; the real binary with the scripted provider reading a PNG under `COX_HOME=/tmp/cox-t40.4` if a scenario can drive `read`; then fmt, clippy `-D warnings`, workspace nextest.
- Out of scope:
  - The ACP `FsReadTool` swap (it reads through the editor's text API; images there stay unsupported and say so).
  - Forwarding the image to the model (T40.5).

Status: done 2026-09-28
Result: `crates/cox-tools/src/read.rs`: after `confine` and the read, `image::sniff` runs before the NUL sniff. An accepted image over `image::MAX_IMAGE_BYTES` returns `ToolError::TooLarge { bytes, cap }` before any encoding; otherwise the output is `<media type>, <size>` (e.g. `image/png, 16 B`, `48.2 KiB`), plus a note that `lines` and `mode` do not apply to images when either was passed, and `structured = image::to_structured(..)`. Other binaries still return `ToolError::Binary`. The tool description says images are returned. `docs/tools.md`: the `read` row plus a paragraph with the cap and the four formats. plan.md §1.11 `read` row: "images v0.2" replaced.
Deviations: the card names `offset` and `limit`; `read` has `lines` and `mode`, so the note names those. The cap is compared in `read.rs` against the shared `MAX_IMAGE_BYTES` (not through `image::attachment`) so an image is base64-encoded once, only after the check.
Check output:
- `cargo nextest run -p cox-tools -E 'test(read_)'`: 14 passed, among them `read_png_returns_structured_image`, `read_oversized_image_is_too_large`, `read_binary_file_is_rejected_with_binary_error`. Before the change the two new tests failed (a PNG with NUL bytes was refused as `Binary`).
- Real binary, scratch `COX_HOME=/tmp/cox-t40.4` (removed afterwards), scripted provider calling `read` on a 16-byte PNG, `--output-format stream-json`: `tool_call_done` with `"ok":true,"visible":"image/png, 16 B"`, run ended `done`, exit 0.
- Workspace: `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo nextest run --workspace`: 1336 passed, 4 skipped.

#### T50.1 Instruction files and the skills index reach system[2]

Model: Claude Code / opus-5.5 · Depends: — · Size: ~150 · Files: `crates/cox-core/src/context.rs`, `crates/cox-core/src/session.rs`, `crates/cox/src/session.rs` (the caller that already owns `cox_ext`)

Goal: the `AGENTS.md`/`CLAUDE.md` hierarchy (`cox_ext::instructions::load`) and the skills index are sent to the model in system[2]. Today system[2] is the `INSTRUCTIONS` constant ("Instruction-file stub until T7.1") in `context.rs`, `instructions::load` is called only by `cox ext` listing (`crates/cox/src/ext_cmd.rs`), and the core is handed an empty skills index. The loaded text is passed into the core as data (the core does no I/O), stays byte-stable for the whole session (cache-stable prefix, §1.9) and is not re-read mid-session.

Check: a test builds a request for a session opened on a scratch tree with an `AGENTS.md` and one skill and finds both texts in system[2]; a second turn's system[2] is byte-identical (`prefix_bytes_identical_between_turns` stays green); the test fails on current `main`. Run the real binary against a `COX_HOME` scratch tree with `--output-format stream-json` and a scripted provider (or the request dump) to see the text in the request.

Done when: the Check passes and the three AGENTS.md commands are clean.

Out of scope: re-reading instruction files mid-session; the repo map (P43).

Plan:
1. `context.rs`: `assemble_with_skills` gains `instructions: &str` (the `instructions::load` block) before `skills_index`; system[2] = the `INSTRUCTIONS` line, then the block, then the index, each joined by `\n` only when non-empty, so a tree with neither keeps today's bytes. The block joins under every profile (the user's rules); `minimal` still drops the index.
2. `cox-core/src/session.rs`: `Session::set_instructions(block, skills_index)`, set once by the surface like `set_agent_defs` (a later call is ignored, so system[2] cannot change mid-session); the one `assemble_with` call site passes both. A child shares the parent's block (not the index: its tools may lack `skill`).
3. `crates/cox/src/session.rs`: at session build, load the chain with `cox_ext::instructions::load` under `context.instruction_budget_tokens` (notices are warnings, D14) and build the index with `cox_ext::skills::index` before `SkillTool` takes the skills; call `set_instructions` for new and resumed sessions. `crates/cox/src/ext_cmd.rs` (a 4th file): its `Roots` construction becomes one shared `instruction_roots` helper so `cox ext` and the session read the same chain.
4. Test `instruction_files_and_skills_index_reach_system_two` in `crates/cox/src/session.rs`: scratch tree with `AGENTS.md` and one skill, a recording scripted provider, two turns; both texts in system[2], and system[0..=2] byte-identical between the turns. Fails on `main` (system[2] is the stub).
5. Real binary: `COX_HOME=/tmp/cox-t50.1 cargo run -- run -p … --output-format stream-json` with a scripted provider or the request dump from a scratch tree with an `AGENTS.md`; then the three AGENTS.md commands.

Status: done 2026-09-28
Result: `crates/cox/src/session.rs` reads the `AGENTS.md`/`CLAUDE.md` chain once at session build (`cox_ext::instructions::load` under `context.instruction_budget_tokens`, notices as warnings) and the skills index (`cox_ext::skills::index`) in `prefix_texts`, and hands both to the new `Session::set_instructions` (a `OnceLock`, so a second call cannot change the prefix mid-session). `context::assemble_with_skills` takes the block before the index: system[2] is the old `INSTRUCTIONS` line, then the block, then the index, each only when non-empty, so a tree with neither sends the old bytes. The block joins under every profile; `minimal` still drops the index. A subagent shares its parent's block but not the index (its tools may lack `skill`). The core still reads no files.
Deviations: 4 files instead of ≤3: `crates/cox/src/ext_cmd.rs` gives its `Roots` construction to a shared `instruction_roots`, so `cox ext` lists exactly the chain a session sends instead of a second copy. About 190 lines including the two tests. The `Recorder` test provider in `crates/cox/src/session.rs` lost its `plugins` feature gate so the new test can use it.
Check output:
- `instruction_files_and_skills_index_reach_system_two` (`crates/cox/src/session.rs`): a scratch tree with an `AGENTS.md` and one skill, a recording scripted provider, two turns; both texts in system[2], system[0..=2] byte-identical between the turns. Failed before the core change (system[2] was the stub line only), passes now.
- `instructions_precede_skills_index_and_survive_minimal` (`crates/cox-core/src/context.rs`) pins the order and the `minimal` rule; `prefix_bytes_identical_between_turns`, `skills_index_is_in_system_2` and `minimal_prefix_under_1000_tokens` stay green.
- Real binary, `COX_HOME=/tmp/cox-t50.1`, `--provider local run -p hi --output-format stream-json` against a local capturing HTTP stand-in: the request's system text carried `# Instructions`, the `AGENTS.md` body and `- greet: …` after the stub line. Scratch tree removed.
- nextest 1316 passed, 4 skipped; fmt and clippy clean.

#### T50.2 Permission-mode changes are recorded, so resume and a woken child keep the live mode

Model: Claude Code / opus-5.5 · Depends: — · Size: ~150 · Priority: P0 · Complexity: 3

Files:
- `crates/cox-protocol/src/types.rs`
- `docs/protocol.jsonschema` (generated)
- `crates/cox-core/src/rollout.rs`
- `crates/cox-core/src/session.rs`
- `crates/cox-core/src/subagent.rs`
- `crates/cox-tui/src/state.rs`
- `crates/cox-core/tests/subagent.rs`, `crates/cox-core/tests/resume.rs` (tests)

Goal: a mode change (`Submission::SetPermissionMode`, Shift+Tab) is written to the rollout, and `History::from_events` rebuilds the last recorded mode instead of always returning `PermissionMode::Default` (`rollout.rs` ~236). Then a resumed session comes back in the mode it had, and a finished child woken by `TaskMessage` (`subagent.rs` `restart`) is never wider than its parent: it takes the parent's live mode (T45.1), or its own recorded mode if that is narrower. Found by T45.1.

Check: a test switches a parent to Plan, runs a child to completion, wakes it with `TaskMessage` and asserts the child's write raises `ApprovalRequired`/is denied as in Plan; a resume test asserts the rebuilt `History.permission_mode` equals the last recorded mode. Both fail on current `main`. Older rollouts with no mode record still load (as `Default`).

Plan:
1. Tests first, failing on `main`. `woken_child_keeps_parent_plan_mode` (`crates/cox-core/tests/subagent.rs`): a `Default` parent approves a `shell` child limited to `touch` (`Risk::Write`), the child answers without writing, the parent switches to Plan, a `TaskMessage` wakes the child and its `touch` must be denied without an `ApprovalRequired` (today it asks, as in `Default`). `resume_restores_last_recorded_permission_mode` (`crates/cox-core/tests/resume.rs`): `SetPermissionMode` Plan then Auto, the rebuilt `History.permission_mode` is `Some(Auto)`. `old_rollout_without_mode_record_has_no_mode` (`rollout.rs`): no record reads as `None`.
2. `crates/cox-protocol/src/types.rs`: new `Event::PermissionModeChanged { mode }`, a roundtrip case; regenerate `docs/protocol.jsonschema` through its drift test. A typed event, not a parsed `Notice`, because the rollout is replayed by type.
3. `session.rs`: `SetPermissionMode` emits the new event (the human-facing `Notice` stays, so no surface changes); resume seeds the live mode with `history.permission_mode.unwrap_or(Default)`, today's behaviour for old rollouts.
4. `rollout.rs`: `History.permission_mode` becomes `Option<PermissionMode>`, the last recorded mode, `None` when the rollout never recorded one.
5. `subagent.rs` `restart`: the woken child runs in the parent's live mode, or its own recorded mode when that is narrower (a private `narrower`, width Plan < Default < Auto < Bypass, with a unit test). `cox_permission::Engine` is untouched.
6. Verify: the tests, fmt, clippy, nextest; the real binary resumed against `COX_HOME=/tmp/cox-t50.2` if a headless run can reach it. More than 3 files (protocol type, generated schema, two integration test files) because the record is a new wire event.

Done when: the Check passes and the three AGENTS.md commands are clean.

Out of scope: the model's view of the mode (T50.3).
Status: done 2026-09-28
Result: `Submission::SetPermissionMode` now also emits a new `Event::PermissionModeChanged { mode }` (`crates/cox-protocol/src/types.rs`; `docs/protocol.jsonschema` regenerated through its drift test), so the change lands in the rollout; the human-facing `Notice` stays. `History.permission_mode` (`rollout.rs`) is now `Option<PermissionMode>`: the last recorded mode, `None` for a rollout with no record. Resume seeds the live mode with `unwrap_or(Default)`, so rollouts written before this change load as before. `subagent.rs` `restart` gives a woken child the parent's live mode (`Session::permission_mode`, T45.1), or its own recorded mode when that is narrower (private `narrower`, Plan < Default < Auto < Bypass). The TUI's exhaustive event match ignores the new event (its `set_mode` already updated the status line). `cox_permission::Engine` is unchanged; no new dependency. 8 files, about 210 added lines, most of it tests and the generated schema: more than 3 files because the record is a new wire event (protocol type, generated schema, the TUI's exhaustive match) and the two Check tests live in two integration test files.
Check output:
- `woken_child_keeps_parent_plan_mode` (`crates/cox-core/tests/subagent.rs`): failed on `main` ("the woken child asked as in Default, not Plan"), passes after.
- `resume_restores_last_recorded_permission_mode` (`crates/cox-core/tests/resume.rs`): Plan then Auto, the rebuilt `History.permission_mode` is `Some(Auto)`. On `main` it does not compile (the field was a bare `PermissionMode`, always `Default`).
- `old_rollout_without_mode_record_has_no_mode` (`rollout.rs`) and `narrower_mode_is_the_less_permissive_of_the_two` (`subagent.rs`): pass.
- Real binary against `COX_HOME=/tmp/cox-t50.2` (removed afterwards), scripted provider: `cox --plain` with `/permissions auto` and one turn, then `cox run -p --continue --output-format stream-json` whose script calls `write`: the resumed run wrote the file. The same with `/permissions default`: the write was denied (headless approval `never`).
- In the worktree: nextest 1321 passed, 4 skipped; fmt and clippy clean.
Follow-ups found (not in this card): resume ignores the configured mode and `--permission-mode` entirely (it takes the rollout's mode, `Default` when none), so a session started in a non-default configured mode and never switched still resumes in `Default`; recording the initial mode at session start would close that. `cox --plain`'s status line keeps showing the configured mode after `/permissions` (`plain.rs` submits the change but never updates its own status mode). A woken child's volatile block still renders its spawn-time `config.permissions.mode` (the T50.3 fix covers it if it renders the live mode).

#### T39.1 Chat wire captures a tool call's thought signature

- Model: Claude Code / opus-5.5
- Status: done 2026-09-28
- Depends: T38.1 (Chat wire emits `ToolUseEnd`)
- Size: ~150
- Priority: P1
- Complexity: 3
- Goal: when a Chat Completions stream carries `extra_content.google.thought_signature` on a tool-call chunk, the stream emits one new `ProviderEvent::ToolUseSignature { signature }` between that call's `ToolUseStart` and `ToolUseEnd`, and `consume_provider` keeps it keyed by call id.
- Files: `crates/cox-protocol/src/types.rs`, `crates/cox-provider-openai/src/chat.rs`, `crates/cox-core/src/turn.rs` (plus the regenerated `docs/protocol.jsonschema`)
- Steps:
  1. Add `ProviderEvent::ToolUseSignature { signature: String }` in `types.rs`, with a doc comment saying it is opaque, is replayed only to the wire that produced it, and follows its `ToolUseStart`. Add an rstest case beside the existing `ProviderEvent` serde cases. Regenerate `docs/protocol.jsonschema` through its drift test.
  2. `chat.rs`: add `signature: Option<String>` and `wire_id: Option<String>` to `AccruedCall`. In `on_tool_call_chunk`, read `chunk["extra_content"]["google"]["thought_signature"]` (a string; the last one wins) and `chunk["id"]`.
  3. Robustness: `index` currently defaults to 0 when absent, which would merge parallel calls from a server that omits it. When a chunk has no `index` and carries a wire `id` different from the current call's `wire_id`, start a new call instead.
  4. `flush` emits `ToolUseSignature` after `ToolUseStart` and before the input delta when a signature was captured.
  5. `turn.rs`: add `signatures: HashMap<CallId, String>` to `Streamed` (it derives `Default`). The new match arm stores the signature under `current`'s id. This is the only exhaustive match on `ProviderEvent` outside the provider crates (checked with grep on 2026-09-28).
  6. Tests:
     - `chat_stream_emits_signature_between_start_and_end`, from a new fixture `fixtures/openai-chat/gemini-tool-signature.sse`.
     - `chat_stream_splits_calls_without_index_by_wire_id`.
     - A `consume_provider` unit test proving the signature lands in `Streamed.signatures`.
- Check:
  ```bash
  mise exec -- cargo nextest run -p cox-provider-openai -E 'test(signature) | test(without_index)'
  mise exec -- cargo nextest run -p cox-core -E 'test(consume_provider)'
  mise exec -- cargo nextest run -p cox-protocol
  ```
- Done when: the three tests pass and the schema drift test is green. done.md records that the field path is unverified until T39.7.
- Out of scope:
  - Putting the signature into history (T39.2) and replaying it (T39.3).
  - The Anthropic `signature_delta`, which is still dropped by `cox-provider-anthropic/src/stream.rs`; that stays as is.
- Execution plan:
  1. Tests first: fixture `fixtures/openai-chat/gemini-tool-signature.sse` (one `read` call whose chunk carries `extra_content.google.thought_signature`); `chat_stream_emits_signature_between_start_and_end` and `chat_stream_splits_calls_without_index_by_wire_id` in `chat.rs`; `consume_provider_keeps_signature_by_call_id` in `turn.rs`; a `provider_event_json_roundtrip` rstest in `types.rs` with the new variant. Confirm they fail (do not compile) on the current code.
  2. `types.rs`: `ProviderEvent::ToolUseSignature { signature }` with the opaque/replay-to-its-own-wire doc comment.
  3. `chat.rs`: `AccruedCall.{signature, wire_id}`; the field path is read in one helper, `thought_signature(chunk)`, commented as unverified until T39.7; an index-less chunk with a new wire id starts a new call; `flush` emits the signature after `ToolUseStart`.
  4. `turn.rs`: `Streamed.signatures`, stored under the current call's id.
  5. Verify: the card's Check, then fmt, clippy and the full nextest run. `docs/protocol.jsonschema` covers only `Event`/`Submission`, so its drift test should stay green unchanged.
- Check output:
  - `cargo nextest run -p cox-provider-openai -E 'test(signature) | test(without_index)'`: `chat_stream_emits_signature_between_start_and_end` and `chat_stream_splits_calls_without_index_by_wire_id` pass (2 passed).
  - `cargo nextest run -p cox-core -E 'test(consume_provider)'`: `consume_provider_keeps_signature_by_call_id` passes (1 passed).
  - `cargo nextest run -p cox-protocol`: 90 passed, including `provider_event_json_roundtrip` (3 cases) and `protocol_jsonschema_matches_committed_file`. `docs/protocol.jsonschema` covers only `Event` and `Submission`, so the new `ProviderEvent` variant leaves it unchanged.
  - Before the fix the new tests did not compile (no `ToolUseSignature` variant); by inspection, the old `index` default of 0 merged the index-less calls into one.
  - Workspace: nextest 1344 passed, 4 skipped; clippy `-D warnings` and `fmt --check` clean.
- Note: the field path `extra_content.google.thought_signature` is **unverified** until the live check in T39.7. It is read in one place, `thought_signature` in `crates/cox-provider-openai/src/chat.rs`, which says so in its comment. The fixture `fixtures/openai-chat/gemini-tool-signature.sse` encodes the same unverified path.

#### T41.2 LSP stdio framing and JSON-RPC client

- Model: Claude Code / opus-5.5 (card: sonnet)
- Depends: -
- Size: ~190
- Priority: P1
- Complexity: 3
- Goal: `lsp::client::Client` speaks `Content-Length` framed JSON-RPC over any `AsyncRead`/`AsyncWrite`, with requests (id → oneshot, per-call timeout), notifications out, a notification stream in, and a message-size cap.
- Files: `crates/cox-tools/src/lsp/client.rs` (new), `crates/cox-tools/src/lsp/mod.rs` (new, `mod` lines only), `crates/cox-tools/src/lib.rs`
- Steps:
  1. `read_message`/`write_message`: parse headers until `\r\n\r\n` and require `Content-Length`. Reject a body over `MAX_MESSAGE_BYTES = 16 MiB` with `LspError::TooLarge`.
  2. `Client::start(reader, writer)` spawns one reader task. Responses resolve pending oneshots. Server requests (for example `workspace/configuration`, `window/workDoneProgress/create`) get a `null` result or a `MethodNotFound` error so the server never blocks. Notifications go to an `mpsc`.
  3. `request(method, params, timeout)` and `notify(method, params)`.
  4. `LspError` (thiserror): `Io`, `Parse`, `TooLarge`, `Timeout`, `Closed` and `Server { code, message }`.
  5. Tests over `tokio::io::duplex`:
     - `framing_round_trips`
     - `oversized_message_is_rejected`
     - `server_request_is_answered`
     - `request_times_out`
     - `closed_pipe_fails_pending_requests`
- Check:
  ```bash
  mise exec -- cargo nextest run -p cox-tools -E 'test(lsp::client)'
  ```
- Done when: all five tests pass, with no `unwrap` outside the tests.
- Plan:
  1. Tests first in `crates/cox-tools/src/lsp/client.rs` against stub bodies: the five card tests plus the framing edge cases — `split_headers_and_back_to_back_messages_are_framed` (bytes written in small chunks across the header boundary, two messages in one write, header name case-insensitive, `Content-Type` ignored), `partial_message_is_closed`, `missing_or_bad_content_length_is_a_parse_error`, `responses_match_ids_out_of_order` (with an error response → `Server { code, message }`), `notifications_reach_the_stream`. Watch them fail.
  2. `read_message` over `AsyncBufRead`: header lines read through a bounded `take` (a line with no `\n` within 8 KiB is `Parse`), clean EOF before a message is `Ok(None)`, EOF mid-message is `Closed`, a length over `MAX_MESSAGE_BYTES` is `TooLarge` before any body is read. `write_message` writes header, body and flushes.
  3. `Client::start(reader, writer) -> (Client, UnboundedReceiver<Notification>)`: one reader task; responses resolve the pending oneshot by integer id; server requests are answered (`workspace/configuration` → one `null` per item, `window/workDoneProgress/create`, `client/(un)registerCapability`, `window/showMessageRequest` → `null`, anything else → `-32601`); on EOF or a framing error the pending map is closed and emptied, so waiters get `Closed`. The notification channel is unbounded on purpose: a bounded one would stall the reader, and with it every response, while the consumer awaits a request (T41.4 drains it). `request` removes its entry and sends `$/cancelRequest` on timeout; `params: null` is omitted from the wire. `Drop` aborts the reader task.
  4. Wiring: `lsp/mod.rs` (`pub mod client;`), `pub mod lsp;` in `lib.rs`, `thiserror` (workspace dependency, already in §1 and `toolchain.md`) added to `crates/cox-tools/Cargo.toml` for `LspError`; the §1 `cox-tools` row gains the LSP client and `thiserror`. Four files because the crate had no `thiserror` yet.
  5. Verify: the Check, then fmt, clippy `-D warnings`, workspace nextest.
- Out of scope: process spawning and the document protocol (T41.4).

Status: done 2026-09-28
Result: `crates/cox-tools/src/lsp/client.rs` (new): `read_message`/`write_message` (`Content-Length` framing; header lines bounded to 8 KiB through `take`; clean EOF between messages is `Ok(None)`, EOF inside one or a broken pipe is `Closed`; a body over `MAX_MESSAGE_BYTES` = 16 MiB is `TooLarge` before it is read, and on write too), `LspError` (`Io`, `Parse`, `TooLarge`, `Timeout { method }`, `Closed`, `Server { code, message }`), `Notification`, and `Client::start(reader, writer) -> (Client, UnboundedReceiver<Notification>)` with one reader task. Responses resolve the pending oneshot by integer id (unknown ids ignored); server requests are answered from their own task (`workspace/configuration` → one `null` per item; `window/workDoneProgress/create`, `client/(un)registerCapability`, `window/showMessageRequest` → `null`; anything else → `-32601`); notifications go to the stream, which ends with the connection. `request` sends `$/cancelRequest` after a timeout; `null` params are omitted. `crates/cox-tools/src/lsp/mod.rs` (new, `pub mod client;`), `pub mod lsp;` in `lib.rs`, `thiserror` (workspace dependency, already in §1 and `toolchain.md`) added to `crates/cox-tools/Cargo.toml`; the §1 `cox-tools` row names the client and `thiserror`.
Deviations: four files, since the crate had no `thiserror` yet. Size: ~290 non-comment lines after rustfmt (the card estimated ~190) — the EOF/size handling and the reply, outcome and envelope helpers; kept in one module because they are one wire. The notification channel is unbounded (a bounded one would stall the reader, and with it every response, while the consumer awaits a request); T41.4 drains it.
Check output:
- `cargo nextest run -p cox-tools -E 'test(lsp::client)'`: 10 passed — `framing_round_trips`, `split_headers_and_back_to_back_messages_are_framed`, `partial_message_is_closed`, `missing_or_bad_content_length_is_a_parse_error`, `oversized_message_is_rejected`, `server_request_is_answered`, `request_times_out`, `closed_pipe_fails_pending_requests`, `responses_match_ids_out_of_order`, `notifications_reach_the_stream`. Against stub bodies 9 failed first (the parse-error test passed only because the stub returned `Parse`).
- Workspace: `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo nextest run --workspace --no-fail-fast`: 1346 passed, 4 skipped. A first fail-fast run stopped on `cox::subagent_messaging headless_run_does_not_wait_for_a_background_shell` (also failed 3 runs alone at 10-20 s under machine load, then passed alone in 2.8 s and in the full run); it touches no LSP code.

#### T44.1 `agent(isolation: "worktree")` asks before it adds a worktree

Model: Claude Code / opus-5.5 · Status: done 2026-09-28 · Depends: - · Size: ~60 · Priority: P1 · Complexity: 2

Goal: fix the gate violation — today an `explore` child with worktree isolation is `Risk::ReadOnly`, so it runs `git worktree add` unasked in every mode, even plan.

Files:
- `crates/cox-core/src/subagent.rs`
- `docs/tools.md`

Steps:
1. `AgentTool::risk`: when `input.isolation == "worktree"`, return `Risk::Destructive` (asks in default/auto, denied in plan, allowed only in bypass or by an allow rule / session grant on `agent(<name>)`). The Engine stays the only decision point; the tool does not check permission itself.
2. `docs/tools.md`: the `agent` row says worktree isolation asks.
3. Tests: `worktree_isolation_asks_in_default_mode`, `worktree_isolation_is_denied_in_plan_mode`, `worktree_isolation_respects_an_allow_rule`.

Check:
```bash
mise exec -- cargo nextest run -p cox-core worktree_isolation_
mise exec -- cargo nextest run --workspace
mise exec -- cargo clippy --workspace --all-targets -- -D warnings
mise exec -- cargo fmt --check
```

Done when: the three tests pass (open question 3: `Destructive` vs `Exec`).

Plan:
1. Tests first in `subagent.rs` `mod tests`: build the `ToolCall` from `AgentTool::risk`/`subject` for `{"task":"x","isolation":"worktree"}` and feed it to `cox_permission::Engine::decide` — `Ask` in default and auto, `Deny` in plan, `Allow { by: Rule }` with an `agent(explore)` allow rule. Confirm the default/auto/plan ones fail on current code (explore is `ReadOnly`).
2. `AgentTool::risk`: after resolve, `isolation == "worktree"` returns `Risk::Destructive` (above `Exec` for an external agent too); no permission check in the tool.
3. `docs/tools.md`: the `agent` row says worktree isolation is `destructive` and asks.
4. Verify: `cargo nextest run -p cox-core worktree_isolation_`, then fmt, clippy, the workspace suite.

Out of scope: removing the isolation option.

Result:
- `AgentTool::risk` (`crates/cox-core/src/subagent.rs`) returns `Risk::Destructive` when `isolation` is `"worktree"`, checked after `resolve` and before the external-agent and max-of-tools branches, so it wins over `Exec` too. The Engine stays the only decision point.
- `docs/tools.md`: the `agent` row says worktree isolation is Destructive and asks (denied in plan).
- Tests (unit, `subagent.rs`, the `ToolCall` built from `risk`/`subject` fed to `Engine::decide`): `worktree_isolation_asks_in_default_mode` (default and auto → `Ask(Risk Destructive)`), `worktree_isolation_is_denied_in_plan_mode` (also pins `isolation: "none"` to `ReadOnly` for explore), `worktree_isolation_respects_an_allow_rule` (`agent(explore)` → `Allow { by: Rule }`). Before the fix the first two failed (`Allow { by: Policy }`); the allow-rule one passed on old code too, since a read-only call was allowed anyway.

Deviations:
- A third file: `crates/cox-core/tests/subagent.rs` — `subagent_worktree_isolation_runs_child_in_its_worktree` now needs approval, so it sets the allow rule `agent(shell)` for both of its sessions (the proof the rule path works end to end).

Check:
- `cargo nextest run -p cox-core worktree_isolation`: 4 passed (the three new tests plus the updated integration test).
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`: clean. Workspace nextest (run before the creator's new "no full suite" rule arrived): 1352 passed, 4 skipped.
- Real binary, scratch `COX_HOME`, scripted `subagent_worktree` scenario in a scratch git repo: `cox run -p` in default mode reports `risk: "destructive"` and denies (headless approval policy `never`); `--permission-mode plan` denies with the plan-mode reason; `git worktree list` shows no worktree was added.

#### T50.4 Resume keeps the session's starting permission mode

Model: Claude Code / opus-5.5 · Depends: — · Priority: P1 · Complexity: 3 · Size: ~120 · Files: `crates/cox-core/src/session.rs`, `crates/cox-core/src/rollout.rs`, `crates/cox/src/session.rs` (or wherever resume applies the flag layer)

Goal: a session started in Plan or Auto through config or `--permission-mode` and never switched comes back in that mode on resume, not in `Default` (a Plan session must never resume wider). The session records its starting mode when it opens (the T50.2 `Event::PermissionModeChanged`, or the same record), so `History.permission_mode` is always `Some` for new rollouts. On resume, an explicit `--permission-mode` flag wins; otherwise the recorded mode; a rollout with no record at all falls back to the configured mode. Found by T50.2.

Check: a test opens a session configured `plan`, runs a turn without switching, resumes, and asserts the resumed session denies a write as Plan does; a second test resumes with an explicit `--permission-mode auto` and gets Auto; both fail on current `main`. Old rollouts still load.

Plan:
1. Tests first, failing on `main`. `crates/cox-core/tests/resume.rs`: `resumed_plan_session_denies_a_write_as_plan_does` (a session configured `plan` runs a turn without switching, is resumed under a `Default` config, and its `touch` is denied without an `ApprovalRequired`; on `main` it asks, as in `Default`) and `resume_without_a_mode_record_uses_the_configured_mode` (the same rollout with every `PermissionModeChanged` dropped, i.e. an old rollout, loads and resumes in the configured `plan`). `crates/cox/tests/run_cli.rs`, real binary with the scripted provider: `resume_with_an_explicit_permission_mode_flag_uses_it` (started `--permission-mode plan`, resumed with `--permission-mode auto`, the write lands) and `resume_without_a_flag_keeps_the_recorded_mode` (started `--permission-mode auto`, resumed without the flag, the write lands).
2. `crates/cox-core/src/session.rs` `build`: a top-level session appends `Event::PermissionModeChanged { mode }` for the mode it opens in to its rollout right after `SessionStarted`, fresh or resumed, so every new rollout has a record and a flag override on resume is recorded too. Rollout only, like the persisted `SessionStarted`: the surfaces already know the opening mode from the config they built the session with. Resume takes `history.permission_mode`, else `config.permissions.mode` (was `Default`). Children are unchanged (their mode is the parent's, T45.1/T50.2).
3. `crates/cox/src/session.rs` `open`: on resume an explicit `--permission-mode` replaces the recorded mode (`history.permission_mode = Some(flag mode)`); otherwise the recorded mode, else config. The resolved mode is written back into `loaded.config.permissions.mode`, so the TUI and `--plain` show the mode the session actually runs in. `cox_permission::Engine` is untouched.
4. Verify: the tests, fmt, clippy, nextest; the real binary against `COX_HOME=/tmp/cox-t50.4` (a Plan-configured session resumed without the flag stays in Plan), removed afterwards. Four code/test files rather than three: the Check needs both a core test and a binary test for the flag.

Done when: the Check passes and the three AGENTS.md commands are clean.

Out of scope: the `--plain` status line (T50.5).
Status: done 2026-09-28
Result: a top-level session (`Session::build`, `crates/cox-core/src/session.rs`) now appends `Event::PermissionModeChanged { mode }` for the mode it opens in to its rollout right after `SessionStarted`, fresh or resumed, so every new rollout carries a record and a flag override on resume is recorded too. Rollout only, not the surface stream, like the persisted `SessionStarted`. A rollout with no record resumes in the configured mode (was `Default`). Children are unchanged. `crates/cox/src/session.rs` `open` resolves the resume mode as explicit `--permission-mode`, then the recorded mode, then config, writes it into `History.permission_mode`, and writes it back into `loaded.config.permissions.mode` so the TUI and `--plain` start by showing the mode the session actually runs in. `cox_permission::Engine` and `rollout.rs` are unchanged (`History::from_events` already takes the last record); no new dependency. 4 files, 167 added lines, 128 of them tests: four files rather than three because the Check needs both a core test and a binary test for the flag.
Check output:
- `resumed_plan_session_denies_a_write_as_plan_does` and `resume_without_a_mode_record_uses_the_configured_mode` (`crates/cox-core/tests/resume.rs`): failed on `main` (the resumed session asked, as in `Default`), pass after.
- `resume_with_an_explicit_permission_mode_flag_uses_it` and `resume_without_a_flag_keeps_the_recorded_mode` (`crates/cox/tests/run_cli.rs`, real binary, scripted provider): failed on `main` (the resumed write was denied), pass after.
- Real binary against `COX_HOME=/tmp/cox-t50.4` (removed afterwards), scripted provider: a session run with `[permissions] mode = "plan"` in config, resumed with `cox run -p --resume <id>` whose script calls `write`: denied with the plan-mode message both with the config still in place and with it removed; the file was not written.
- In the worktree: `cargo nextest run --workspace` 1347 passed, 4 skipped; `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean.
Notes: a session started with `--permission-mode bypass` now resumes in Bypass without the flag (the recorded mode wins, as it already did for a `/permissions bypass` switch since T50.2). In the TUI, switching to another session reuses the launch's `--permission-mode` flag, which then wins over that session's record.

#### T50.6 `headless_run_does_not_wait_for_a_background_shell` is not timing-flaky

Model: Claude Code / opus-5.5 · Depends: — · Size: ~60 · Files: the test file that holds it (`crates/cox/tests/subagent_messaging.rs`), plus the code under test only if the test exposes a real bug

Goal: the e2e test fails under full-workspace load (seen by T40.1 and T41.2 on 2026-09-28: 3 of 3 failures when run alone under load at 10–20 s, passes in ~3 s when idle). Find whether it is a fixed wall-clock bound, a race with the detached shell's teardown (T38.2 changed session-end cancellation), or a real bug; make the test wait on an event or a deadline that holds under load, never on a fixed sleep; fix the code instead if it is a real bug.

Check: the test passes 20 times in a row under load (e.g. `cargo nextest run --workspace` in parallel with a second nextest run, or `stress`-style repeat with `--test-threads` high); the root cause is written in the done.md entry.

Done when: the Check passes and the three AGENTS.md commands are clean.

Out of scope: other slow tests.

Plan:
1. Reproduce first: build the test binary, run the test in a loop (`--test-threads` high, several copies at once) while the machine is under the parallel agents' build load, and record which assertion fails (the 10 s `elapsed` bound, the machine-wide `pgrep -f "sleep 4001"` leak check, or the 30 s `run_scripted` timeout) and where the run spends its time (process start, turns, `end()` + `wait_tasks_cleared(SHELL_CANCEL_GRACE)`).
2. Write the root cause down here before changing anything.
3. Fix at the responsible layer: in `crates/cox/tests/subagent_messaging.rs`, replace any fixed wall-clock bound that load can break with a bound that holds under load and still proves the claim (the shell sleeps for 4001 s, so "did not wait" is any exit far below that), and make the leak check see only this run's process (a command line unique to the run, like T38.2's `sleep 4011.<pid>`, polled with a deadline). If the repro shows a real bug in `crates/cox/src/run.rs` or `crates/cox-core/src/tasks.rs` (e.g. the shell outliving the run), fix the code instead and keep the test strict.
4. Verify: the test 20 times in a row under load, then fmt, clippy, and nextest on `-p cox` (creator rule 2026-09-28: no whole-workspace runs for checks).

Root cause (reproduced before any change, load average 60–86 on 16 cores from the parallel agents' builds): 8 concurrent copies of the test, 3 rounds — round 1 8/8 `cox did not finish within 30s`, round 2 8/8 and round 3 3/8 `headless run waited on a background shell task … 10.0–17.8 s`; the leak check never fired and no `sleep 4001` was left behind. Timestamping every stream-json line of the real binary (same scenario, 8–24 copies at once) shows where the time goes: `session_started` at 0.3–2 s (9 s on the first exec of a freshly linked 166 MB debug binary), then 1–5.3 s between `tool_call_requested` and `task_created` — the pre-call workspace checkpoint (`checkpoint::before` → `GitCheckpointer::snapshot`: `git init`, `rev-parse`, `add -A`, `write-tree`, each a process spawn under load) — then 0.3–3.7 s to exit (turn 2, `end()`, the SIGTERM, and the post-kill `checkpoint::after` snapshot and archive row that `wait_tasks_cleared` waits for, capped by `SHELL_CANCEL_GRACE`). Idle, the whole run takes 1–1.6 s. So both wall-clock bounds (10 s `elapsed`, 30 s `run_scripted` timeout) time process start-up and git spawns, not the claim: a run that waited on the shell would take 4001 s. Not a code bug: the shell is always killed (no leftover process in any run, including the 30 s timeouts, which were killed before the shell ever started). A second latent flake: `pgrep -f "sleep 4001"` is machine-wide, so another worktree running the same test at the same moment makes `leaked` true — shown by starting an unrelated `/bin/sleep 4001` and running the unchanged test, which then failed in 0.31 s with "a `sleep 4001` process outlived the headless run".
Status: done 2026-09-28
Result: test-only fix in `crates/cox/tests/subagent_messaging.rs` (and the scenario's comment). `headless_run_does_not_wait_for_a_background_shell` no longer asserts `elapsed < 10 s` or runs under the shared 30 s `run_scripted` timeout; its bound is `DID_NOT_WAIT` = 300 s, derived from the claim (a run that waited on the shell lasts 4001 s), which is 5× the slowest run seen at load average 180. The leak check now looks for this run's own command line: the test copies the scenario into its `COX_HOME` tempdir with `sleep 4001` rewritten to `sleep 4001.<test pid>`, asserts `task_created` carries that command, polls `pgrep -f 'sleep 4001\.<pid>'` until a 10 s deadline (the killed `sleep` is reaped asynchronously), and a `KillOnDrop` guard `pkill`s the pattern when the test ends, pass or panic. No product code changed: the repro showed no real bug.
Deviations: none in scope. Found, not fixed: (1) removing `session.end()` from `run.rs` still passes this test (run 7.8 s, no leftover `sleep`): when cox exits, the shell's PTY master closes and the kernel hangs up the shell's session (SIGHUP), so an ordinary `sleep` dies either way; only a SIGHUP-ignoring child would show the difference, which `crates/cox-core/tests/bash_tasks.rs` (T38.2) covers at the core level. (2) The post-kill `checkpoint::after` workspace snapshot of a detached shell runs inside the exit's `wait_tasks_cleared(SHELL_CANCEL_GRACE)` window; under heavy load it can use up the 5 s grace, after which `shutdown_background` drops the pending snapshot, archive row and `TaskCompleted` (no process leaks: the shell is already dead by then). (3) The other three tests in the file keep the 30 s `run_scripted` bound, which the same load spike (8/8 runs past 30 s) could exceed; out of scope per the card.
Check output:
- Before the change, load average 60–86: 8 concurrent copies × 3 rounds → 19 of 24 failed (8 × `cox did not finish within 30s`, 11 × `waited on a background shell task` at 10.0–17.8 s); a live unrelated `sleep 4001` → failed on the leak check.
- After: 20 rounds × 8 concurrent copies (160 runs) plus 24 `yes` CPU burners, load average 100–180: 160/160 passed, slowest 62 s, no leftover `sleep 4001*`.
- `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo nextest run -p cox` 170 passed, 1 skipped (only `crates/cox` tests changed, so per the creator's 2026-09-28 rule no whole-workspace run).

#### T39.2 Core keeps a tool call's signature in history and the rollout


- Model: Claude Code / opus-5.5
- Status: done 2026-09-28
- Depends: T39.1
- Size: ~170
- Priority: P1
- Complexity: 4
- Goal: a signature captured in T39.1 lives in history as `Content::Thinking { text: "", signature: Some(sig) }` directly before its `Content::ToolUse`, and the rebuild after resume produces the same messages (§1.15 invariant 6).
- Files: `crates/cox-core/src/session.rs`, `crates/cox-core/src/turn.rs`, `crates/cox-core/src/rollout.rs`
- Steps:
  1. `session.rs` (the assistant-message build, ~line 1610): for each call, push the signed `Content::Thinking` right before its `Content::ToolUse` when `streamed.signatures` has the call id. Pass the signatures to `run_tools`.
  2. `turn.rs` `run_tools`: right before `Event::ToolCallRequested` for a call that has a signature, emit `ItemStarted`/`ItemDone` with `ItemKind::Thinking { text: String::new(), signature: Some(sig) }`, so the rollout gets it in the same order as the live history.
  3. `rollout.rs`: an `ItemKind::Thinking` item with a signature appends `Content::Thinking` to the last assistant message. Reuse the shape of `append_tool_use` through one shared `append_assistant_block` helper, not a second copy. Unsigned thinking items stay ignored as today.
  4. Tests:
     - `signed_tool_call_keeps_signature_before_its_tool_use` (live history).
     - `resume_rebuilds_signed_thinking_before_tool_use` (rollout).
     - The existing `resume_builds_identical_request` extended with a scripted turn that carries a `ToolUseSignature`.
  5. Confirm that `router::strip_thinking` and `strip_thinking_before` already drop these blocks on a model switch, and add one assertion that proves it.
- Check:
  ```bash
  mise exec -- cargo nextest run -p cox-core -E 'test(signature) | test(resume_builds_identical_request) | test(strip_thinking)'
  ```
- Done when: live history and the rebuilt history are equal for a signed tool round. The scripted provider can emit `ToolUseSignature` (a scenario key, only if the scenario format needs one; otherwise a hand-built event list in the test).
- Out of scope:
  - Wire translation (T39.3) and surface rendering (T39.4).
  - Signatures on plain text parts (Gemini may send them on non-tool responses; the loop does not need them).
- Execution plan:
  1. Tests first. `rollout.rs`: `resume_rebuilds_signed_thinking_before_tool_use` (hand-built events: a signed `ItemKind::Thinking` item before each `ToolCallRequested`, plus an unsigned one that stays ignored). `crates/cox-core/tests/resume.rs`: a test-only `Signed` provider that wraps `Scripted` and inserts `ToolUseSignature` after every `ToolUseStart` (a hand-built event stream, so the scenario format needs no new key); `signed_tool_call_keeps_signature_before_its_tool_use` (live history has the signed block right before its `ToolUse`, and `router::strip_thinking` drops it) and `resume_builds_identical_request_with_signature` (the existing test's body, shared through one helper, run with `Signed`). Confirm they fail on the current code.
  2. `turn.rs`: `run_tools` stays the entry point for its other callers and delegates to a new `run_signed_tools(session, turn, calls, &signatures)`, which emits `ItemStarted`/`ItemDone` with `ItemKind::Thinking { text: "", signature }` right before a signed call's `ToolCallRequested`.
  3. `session.rs`: the assistant-message build pushes the signed `Content::Thinking` before each signed call's `ToolUse` and calls `run_signed_tools` with `streamed.signatures`.
  4. `rollout.rs`: `append_tool_use` becomes a caller of one shared `append_assistant_block`; a finished signed `ItemKind::Thinking` item appends through it.
  5. Verify: the card's Check, fmt, clippy, `cargo nextest run -p cox-core` (history build, rollout and resume all live there).
- Result:
  - `turn.rs`: `run_tools` now delegates to `run_signed_tools(session, turn, calls, &signatures)`, which emits an `ItemStarted`/`ItemDone` pair with `ItemKind::Thinking { text: "", signature: Some(sig) }` right before a signed call's `ToolCallRequested`. The other callers (`init.rs`, `plugin_model.rs`, `user_shell`) keep calling `run_tools` unchanged.
  - `session.rs`: the assistant-message build pushes `Content::Thinking { text: "", signature: Some(sig) }` right before each signed call's `ToolUse` and runs the batch through `run_signed_tools` with `streamed.signatures`.
  - `rollout.rs`: `append_tool_use` now goes through one shared `append_assistant_block`; a finished `ItemKind::Thinking` item with a signature appends `Content::Thinking` through it. Unsigned thinking items are still ignored.
  - `router::strip_thinking` (and `context::strip_thinking_before`, which calls it) already drops these blocks, since it matches every `Content::Thinking`; `signed_tool_call_keeps_signature_before_its_tool_use` asserts it.
- Tests:
  - `rollout::tests::resume_rebuilds_signed_thinking_before_tool_use`: hand-built events with two signed calls, one unsigned call and one unsigned thinking item.
  - `crates/cox-core/tests/resume.rs`: a test-only `Signed` provider wraps `Scripted` and inserts `ToolUseSignature` after every `ToolUseStart`, so the scenario format needed no new key. `signed_tool_call_keeps_signature_before_its_tool_use` checks the live history and the strip; `resume_builds_identical_request_with_signature` runs the existing test's body (now the shared helper `resume_matches_live`) with `Signed` and asserts the signed block exists. `resume_builds_identical_request` still runs the plain scenario.
  - Before the fix: `resume_rebuilds_signed_thinking_before_tool_use` and `signed_tool_call_keeps_signature_before_its_tool_use` failed. `resume_builds_identical_request_with_signature` fails without the fix on its signed-block assertion.
- Deviations: the resume test is split into a helper and two tests (plain and signed) rather than changing the one existing test, so the unsigned path keeps its own case. Source diff: `rollout.rs` +91 (about 60 of it the test), `session.rs` +12, `turn.rs` +28; `tests/resume.rs` is a test file.
- Check output:
  - The card's Check, plus `test(signed)`: 6 passed (`resume_builds_identical_request`, `resume_builds_identical_request_with_signature`, `signed_tool_call_keeps_signature_before_its_tool_use`, `resume_rebuilds_signed_thinking_before_tool_use`, `router_strip_thinking_keeps_everything_else_verbatim`, `consume_provider_keeps_signature_by_call_id`).
  - `cargo nextest run -p cox-core`: 272 passed, 1 skipped. `cargo nextest run -p cox -E 'test(resume) | test(rollout)'` (the binary's resume path over `History::from_rollout`): 6 passed.
  - `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` clean.
  - The real binary, with a scratch `COX_HOME` and the scripted provider, ran a `read` tool turn and a `--continue` resume; the scratch dir was removed afterwards. The scripted provider emits no signature, so this covers only the unsigned path.

#### T41.1 `[lsp]` config and its project-config guard


- Model: Claude Code / opus-5.5
- Depends: -
- Size: ~110
- Priority: P1
- Complexity: 2
- Goal: `[lsp]` is part of the config with `enabled`, `timeout_s`, `quiet_ms` and a `servers.<name> { command, args, extensions }` table, with a default matrix. A project config cannot set `lsp.servers`: a repository must not choose a program cox runs.
- Files: `crates/cox-protocol/src/config.rs`, `crates/cox-config/src/load.rs`. Data: `crates/cox-protocol/default.toml` (plus the regenerated `docs/config.jsonschema` and `docs/config.md`).
- Steps:
  1. Add an `LspConfig` struct with serde defaults:
     - `enabled = true`, `timeout_s = 30`, `quiet_ms = 500`;
     - servers `rust` (`rust-analyzer`, `rs`), `typescript` (`typescript-language-server --stdio`, `ts tsx js jsx`), `python` (`pyright-langserver --stdio`, `py`) and `go` (`gopls`, `go`).
  2. Add `lsp.servers` to the project-config guard list in `load.rs`, with the same refusal message as the other guarded keys.
  3. Tests: `lsp_defaults_parse`, `project_config_cannot_set_lsp_servers`, and the docs drift test.
- Check:
  ```bash
  mise exec -- cargo nextest run -p cox-protocol -E 'test(lsp)'
  mise exec -- cargo nextest run -p cox-config -E 'test(lsp) | test(schema)'
  ```
- Plan:
  1. Tests first. `crates/cox-protocol/src/config.rs`: `lsp_defaults_parse` (both `LspConfig::default()` and `default.toml` through figment give `enabled`, `timeout_s = 30`, `quiet_ms = 500` and the four servers with their commands, args and extensions); fails to compile until `LspConfig` exists. `crates/cox-config/src/load.rs`: `project_config_cannot_set_lsp_servers` (a project `.cox/config.toml` that adds a server and changes the default `rust` command is reverted to the user/default servers, one `lsp.servers` violation, `source_of("lsp.servers")` is not `project`, a project `lsp.timeout_s` still applies).
  2. `config.rs`: `LspConfig { enabled, timeout_s, quiet_ms, servers: BTreeMap<String, LspServerConfig> }` and `LspServerConfig { command, args, extensions }`, both `deny_unknown_fields` + `default`, hand-written `Default` carrying the matrix; `Config.lsp`. `default.toml`: `[lsp]` plus one `[lsp.servers.<name>]` table per default server.
  3. `load.rs`: guard `lsp.servers` — any difference from the layers without the project reverts the whole map (a repository must not choose a program cox runs), reported as a `GuardViolation` like the others; add the key to `GUARDED_KEYS`.
  4. Regenerate `docs/config.md` and `docs/config.jsonschema` through their drift tests (delete, re-run the test that writes them). Verify: the card's Check, `cox-protocol` and `cox-config` suites, the config tests in `crates/cox`, fmt, clippy; the real binary's `config show` against `COX_HOME=/tmp/cox-t41.1` with a project config that sets `lsp.servers` (warned and reverted), removed afterwards.
- Done when: `docs/config.md` documents every `lsp` key, enforced by the existing docs test.
- Out of scope: using the config (T41.7).

- Result:
  - `crates/cox-protocol/src/config.rs`: `LspConfig { enabled, timeout_s, quiet_ms, servers }` and `LspServerConfig { command, args, extensions }` (`deny_unknown_fields`, `default`; `servers` is a `BTreeMap` so listings have one order), `Config.lsp`, hand-written `Default` with the four-server matrix. `default.toml`: `[lsp]` and one `[lsp.servers.<name>]` table per default server; the guard is documented on `lsp.servers.rust.command`.
  - `crates/cox-config/src/load.rs`: `apply_project_guards` reverts the whole `lsp.servers` map to the layers without the project when the project changed it (added a server or changed any field), one `GuardViolation { key: "lsp.servers", project_value: <changed names>, reverted_to: <kept names> }`, printed by `crates/cox` with the same `project config ignores … (guard); using …` warning as the other guarded keys; `lsp.servers` added to `GUARDED_KEYS`. `LoadedConfig::source_of` now treats a leaf key under a guarded table (`lsp.servers.rust.command`, which is what `cox config show --sources` asks for) as reverted too, so it reports `default`/`user` instead of `project`.
  - `docs/config.md` and `docs/config.jsonschema` regenerated by deleting them and re-running `config_docs_config_md_matches_default_toml` and `config_jsonschema_matches_committed_file`; the diff is additions only.
- Tests: `lsp_defaults_parse` (cox-protocol: `LspConfig::default()` equals `default.toml`'s `[lsp]`, with the four servers' commands, args and extensions) failed to compile before `LspConfig` existed; `project_config_cannot_set_lsp_servers` (cox-config: a project that changes `rust`'s command and adds a server is reverted, the user's own `zig` server survives, `lsp.timeout_s` stays project-settable, provenance of the leaf keys is `default`/`user`) failed on the guard-less code and again, before the `source_of` fix, on the leaf-key provenance.
- Deviations: `default.toml` has no `servers = {}` line (TOML cannot extend an inline table with `[lsp.servers.<name>]` headers); the `source_of` leaf-key fix was not in the card but is needed for `cox config show --sources` to report the reverted servers truthfully.
- Check output summary: `cargo nextest run -p cox-protocol -E 'test(lsp)'` 1 passed; `cargo nextest run -p cox-config -E 'test(lsp) | test(schema)'` 2 passed; `cargo nextest run -p cox-protocol -p cox-config` 106 passed; `cargo nextest run -p cox -E 'test(config)'` 12 passed; `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean. Real binary, scratch `COX_HOME=/tmp/cox-t41.1` and a project `.cox/config.toml` setting `lsp.timeout_s = 10` and `lsp.servers.rust.command = "./evil"`: `cox config show --sources` warned `project config ignores lsp.servers = rust (guard); using go, python, rust, typescript`, showed `lsp.servers.rust.command = "rust-analyzer"  # default` and `lsp.timeout_s = 10  # project`; scratch removed.
- Status: done 2026-09-28

#### T37.23.4 User bubble and thinking inside the transcript text

Depends: — · Size: ~150 · Files: `desktop/macos/Packages/CoxTranscript/…`, `desktop/macos/Packages/CoxTranscriptText/…`
Goal: user prompts and thinking blocks render in `TranscriptView` with the `UserBubble` and `ThinkingDisclosure` look (T37.21.5), as styled TextKit fragments or card attachments, so their text stays selectable across blocks.
Check: snapshots of a turn with a user prompt with an attachment and a folded and an open thinking block; a drag from the prompt into the reply copies both in order.
Status: done 2026-09-28
Result:
- User prompts and thoughts are styled text inside `TranscriptView`'s one text (A87), not card attachments, so a selection can start partway through a prompt.
- `CoxTranscriptText/TranscriptDecor.swift`:
  - a `Decor` attribute and a layout-manager delegate give those paragraphs a `DecorFragment`, which draws the bubble slice or the thought's hairline rule;
  - `TranscriptStyle` gains `bubble` and `thought`.
- Attachment tiles and the thought's fold header are view-backed `CardAttachment`s, supplied through `TranscriptCards(thumbnail:thinking:)`.
- Thoughts start folded:
  - `setThought(_:open:)` edits only the text after the header, and no other block's range moves;
  - Copy gives what is shown: a prompt without its tiles, and nothing for a folded thought.
- CoxUI changes: `Thumbnail` is public, `ThinkingHeader` is split out of `ThinkingDisclosure`, and `SurfaceColour` is new.
- DT§5.2 and DS§6.3 are updated.
Deviations:
- About 290 source lines in 10 files, against the card's ~150 lines and 3 files.
- The header reads "Thinking", because the thinking block carries no duration (T37.23.10).
- The bubble is a fill only, with no glass or e2 shadow (T37.23.9).
- The `everyBlockKind` snapshots were re-recorded.
Check:
- CoxTranscriptText: 26/26, including `TranscriptDecorTests`.
- CoxTranscript: 8/8, including `TranscriptTurnTests`: light and dark snapshots, and a real `NSEvent` drag from the prompt into the reply that copies prompt → reasoning → reply as Markdown.
- CoxUI: 111/111.
- `swift-format lint --strict` and `swiftlint --strict` are clean.
Not done:
- The thought duration: T37.23.10.
- The bubble's glass and elevation, and the prompt's hover actions: T37.23.9.

#### T37.30.3 MCP login status and OAuth

Depends: T37.30.1, T37.30.2 · Size: ~150 · Files: `crates/cox-app/…`, `…/Screens/SettingsScreen.swift`
Goal: per MCP server, its login status on the Settings screen, and Log in / Log out through `Host::open_url` and the existing `cox-mcp` OAuth.
Check: a fixture server shows logged out, then logged in after a scripted callback; tests use `cox_mcp::auth`'s memory store.
Status: done 2026-09-28
Result:
- `crates/cox-app/src/mcp_login.rs` has two calls:
  - `servers()` lists each MCP server with its login state: stdio, logged out, logged in with time left, expired, or unreadable.
  - `set_login()` runs cox-mcp's OAuth, opening the page through `Host::open_url`, or logs out with `auth::logout`.
- A `Flow` seam lets tests script the OAuth callback. `App::with_mcp` injects `cox_mcp::auth`'s memory store.
- cox-ffi changes:
  - `settings` and `set_setting` are async, because reading a token can wait on the keychain.
  - `mcp_login` is new, a one-expression forward (A90).
- On the Swift side:
  - CoxClient: `McpServer`, `McpLogin` and `mcpLogin`.
  - CoxModel: `SettingsStore.setLogin` and `.logins`.
  - CoxUI: a `Logins` box on the MCP page.
  - CoxCore converts the new types.
Deviations:
- `ProjectRow` became `cox_app::Project` with `sessions: u64`, so cox-ffi forwards it without mapping.
- `cox_mcp::auth::human` is public.
- cox-app depends on cox-mcp and rmcp directly. Both were already in its tree through cox-session.
- The change touches more than 3 files.
Check:
- `nextest -p cox-app -p cox-ffi -p cox-config -p cox-mcp`: 75/75. Clippy `-D warnings` and fmt are clean.
- Swift: CoxModel 22/22, CoxUI 107, and CoxCore 9/9 against a rebuilt XCFramework with `COX_KEYRING=off`. swiftlint and swift-format are clean.
- After merging into `p37-desktop`, the same Rust suites passed 80/80 once main's `lsp.servers` guard (T41.1) got its own reason.
Not done:
- `-p cox --test deps` was not run locally; it is left to CI. The new edges add no terminal toolkit, `clap` or `anyhow` to cox-app.

#### T37.30.4 Show dropped project values

Depends: T37.30.1 · Size: ~150 · Files: `crates/cox-app/src/settings.rs`, `…/Screens/SettingsScreen.swift`
Goal: the Settings screen lists project values the guard list threw out, with the reason, so a user sees why a project setting did not apply.
Check: snapshot of a project file that raises the budget: the value is listed as dropped with its reason.
Status: done 2026-09-28
Result:
- cox-config: `GuardViolation::reason()`, with a test that every guarded key has its own reason.
- cox-app: `SettingsView.dropped` (key, value, kept, reason), filled from `LoadedConfig.violations`.
- cox-ffi: a remote `Dropped` record only, with no new forward.
- CoxModel: `SettingsStore.dropped(in:)`, grouped by page.
- CoxUI: a "Dropped from the project" box with a warning badge (`999 → 5`).
- DESIGN.md §6.5 and DT§5.7 have a sentence each.
Deviations: none beyond T37.30.3's.
Check:
- The insta snapshot `a_project_value_the_guard_drops_is_listed_with_its_reason` shows `budget.session_usd`: project value 999, kept 5, reason "A project may not raise a budget above your own".
- 4 CoxUI PNG snapshots of the Budget page.
- The Rust and Swift suites listed in T37.30.3.
Not done: nothing.

#### T37.24 Composer: mentions, commands, shell mode, attachments, queue

Depends: T37.23, T37.21.7 · Size: split at claim · Files: `…/Organisms/Composer.swift`, `…/Molecules/ComposerChip.swift`
Goal: DT§5 composer with completion driven by `cox-app` (T37.10).
Check: UI test types `@`, picks a file, sends; the intent reaches the fixture client.
Status: done 2026-09-28
Result: T37.24 landed as four commits.
- T37.24.1: `CoxUI/Organisms/Composer.swift`, `Composer(state:send:)`:
  - It has the editor with its hint, completion rows (new molecule `CompletionList`), shell mode with "Share output", mention chips, a "Queued · n" chip and Send.
  - ⏎, ⇧⏎, ⌘⏎, ↑ ↓ ⇥ ⎋ and ⌫ each map to a `Composer.Intent`.
- T37.24.2: completion through the client:
  - `SessionClient.complete` and `CoxClient.Completion`, with `LiveSession` forwarding to the existing `SessionHandle.complete`.
  - `CoxModel/ComposerStore.swift` handles `@` and `/` completion and picks one intent (`shell`, `command` or `send`).
  - `CoxTranscript/SessionComposer.swift` connects the composer to the store.
- T37.24.3: dropped or picked files are read off the main actor and sent as attachments, shown as `Thumbnail` rows with a remove badge (`ComposerAttachments`).
- T37.24.4: while a turn runs, ⏎ queues the prompt (`.queue`) and ⌘⏎ interrupts and sends.
- DESIGN.md §6.3 and §6.4 and the DT§4.6 rows are updated. cox-ffi is unchanged.
Deviations:
- The selected row is on `accent.soft`.
- Send uses `CoxButtonStyle(.primary)`.
- Completion uses the word at the end of the draft, not the word at the caret.
- `failure` is not shown.
- The queue length is derived from block turn numbers.
- A draft with attachments cannot be queued.
- `LiveSession.complete` was checked against the bindings but not compiled.
- Paste is not handled.
Check:
- CoxUI: 113 tests in 38 suites.
- CoxModel: 27.
- CoxTranscript `ComposerFlowTests`: 2. One is a real editor test (`@` → ↓ ⏎ → ⏎ reaches `FixtureSession`), the other is `whileATurnRunsReturnQueuesAndCommandReturnInterruptsAndSends`.
- `ComposerStoreTests` and `attachedFilesAreReadAndSentWithTheTurn` pass.
- swiftlint and swift-format are clean.
Not done:
- Placing `SessionComposer` in `MainScreen` goes to T37.22.3.
- Remaining work is in T37.24.5–T37.24.9.

#### T37.39.1 `cox-ffi` forwards only: the check

Depends: — · Size: ~80 · Files: `crates/cox-ffi/tests/forward_only.rs`, `AGENTS.md`
Goal: A90's rule as a test. Every `#[uniffi::export]` function and method in `lib.rs`, `session.rs` and `host.rs` has a body of one expression that calls into `cox-app`, or converts through `types.rs`. The AGENTS.md `cox-ffi` row states the rule instead of the line count.
Check: the test passes on the current crate; a scratch copy of a method with an `if`, a `match` or a second statement makes it fail.
Status: done 2026-09-28
Result:
- `crates/cox-ffi/tests/forward_only.rs` parses `lib.rs`, `session.rs` and `host.rs` with `syn`. It fails on any function or method body that is not one forward expression, exported or not, trait default bodies included.
  - Allowed forms: calls, method chains, fields, `?`, `.await`, `&`, struct literals, and closures or `async` blocks that are themselves one such expression.
  - Failing forms: `let`, macros, `if`, `match`, loops, operators, `as` and the rest.
- One exemption: the `From<OwnerError> for AppError` that flattens cox-app's error for Swift.
- Code that broke the rule was fixed inside cox-ffi:
  - `let`s were folded into calls.
  - Async methods take `self: Arc<Self>`. Swift's API is unchanged.
  - The runtime `OnceLock` is at module scope.
- The AGENTS.md `cox-ffi` row states the rule (A90).
Deviations:
- Three files beyond the card's two, plus the example, `Cargo.toml` and `Cargo.lock`; about 60 changed lines.
- New dev-dependency `syn` 3.0.5, already in the lockfile. Recorded in §1.1, `toolchain.md` and `rust.md`.
- At the merge into `p37-desktop`, T37.30.3's `settings`, `set_setting` and `mcp_login` were brought under the rule the same way. The branch's `ProjectRow → Project` conversion was dropped, because T37.30.3 made `cox_app::Project` forwardable as is.
Check:
- `nextest -p cox-ffi`: 6/6, on the branch and again after the merge.
- The card's negative check: a scratch `if`, `match` or second statement in `SessionHandle::id` each fails the test, then was reverted.
- Clippy `-D warnings` and fmt are clean.
Not done: where a call lands (only `cox-app` and `cox-protocol`) stays `deps.rs`'s job.

#### T37.23.7 Follow the tail while a reply streams

Depends: — · Size: ~80 · Files: `desktop/macos/Packages/CoxTranscript/…`
Goal: while the view is scrolled to the bottom it stays there as a reply streams; scrolling up stops following until the user returns to the bottom.
Check: a test streams `docTail` patches and the view stays at the bottom; after a scroll up it stays put.
Status: done 2026-09-28
Result:
- `CoxTranscript/TailFollow.swift`: a patch batch that lands while the reader is at the bottom scrolls the new end into view. After the reader scrolls up, the view stays put until they return to the bottom.
- Whether to follow is decided on each scroll, so text that grows below between batches does not break following. A transcript opened at the top stays there.
- `TranscriptCoordinator.follow` runs each `didApply` batch through it.
Deviations:
- The streaming benchmark times the view's own follow instead of calling `scrollToEndOfDocument` on every frame. Both budgets still pass.
- DT§5 is not updated.
Check:
- `TranscriptTailTests` (3/3) use the real hosted scroll view and `docTail` batches:
  - stays pinned after every batch;
  - stays exactly in place after a 300 pt scroll up;
  - follows again after the reader returns;
  - a short transcript is followed once it outgrows the window;
  - a transcript opened at the top stays at offset 0.
- Each test fails with the scroll removed or with it always on.
- CoxTranscript: 11 tests in 5 suites. Streaming busy 16.1%, max 5.18 ms; scroll hitch 0.00% at load 25.8.
- swiftlint and swift-format are clean.
Not done: app wiring waits for T37.22.3.

#### T37.29 Inspector tabs: Changes, Plan, Context & Cost, Tasks, Info

Depends: T37.23, T37.21.9 · Size: split at claim · Files: `…/Organisms/Inspector.swift`, `…/Molecules/ChangedFileRow.swift`, `…/Molecules/CheckpointRow.swift`
Goal: DT§5 inspector built from DS§6 rows.
Check: snapshot per tab.
Status: done 2026-09-28
Result: first slice — the tab structure and the Changes tab.
- `Inspector` puts every tab's content in one scrolling body.
- `Organisms/ChangesTab.swift`, `ChangesTab(state:send:)`:
  - Three sections: changed files under "Review ⌘⇧R", checkpoints, and worktree facts.
  - Intents: `review`, `open(path)`, `revert(path)`, `rewind(checkpoint)`.
  - An empty state.
- `InspectorSection` is the section block the other tabs reuse. `CheckpointRow.Checkpoint` gains an `id`.
- Fixtures are in `PreviewState+Inspector.swift`. DS§6.4 has a `ChangesTab` row.
Deviations:
- The tab takes plain fixture values; the cox-app call that feeds it is T37.29.1.
- ⌘⇧R is shown but not bound; the menu owns it (T37.22.3).
Check:
- CoxUI: 121 tests, twice; the 5 new snapshots were recorded, and existing ones (`inspectorFrame`, `MainScreen`) are unchanged.
- swiftlint and swift-format are clean.
- After merging into `p37-desktop`, `swift build --build-tests` for CoxUI succeeds.
Not done: the other tabs and the data calls are T37.29.1–T37.29.5.

#### T37.23.5 Structured diff hunks from Rust for edit cards

Depends: — · Size: ~150 · Files: `crates/cox-app/…`, `desktop/macos/Packages/CoxModel/…`, `desktop/macos/Packages/CoxTranscript/…`
Goal: `cox-app` sends an edit's hunks as structured lines (kind, old/new numbers, `StyledDoc` spans), so `ToolCard` shows `DiffHunkView`s and Swift never parses a unified diff.
Check: a `cox-app` test of the hunk shape for a scripted edit; a snapshot of an opened edit card.
Status: done 2026-09-28
Result:
- `crates/cox-render/src/diffmodel.rs` defines `DiffModel`, `DiffHunk`, `DiffLine` and `DiffLineKind`, highlighted through `highlight_runs`. It builds without the ratatui feature.
- The unified-diff parse moved there from `diff.rs`, and the TUI calls it too, so there is still one diff engine.
- cox-app: `BlockKind::Tool.diff` is `Option<DiffModel>`, built on `ToolCallDone`.
- cox-ffi: remote declarations replace the `Diff` record, and no export was added.
- Swift:
  - CoxClient's `Diff` becomes `DiffModel` and the types under it.
  - CoxCore's `Convert.swift` maps them.
  - `TranscriptCard` fills `ToolCard`'s `.diff` hunks.
- New fixture `desktop/macos/Fixtures/edit.json` (a write then an edit), recorded from `crates/cox-ffi/fixtures/edit.toml`.
Deviations:
- More than 3 files.
- `CodeRun` roles stay `.plain`: spans carry theme colours and no `StyleToken` names a syntax role.
- There is no word-level diff on the desktop (T37.23.11).
- The fixture is recorded with `COX_PERMISSIONS_MODE=auto`.
- `CoxTranscriptTests/Host.swift`'s `hosting` is no longer private.
Check:
- `nextest -p cox-render -p cox-app -p cox-ffi -p cox-tui`: 348/348, including `scenarios__edit.snap` and 3 `diffmodel` tests.
- Clippy and fmt are clean.
- Swift: CoxModel 20/20 (replays both fixtures), CoxTranscriptText 23/23, CoxTranscript 8/8 with `EditCardSnapshotTests` (light and dark).
- swiftlint and swift-format are clean.
- After merging into `p37-desktop`: `nextest -p cox-render -p cox-app -p cox-ffi -p cox-tui` 355/355 (forward_only included), CoxModel 30/30, CoxTranscript 13/13.
Not done:
- CoxCore was not compiled; its names were checked against generated bindings.
- Syntax roles for `CodeRun` wait on the creator: either `StyleToken` gets syntax roles, or the desktop uses theme colours.

#### T37.23.8 Headings, lists and quotes in replies

Depends: — · Size: ~120 · Files: `desktop/macos/Packages/CoxTranscriptText/…`
Goal: `StyledDoc` headings, lists, quotes, rules and tables render with their structure (indents, markers, heading sizes from tokens) instead of flat paragraphs.
Check: a snapshot of a reply with each block kind; Copy as Markdown of it round-trips the structure.
Status: done 2026-09-28
Result:
- Reply structure is set by paragraph styles in the one transcript text (A87), in `CoxTranscriptText/TranscriptStructure.swift`:
  - headings use `font.transcript.h3`, with block spacing above them;
  - list and quote lines hang past the marker or rail that `cox-render` sends, and lists are indented by `space.xxl`;
  - tables line up on tab stops;
  - a rule is an attachment drawn with the thought's hairline.
- A streamed reply ends with the same styles as a full load.
- CoxModel's `DocMarkdown.swift` turns Rust-shaped docs back into clean Markdown (`-` bullets, `>` quotes, no doubled `##`). This also fixes the stored text of streamed replies.
Deviations:
- 5 source files and about 280 lines.
- The `everyBlockKind` snapshots were re-recorded.
- The `#` markers stay visible, as in the TUI, because the markers are text from Rust (DT-3).
- One heading token is used for every level.
Check:
- `TranscriptReplyTests`: a light and dark snapshot with every block kind, and `copyAsMarkdownGivesTheStructureBack` for a whole reply and a partial selection.
- CoxTranscriptText 29, CoxTranscript 10 (with the benchmark gates), CoxModel 20.
- swiftlint and swift-format are clean.
- After merging into `p37-desktop`: CoxModel 30/30, CoxTranscriptText 29/29, CoxTranscript 15/15.
Not done:
- Per-level heading sizes (DT§5.9's 17/15/13 pt) need two new tokens.
- Hiding the markers needs `StyledDoc` to send depth and marker apart from the text.
- Both are creator questions, recorded in `ideas.md`.

#### T37.27 Approvals, questions, inbox, notifications with actions, Dock badge

Depends: T37.23, T37.4 · Size: split at claim · Files: `…/Organisms/ApprovalCard.swift`, `desktop/macos/Packages/CoxPlatform/…`
Goal: DT§5 approvals and questions in the transcript and as actionable notifications; the inbox and Dock badge count what needs you.
Check: a fixture with a pending approval shows the card, the notification and badge 1; approving from the notification resumes the turn.
Status: done 2026-09-28
Result: four commits.
- T37.27.1: CoxUI `ApprovalCard` (Allow, Allow for session, Deny, a risk chip, a decided row) and `QuestionCard` (options or a typed answer).
  - CoxTranscript `DecisionCard` maps a block to the card and sends `.approve` or `.answer`.
  - `TranscriptView(store:crossBlockSelection:send:)` fills the slot.
- T37.27.2: the recorder writes `notes {batch, item, badge}`, and a new fixture `approve-write.json` has one approval with badge 1.
  - CoxClient gets `InboxItem`/`Need` → `HostNote`.
  - `FixtureCoreClient(host:waitsForYou:)` notifies the host and holds the turn until the card is answered.
- T37.27.3: cox-app's `Host` gains `badge(u32)`, called when the count falls. It is forwarded through `AppHost` → `HostBridge` → `MacHost`.
- T37.27.4: CoxPlatform `NotificationActions` defines the categories (Allow and Deny; Answer as typed text).
  - `content(for:)` builds the notification, and `route(action:userInfo:text:)` turns a response into a `NotificationRoute`.
  - `NotificationResponder` is the delegate the app installs.
  - `Decision.deniedByUser` is shared with `DecisionCard`.
- Design doc §4.4 and §5.6 are updated.
Deviations:
- No Edit… button and no grant preview, because the approval block carries neither (T37.27.6).
- Holding the turn in the fixture is opt-in (`waitsForYou`).
- The `Host` trait change reaches every implementor.
Check:
- cox-ffi `an_approval_is_noted_with_badge_one_and_allowing_it_resumes_the_turn`.
- `nextest -p cox-app -p cox-ffi`: 46/46.
- CoxModel `aRecordedApprovalIsNotedWithBadgeOneAndApprovingResumesTheTurn` fails without `waitsForYou`.
- CoxPlatform 13, including `allowingFromTheNotificationResumesTheRecordedTurn`.
- CoxCore 7, against a rebuilt XCFramework.
- CoxUI: 28 card snapshots. CoxTranscript: `DecisionCard` 6.
- swiftlint and swift-format are clean.
- After merging into `p37-desktop`: T37.30.3's test hosts gained `badge`, and `DecisionCard`'s snapshots were re-recorded (the user turn above them is T37.23.4's bubble). `nextest -p cox-app -p cox-ffi` 57/57; CoxModel 32, CoxPlatform 13, CoxTranscript 21, CoxUI 128.
Not done:
- Installing `NotificationResponder` and `HostBridge(MacHost())` in the app goes to T37.22.3, and notifications there post only when the session is not visible.
- Remaining parts are T37.27.5–T37.27.7.

#### T37.29.4 Inspector Tasks tab

Depends: — · Size: ~120 · Files: `desktop/macos/Packages/CoxUI/…/Organisms/TasksTab.swift`, `desktop/macos/Packages/CoxModel/…`
Goal: subagent and background-call rows with label, tier, state and cost, fed from `BlockKind.task`; a click opens the child transcript (a session-open-by-task call in cox-app if one is missing).
Check: a snapshot per cell; a test that a click sends the open intent with the child id.
Status: done 2026-09-28
Result:
- CoxUI `Organisms/TasksTab.swift`, `TasksTab(state:send:)`:
  - One `InspectorSection`, "Subagents & background · n", with one `InspectorRow` per task: glyph, label, tier `Badge`, cost once done, and the ToolHeader status glyph.
  - A row click or "Open transcript" sends `.open(task:)`.
  - An empty state.
- CoxModel `TaskRows.swift`: `SessionStore.tasks` maps each `BlockKind.task` to a `TaskRow` (id, label, tier, state, cost).
- DS§6.4 has a `TasksTab` row. `ToolHeaderStatus` is internal instead of private.
Deviations:
- The intent carries the task id, not a child session id, because the task block has only a `TaskId` (T37.29.6).
- Every row is clickable: the block does not say whether a task is a subagent or a background shell.
Check:
- CoxModel `taskBlocksBecomeRowsWithStateAndCostInTimelineOrder`: 31 tests.
- CoxUI: 5 TasksTab snapshots and `aRowClickOpensTheTaskTranscriptByItsId`; the full suite passes 130 tests.
- swiftlint and swift-format are clean.
- After merging into `p37-desktop`: CoxModel 33/33, CoxUI TasksTab and Inspector 11/11.
Not done: opening the child transcript (T37.29.6); app wiring (T37.22.3).

#### T37.24.6 Prompt history in the composer

Depends: — · Size: ~120 · Files: `crates/cox-app/…`, `crates/cox-ffi/src/session.rs`, `desktop/macos/Packages/CoxModel/…`
Goal: ↑ in an empty composer walks the session's earlier prompts, newest first, through a new `cox-app` call and its one-expression FFI forward (A90).
Check: a cox-app test for the call; a UI test presses ↑ twice and gets the two earlier prompts, with the fixture client serving them without Rust.
Status: done 2026-09-28
Result:
- cox-app `Workspace::prompts(session, limit)` reuses the TUI's Ctrl+R query (`Store::user_prompts`), keeps this session's prompts, newest first; `LiveSession::history(limit)` calls it. cox-ffi `SessionHandle::history(limit)` is a one-expression forward (A90).
- Swift: `SessionClient.history(limit:)` on `LiveSession` (CoxCore) and `FixtureSession` (new `prompts:` parameter). `ComposerStore.recall(_:)`: ↑ in an empty draft starts the walk, ↓ past the newest empties the draft, typing or sending ends it. `Composer` gets `.recall(Int)`, `State.isRecalling` and an `arrow()` helper that routes ↑/↓ to the completion rows or the history.
Deviations:
- `ComposerFlowTests` is `@Suite(.serialized)` and `settle(until:)` runs the run loop while it polls: both old tests already failed at random on HEAD.
- 11 files, about 110 non-test lines.
Check:
- `cargo nextest run -p cox-app --test app`: 8/8, including `history_is_the_sessions_own_prompts_newest_first`; `-p cox-app -p cox-ffi` 57/57 with `forward_only`.
- CoxModel `upWalksOlderPromptsStopsAtTheOldestAndDownPastTheNewestEmptiesTheDraft`; CoxTranscript `upInTheEmptyComposerBringsBackTheEarlierPromptsNewestFirst` (real key events, fixture client); CoxCore 9/9.
- After merging into `p37-desktop` with T37.29.1, T37.27.7, T37.24.5, T37.23.6 and T37.25: cox-app and cox-ffi 63/63, CoxModel 39/39, CoxTranscript 27/27, CoxUI 138/138, CoxPlatform 13/13.
Not done: app wiring (T37.22.3).

#### T37.29.1 `changes()` from cox-app for the Changes tab

Depends: — · Size: ~150 · Files: `crates/cox-app/…`, `crates/cox-ffi/src/session.rs`, `desktop/macos/Packages/CoxModel/…`
Goal: `SessionHandle::changes()` returns what `ChangesTab` shows: files with change kind, added/removed lines and the call that changed them; checkpoints with id, label and time; the worktree's branch and base commit. The FFI side is a one-expression forward (A90); CoxModel maps it to `ChangesTab`'s state.
Check: a cox-app test over a scripted session with two edits and a checkpoint; a CoxModel test that the mapping fills the tab.
Status: done 2026-09-28
Result:
- cox-app `LiveSession::changes()` returns `Changes { files, checkpoints, worktree }`, built on request by `crates/cox-app/src/changes.rs` from the timeline blocks, the store's checkpoint rows and `cox_tools::git::linked`: each file (path relative to cwd, Edited/Created/Deleted, added/removed from the diff model, last call and turn; created-then-deleted and rewound calls drop out), one checkpoint per turn that changed files (turn, label, RFC 3339 time), and the linked worktree (branch, base, short merge-base, size) or `None`.
- cox-store `checkpoint_rows` (with `created_at`; `checkpoint_list` delegates to it); cox-tools `git::linked`.
- cox-ffi `SessionHandle::changes` is a one-expression forward, records declared with `#[uniffi::remote]`.
- Swift: CoxClient `Changes.swift` and `SessionClient.changes()` (also on `FixtureSession`); CoxCore `ChangesConvert.swift`; CoxModel `ChangesTabState` maps to `ChangesTab.State`; `SessionStore.changesTab()` loads it. `docs/design/desktop.md` lists `changes` on SessionHandle.
Deviations:
- About 330 non-test lines in 16 files: cox-store and cox-tools did not expose the time or the merge-base.
- The merge into `p37-desktop` counts added/removed from T37.23.5's `DiffModel` lines instead of `diffstat::counts` on the unified text.
Check:
- `cargo nextest run -p cox-app -p cox-ffi -p cox-store`: 84, including `changes_lists_the_edited_and_created_files_and_the_turn_to_rewind_to` and `forward_only`; `-p cox-tools git::` 10.
- CoxModel 33 (3 new ChangesTab tests), CoxCore 10 (`aChangesRecordConvertsFieldForField`).
- After merging into `p37-desktop`: cox-app, cox-ffi and cox-store 86/86; CoxModel 39/39.
Not done: a `deleted` glyph in CoxUI and the line count of a created file (T37.29.7); app wiring and `rewind(checkpoint:)` → `Intent.rewind` (T37.22.3).

#### T37.27.7 "Needs you" inbox store for the sidebar

Depends: — · Size: ~100 · Files: `desktop/macos/Packages/CoxModel/…`, `desktop/macos/Packages/CoxUI/…/Sidebar.swift`
Goal: a Swift store over the app inbox gives the sidebar's "Needs you" rows, one per item, with expired rows read-only.
Check: with the `approve-write` fixture the store lists one row, which clears once the card is answered.
Status: done 2026-09-28
Result:
- CoxModel `InboxStore` (`refresh()`, `rows: [InboxRow]`, `count`) reads the inbox through a new `InboxClient` protocol (CoxClient `Inbox.swift`, with `Need.call`). Each item is one `InboxRow`: id `session#seq`, the session it opens, status waiting/idle/error, the `HostNote` text as title and what it waits for as subtitle; an expired item is read-only and reads "expired".
- `FixtureCoreClient` implements `InboxClient` with one inbox shared by its sessions (an item appears when its note is pulled and goes once answered); `LiveCoreClient` forwards to cox-ffi `App.inbox()`.
- CoxUI `Sidebar.Session` gains `session`, `isReadOnly` and `opens`; a read-only row is disabled. `#Preview("needs you")` and four snapshots.
Deviations: five source files instead of three (about 140 lines).
Check:
- `theRecordedApprovalIsOneRowThatClearsOnceAnswered` (approve-write fixture: one row, none after `.approve`); CoxModel 35, CoxPlatform 13, CoxUI 134.
- After merging into `p37-desktop` (with `approve-write.json` re-recorded for T37.25's usage text): CoxModel 39/39, CoxUI 138/138.
Not done: CoxCore was not compiled by the agent (one line, reusing HostBridge's conversion; left to CI); wiring `refresh()` to host notify/badge and the rows into the sidebar (T37.22.3); dismissing news items; the fixture keeps arrival order, not urgency order.

#### T37.24.5 Paste into the composer

Depends: — · Size: ~60 · Files: `desktop/macos/Packages/CoxUI/…/Composer.swift`, `desktop/macos/Packages/CoxModel/…/ComposerStore.swift`
Goal: ⌘V of an image or file URLs attaches them (as T37.24.3's drop does) instead of inserting text.
Check: a UI test pastes a PNG from a private pasteboard and the `send` intent carries it.
Status: done 2026-09-28
Result:
- CoxUI `Composer.swift`: a private `ComposerPaste` view catches ⌘V in the editor's window with an app-local key-event monitor while the editor has focus (the Edit menu's Paste takes ⌘V before `onKeyPress`). File URLs go through `.drop` and `ComposerStore.attach(_ urls:)`; an image with no file behind it (PNG, or TIFF converted to PNG) sends the new `.pasteImage(Data)`; anything else, or an image that comes with text, is left to the normal paste.
- Environment value `composerPasteboard` (default `.general`). CoxModel `ComposerStore.attach(_ data:name:type:)`, which the URL path now uses too; `SessionComposer` maps `.pasteImage` to "Pasted image.png".
Deviations: `SessionComposer.swift` as a third file; about 85 source lines.
Check:
- `pastingAPNGAttachesItAndSendCarriesIt` (private `NSPasteboard`, ⌘V through `NSApp.sendEvent`; fails with the monitor off) and `pastingTextAttachesNothingAndLeavesTheKeyToTheMenu`.
- CoxModel 33, CoxUI 132, CoxTranscript 25; swiftlint and swift-format clean.
- After merging into `p37-desktop`: CoxModel 39/39, CoxTranscript 27/27, CoxUI 138/138.
Not done: a try in the real app (T37.32/T37.22.3), including text paste through the Edit menu and ⌘V on a Cyrillic layout.

#### T37.23.6 Restyle the transcript when the text size changes

Depends: — · Size: ~80 · Files: `desktop/macos/Packages/CoxTranscriptText/…`, `desktop/macos/Packages/CoxTranscript/…`
Goal: `TranscriptStyle` is rebuilt when `[desktop.transcript]` text size or line height changes, and the whole text restyles in place without losing the selection.
Check: snapshots at two text sizes; a selection survives the change.
Status: done 2026-09-28
Result:
- CoxTranscriptText `TranscriptTextView.restyle(_:)` builds the text again at the new style and copies only the attributes into the storage: characters, block ranges, selection, open thoughts and every card, prompt tile and thought header (with its hosted view) stay; fonts, decor and structure paragraph styles, `docStarts` and the inset take the new style.
- CoxTranscript `TranscriptView.updateNSView` rebuilds `TranscriptStyle.cox` when `coxAppearance.textScale` changes and restyles inside `TailFollow.around(restyling: true)`; `TailFollow` watches the frame and keeps a reader at the bottom until the next batch.
Deviations:
- `[desktop.transcript]` has no text-size or line-height key, so the trigger is `Appearance.textScale` (DS§3.2, ⌘+/⌘−). `TranscriptStyle` has no line height; token line heights are not applied to the transcript text (question for the creator in `ideas.md`).
- A test-only `textScale:` parameter on `Host.show`.
Check:
- CoxTranscriptText 31 (`TranscriptRestyleTests`: every character's look equals a fresh load; text, ranges, selection, open thought, attachments and inset kept).
- CoxTranscript 26 (`TranscriptTextSizeTests` on the hosted view: selection, ranges and card survive ×1.3, font grows ×1.3, a reader at the bottom stays there; snapshots at 100 % and 130 %).
- After merging into `p37-desktop`: CoxTranscriptText 31/31, CoxTranscript 27/27.
Not done: ⌘+/⌘− wiring (T37.32/T37.22.3).

#### T37.25 Token meter and token popover

Depends: T37.12, T37.24 · Size: ~150 · Files: `…/Molecules/TokenMeter.swift`, `…/Organisms/TokenPopover.swift`
Goal: ↑ sent, ↓ received and live tok/s with a sparkline in the composer; the popover shows turn and session breakdown, first-token time, cost and the context bar (mockup 30).
Check: snapshots idle and streaming; VoiceOver label reads the three numbers; the UI does no arithmetic on them (values come formatted from `cox-app`).
Status: done 2026-09-28
Result:
- CoxUI `Molecules/TokenMeter.swift`: ↑ sent, ↓ received, StatusDot, tok/s and a Sparkline, a CapsuleStyle button that VoiceOver reads with the core's spoken line. `Organisms/TokenPopover.swift`: heading and phase, big tok/s with sparkline, rate line (avg, first token, peak), Turn/Session grid (sent, cache read/write, uncached, received, thinking, cost), context heading with StackedBar and legend, footnote.
- `Composer.State` gains `meter`, `tokens` and a `toggleTokens` intent; `SessionComposer` fills them from `UsageView` (`CoxTranscript/TokenMeter+Usage.swift`). The UI does no arithmetic.
- cox-app `meter_text.rs`: `MeterText`/`MeterRow` carry every figure formatted plus the DS§8 spoken line; `Meter` tracks avg rate, peak rate and session thinking. `UsageView.text` crosses cox-ffi, CoxCore `Convert.swift` and CoxClient `Timeline.swift`.
Deviations:
- About 15 files and 500 non-test lines: the formatted figures cross the FFI, CoxCore and CoxClient.
- `StackedBar` and its `Kind` are public. DESIGN.md rows and the `Usage` line in `docs/design/desktop.md` updated.
- All three fixtures re-recorded for the usage `text` (`read-and-reply.json` in the branch, `edit.json` and `approve-write.json` at the merge).
Check:
- `cargo nextest run -p cox-app -p cox-ffi`: 59/59 (number formats, spoken line, avg/peak/first token in the Meter fold); clippy and fmt clean.
- CoxCore 9/9, CoxModel 30/30, CoxTranscript 13/13, CoxUI 130/130 with new snapshots (meter idle/streaming/open, popover streaming/idle, composer with popover).
- After merging into `p37-desktop`: cox-app and cox-ffi 63/63, CoxModel 39/39, CoxTranscript 27/27, CoxUI 138/138.
Not done: the context window and its split (T37.25.1); app wiring (T37.22.3).

#### T37.23.11 Word-level diff in the desktop hunks

Depends: — · Size: ~80 · Files: `crates/cox-render/src/diffmodel.rs`, `crates/cox-render/Cargo.toml`, `desktop/macos/Packages/CoxTranscript/…`
Goal: `DiffLine` carries the changed word ranges of a paired del/add line, computed in `diffmodel` (so `similar` no longer needs the ratatui feature and the TUI keeps one word-diff path), and the edit card marks them.
Check: a `diffmodel` test for a one-word change; the TUI word-diff snapshots unchanged; an edit-card snapshot with the marked words.
Status: done 2026-09-28
Result:
- `cox-render/src/diffmodel.rs` owns the one word-diff engine: `words()` (similar `from_words`, merged byte ranges), `align`/`Aligned` (moved from `diff.rs`), `replaced()` and `WORD_DIFF_CAP`. `DiffLine.words: Vec<WordRange { start, end }>` (UTF-8 byte offsets); its `spans` are cut at every range edge. The TUI's `diff.rs` draws its dim/add/del spans from the same ranges through `marked()`; `similar` is no longer behind the `ratatui` feature.
- cox-ffi declares `WordRange` in `types.rs`; CoxClient `DiffLine.words`/`WordRange`, CoxCore `Convert.swift` and `TranscriptCard` map it. CoxUI `CodeRun.isChanged` and `attributed(_:mark:)`; `DiffLineView` puts changed runs on the `diff.*Gutter` token. DESIGN.md `DiffLineView` row updated.
Deviations:
- 15 files, about 165 non-moved lines.
- `desktop/macos/Fixtures/edit.json` re-recorded (at the merge again, so it carries both `words` and T37.25's usage `text`); cox-app `scenarios__edit.snap` updated for `words`.
- Words split on whitespace, as in the TUI.
Check:
- `cargo nextest run -p cox-render -p cox-app -p cox-ffi -p cox-tui`: 358/358, including `a_one_word_change_marks_that_word_on_both_lines` and `only_a_paired_line_within_the_cap_carries_words`; no cox-tui snapshot changed. clippy (also `-p cox-render --no-default-features`) and fmt clean.
- CoxModel 33, CoxUI 133 (`onlyAChangedRunSitsOnTheMark`), CoxTranscript 23 with `anOpenedEditCard` light/dark re-recorded, CoxTranscriptText 29.
- After merging into `p37-desktop`: cox-render, cox-app and cox-ffi 114/114; CoxModel 39/39, CoxTranscript 27/27, CoxPlatform 13/13, CoxUI Diff/Code 6/6.
Not done: CoxCore tests (the bindings were checked with `uniffi-bindgen` from a debug library instead; CI runs them).

#### T37.27.5 Pinned approval bar above the composer

Depends: — · Size: ~100 · Files: `desktop/macos/Packages/CoxUI/…`, `desktop/macos/Packages/CoxTranscript/…/SessionComposer.swift`
Goal: a pending approval or question is pinned above the composer (T37.24), with ⌘↩ Allow and ⌘⌫ Deny, while its card stays in the transcript.
Check: a snapshot with a pending approval shows the bar; the shortcut sends `.approve`.
Status: done 2026-09-28
Result:
- CoxUI `Organisms/DecisionBar.swift`: `DecisionBar(content, choose:)`, the mockup's pinned bar: a symbol, "Waiting for you: <command>" in semibold mono, then Allow / For session / Deny; a question reads "Question from cox: …" with its answers as buttons when they fit (`ViewThatFits`). ⌘⏎ (and keypad Enter) allows and ⌘⌫ denies through `WindowKeys`, an app-local key monitor (the focused composer editor takes both keys before a view shortcut); held-key repeats are swallowed.
- CoxTranscript `SessionComposer` shows the bar above `Composer` while `store.session.waiting` is set, sends the choice through `store.session.send` and reports a failure with `store.report`. `DecisionCard.swift`: `SessionStore.waiting`, `Block.waiting` and `DecisionBar.Choice.intent(call:)`; a bash approval shows its command, any other tool "<tool> <summary>". The card stays in the transcript.
- DESIGN.md §6.4 `DecisionBar` row; `docs/design/desktop.md` keyboard table tells the focused card's keys from the pinned bar's.
Deviations: ⌘⏎/⌘⌫ as the card says rather than DT§5.2's ⏎/⎋, because plain ⏎ sends from the composer; the keyboard table now says which applies where.
Check:
- CoxTranscript 31 (`PinnedDecisionTests`: light/dark snapshot of the pending `approve-write` fixture with the bar; ⌘⏎ through `NSApp.sendEvent` sends `.approve(call:, decision: .allow)` and the bar clears when the decision lands; ⌘⌫ denies; block-to-bar mapping). CoxUI 139 (`DecisionBarSnapshotTests`, 3 states × 4 variants). swiftlint and swift-format clean.
- After merging into `p37-desktop`: CoxTranscript 31/31, CoxUI DecisionBar 1/1.
Not done: app wiring (T37.22.3); the composer's ⌘V monitor could reuse `WindowKeys` (T37.27.8).

#### T37.24.9 Caret-aware completion and the failure notice

Depends: — · Size: ~80 · Files: `desktop/macos/Packages/CoxUI/…/Composer.swift`, `desktop/macos/Packages/CoxModel/…/ComposerStore.swift`
Goal: completion uses the token at the caret, not the end of the draft, and `ComposerStore.failure` shows as a `NoticeRow` above the composer.
Check: a UI test completes mid-text; a snapshot with a failure.
Status: done 2026-09-28
Result:
- Completion uses the `@` or `/` word that ends at the caret: `ComposerStore` keeps the editor's selection as UTF-16 offsets (`selectedRange`, `select(_:)`); a pick replaces that word in place, keeps the rest and puts the caret after the insert and one space; no rows while the caret is inside a word or text is selected.
- The composer's `TextEditor` uses a selection binding and edits its own copy of text and selection (`Organisms/ComposerDraft.swift`), sends `.edit` then the new `.select(Range<Int>)`, and takes back what the store returns (after a pick, a recalled prompt, a send or `!`). `NSTextView` reports a keystroke's caret before its text, so reading the text straight from the value made the caret jump to 0.
- `Composer.State.failure` shows as an error `NoticeRow` on its own glass strip above the composer (DS§8 contrast); `SessionComposer` passes `store.failure` and `store.selectedRange`.
Deviations: a fourth source file, `ComposerDraft.swift`, keeps `Composer.swift` under SwiftLint's 400-line limit.
Check:
- `aTokenTypedMidTextIsCompletedInPlaceAndTheCaretFollowsTheInsert` (real key events: types "fix it", moves the caret, types " @", picks with ⏎, caret 17, sends), `aBangTypedIntoTheEmptyComposerLeavesTheEditorEmptyInShellMode`, store test `theTokenAtTheCaretIsCompletedMidTextAndTheRestStays` (non-ASCII UTF-16 offsets), `ComposerFailureTests` (4 variants).
- CoxModel 40, CoxTranscript 29, CoxUI 139; `ComposerFlowTests` 7/7 on two more runs; swiftlint and swift-format clean.
- After merging into `p37-desktop` with T37.27.5: CoxModel 40/40, CoxTranscript 33/33, CoxUI Composer and DecisionBar 5/5.
Not done: a dismiss control on the notice (it clears on the next successful send).

#### T37.29.2 Inspector Plan tab

Depends: — · Size: ~120 · Files: `desktop/macos/Packages/CoxUI/…/Organisms/PlanTab.swift`, `crates/cox-app/…`
Goal: the live todo list with statuses (DT§5), from the structured todo result (T37.3) through a cox-app view; a DS§6 row.
Check: a snapshot per cell; a cox-app test that the latest todo result is the view.
Status: done 2026-09-28
Result:
- cox-app `timeline.rs`: the fold keeps every `todo` result's list with its turn; `Timeline::plan()` returns the latest list (empty before the first `todo` call); a conversation rewind drops the rewound turns' lists; another tool's result of the same shape is ignored. Reaches the app through `Controller::plan()` and `LiveSession::plan()`.
- cox-ffi `SessionHandle::plan()`, a one-expression forward (A90); `TodoItem`, `TodoState` as `#[uniffi::remote]` in `types.rs`.
- Swift: CoxClient `Plan.swift` (`TodoItem`, `SessionClient.plan()`, `FixtureSession(plan:)`); CoxCore conversion in `ChangesConvert.swift` and `LiveSession.plan()`; CoxUI `Organisms/PlanTab.swift` (`PlanTab(state:)`, read-only): one section "Plan · 2 of 5", a box per step (empty pending, `accent` in progress, checked `status.success` when done, struck through in `text.secondary`), "No plan yet" when empty; fixtures in `Previews/PreviewState+Plan.swift`.
- DS§6.4 `PlanTab` row; `plan` on SessionHandle in `docs/design/desktop.md`.
Deviations:
- No "updated 14:03" or "From the model" note: no event carries the time.
- Pulled on request through the FFI like `changes()` (the blocks do not carry the list); about 16 files.
Check:
- `cargo nextest run -p cox-app -p cox-ffi`: 64/64, including `the_plan_is_the_latest_todo_result_and_a_rewind_restores_the_one_before` and `forward_only`; clippy and fmt clean.
- CoxUI 141 (5 PlanTab snapshots), CoxModel 39, CoxCore 11 (`aTodoItemConvertsWithEachState`, against a debug XCFramework); swiftlint and swift-format clean.
- After merging into `p37-desktop`: cox-app and cox-ffi 64/64; CoxModel 40/40; CoxUI PlanTab 3/3; CoxTranscript builds with its tests.
Not done: app wiring (pull again when a `todo` call finishes; T37.22.3).

#### T37.29.6 Open a task's transcript from the Tasks tab

Depends: — · Size: ~120 · Files: `crates/cox-app/…`, `crates/cox-ffi/src/session.rs`, `desktop/macos/Packages/CoxModel/…`
Goal: `SessionHandle::open_task(task)` returns what a Tasks-tab click opens: a subagent's child `SessionId` (from the parent session's children, no protocol change) or, for a background shell, its archived output id; the task row says which kind it is so the tab can label it. The FFI side is a one-expression forward (A90).
Check: a cox-app test over a scripted session with one subagent and one background shell; a CoxModel test that `.open(task:)` resolves to the child session.
Status: done 2026-09-28
Result:
- cox-app `LiveSession::open_task(task)` returns `Option<TaskTarget>`: `Transcript { session }` for a subagent, `Output { archive }` for a finished background shell, `None` for an unknown task or a running shell. It reads the parent's rollout and its children (new Diesel query `Store::children(parent)` in `cox-store/src/queries.rs`); `cox_app::tasks::open` (new `tasks.rs`) pairs the n-th subagent with the n-th child whose first prompt matches the task text, so a fork or handoff child in between is skipped. No protocol change.
- `BlockKind::Task` gains `kind: TaskKind` (Agent or Shell), from the core's `<tool>: …` label through `cox_core::tasks::TaskKind::of`, settled on completion by an exit code or archive.
- cox-ffi: remote enums `TaskKind`, `TaskTarget`; `SessionHandle::open_task`, a one-expression forward.
- Swift: CoxClient `TaskKind`, `TaskTarget`, `SessionClient.openTask` (`FixtureSession(tasks:)`); CoxModel `TaskRow.kind`, `SessionStore.open(task:)`; CoxCore `TaskConvert.swift`, `LiveCoreClient.openTask`. DT§4.3 Task row updated.
Deviations:
- About 230 non-test lines in 16 files: the new block field breaks every exhaustive Swift match (one-token edits in CoxTranscript and CoxTranscriptText).
- `subagent_explore` insta snapshot re-recorded (gains `"kind":"agent"`).
Check:
- `cargo nextest run -p cox-app -p cox-store -p cox-ffi`: 94, including `open_task_finds_the_subagents_session_and_the_shells_output` (Scripted provider, a foreground explore subagent and a background `bash`); clippy and fmt clean.
- CoxModel 40 (`openingATaskResolvesToTheChildSessionOrTheShellOutput`), CoxCore 10 (debug XCFramework), CoxTranscriptText 31, CoxTranscript all but the load-bound benchmark.
- After merging into `p37-desktop` with T37.29.2: cox-app, cox-ffi and cox-store 95/95; CoxModel 41/41, CoxTranscriptText 31/31, CoxTranscript 33/33, CoxUI TasksTab and PlanTab 7/7.
Not done: a kind label or glyph in CoxUI's `TasksTab.Item` (T37.29.8); opening the transcript or output in the app (T37.22.3).

#### T37.24.8 Queue from Rust

Depends: — · Size: ~100 · Files: `crates/cox-app/…`, `desktop/macos/Packages/CoxModel/…/ComposerStore.swift`
Goal: `Intent::Queue` carries attachments and the status patch reports the queue length, so `ComposerStore` stops deriving it from block turn numbers and a draft with attachments can be queued.
Check: cox-app tests for both; the Swift count comes from the patch.
Status: done 2026-09-28
Result:
- cox-app `Intent::Queue { text, attachments }` (`intent.rs`); `Send` and `Queue` share the rule that a turn needs text or an attachment.
- New `TimelinePatch::Status { status: Status { queued } }` (`patch.rs`): `LiveSession` counts a queued turn when it is sent and uncounts it when it starts (`Controller::enqueue`/`dequeue`); only the latest status stays queued, and a `Reset` or `snapshot()` keeps the status and the meter.
- cox-ffi declares `Status` and the queue attachments in `types.rs`.
- Swift: CoxClient `Status`, the `.status` patch and `.queue(text:attachments:)`; `SessionStore.status`; `ComposerStore.queued` reads `session.status.queued` (the turn-number arithmetic and the "attachments cannot wait" refusal are gone; queuing clears the attachments); CoxCore `Convert.swift`; `TranscriptPatches` ignores `.status`. DT§4.3 patch and intent lines and the DT§4.6 CoxModel row updated.
Deviations:
- The DT§4.3 `Status` patch lands here with `queued` only; T37.24.7 adds mode, model and effort.
- 20 files, +284/−89 with tests: every layer the patch and intent cross.
Check:
- `cargo nextest run -p cox-app -p cox-ffi`: 66/66, including `a_queued_turn_carries_its_attachments_and_counts_until_it_starts`, `the_status_patch_counts_turns_queued_and_not_yet_started`, `only_the_latest_status_stays_queued_and_a_reset_keeps_it` and `forward_only`; clippy and fmt clean.
- CoxModel 39, CoxCore 11, CoxTranscriptText 31, CoxTranscript 29 (ComposerFlowTests 5/5; the streaming benchmark missed its busy budget under load).
- After merging into `p37-desktop` with T37.24.9, T37.27.6, T37.29.2, T37.29.5 and T37.29.6: cox-app, cox-ffi and cox-store 101/101; CoxCore 12/12 against a dev-profile XCFramework; CoxModel 44/44; CoxTranscript 35/35; CoxUI Approval, Composer and DecisionBar 10/10.
Not done: app wiring (T37.22.3).

#### T37.27.6 Approval Edit… and the grant preview

Depends: — · Size: ~150 · Files: `crates/cox-app/src/timeline.rs`, `desktop/macos/Packages/CoxUI/…/ApprovalCard.swift`, `desktop/macos/Packages/CoxTranscript/…/DecisionCard.swift`
Goal: the approval block carries the call input and what "Allow for session" would grant (`grants_for`); the card shows the grant and Edit… edits the input into `Decision.edit`.
Check: a cox-app test that the block carries both; a UI test that an edit sends the edited JSON; a snapshot showing the grant.
Status: done 2026-09-28
Result:
- The approval block carries the call's `input` (JSON) and `grants`, the subjects "Allow for session" would record, from `grants_for` through `cox_core::permission` (`crates/cox-app/src/patch.rs`, `timeline.rs`; `#[uniffi::remote]` in `cox-ffi/src/types.rs`). The permission decision stays in the engine: Swift shows the grant and sends `Decision.edit`.
- CoxUI `ApprovalCard`: `Content` gains `grant` and `input`; new `init(_:act:edit:)` beside the unchanged `init(_:act:)`. A line "Allow for session grants: bash: a · b"; Edit… swaps the command well for a JSON field with Run edited (enabled while the draft parses) and Cancel.
- CoxTranscript `DecisionCard` fills the grant and a pretty-printed input; an edit becomes `.approve(call:, decision: .edit(input:))`. CoxClient `BlockKind.approval` and its decoding, CoxCore `Convert.swift`, every Swift `.approval` pattern and the DESIGN.md ApprovalCard row follow.
Deviations:
- About 20 files: two new associated values reach every `.approval` pattern, both mirrors, the fixtures and snapshots.
- All three fixtures re-recorded.
- The UI test clicks through AppKit's private `_FocusRingView` (no accessibility tree off-screen); only the test depends on it.
- At the merge, T37.27.5's bar mapping and `PinnedDecisionTests` follow the 9-value case, and the test reads the waiting call from the recording instead of a fixed id.
Check:
- `cargo nextest run -p cox-app -p cox-ffi`: 64/64, including `an_approval_carries_the_input_and_what_allow_for_session_grants`; three scenario snapshots differ only by `input` and `grants`. clippy and fmt clean.
- CoxUI 141 (new grant snapshots; ApprovalCard click-and-type UI tests), CoxModel 39, CoxTranscriptText 31, CoxPlatform 13, CoxTranscript 29 (DecisionCard snapshot re-recorded), CoxCore 10.
- After merging into `p37-desktop`: cox-app and cox-ffi 72/72; CoxModel 41/41, CoxPlatform 13/13, CoxTranscriptText 31/31, CoxTranscript 35/35, CoxUI Approval/Composer/DecisionBar 10/10; CoxCore 12/12 after T37.29.5.
Not done: app wiring (T37.22.3).

#### T37.29.5 Inspector Info tab

Depends: — · Size: ~80 · Files: `desktop/macos/Packages/CoxUI/…/Organisms/InfoTab.swift`, `crates/cox-app/…`, `crates/cox-ffi/src/session.rs`
Goal: session id, cwd, worktree, config provenance and rollout path as a KeyValueGrid, from a new `SessionHandle::info()` forward.
Check: a snapshot per cell; a cox-app test for `info()`.
Status: done 2026-09-28
Result:
- cox-app `info.rs` (`Info`, `ConfigSource`, `build`) and `LiveSession::info()`: session id, cwd, rollout path (new `cox_store::Store::rollout_path`, which the store's two readers now use too), the linked worktree through `cox_tools::git::linked`, and the config layers that set at least one key, in load order, with key count and file, from `settings::view` over cox-config's `source_of`.
- cox-ffi `SessionHandle::info`, a one-expression async forward; `Info`, `ConfigSource` as `#[uniffi::remote(Record)]`.
- Swift: CoxClient `Info.swift` and `SessionClient.info()` (`FixtureSession(info:)`); CoxCore `InfoConvert.swift` (the `Linked` conversion is one shared `CoxClient.Linked.init` in `ChangesConvert.swift`); CoxModel `InfoTabState` (paths shortened to `~`), `SessionStore.infoTab()`; CoxUI `Organisms/InfoTab.swift`: "Session" and "Config" `InspectorSection`s, each a KeyValueGrid, previews in `PreviewState+Info.swift`. DS§6.4 `InfoTab` row.
Deviations:
- More than 3 files: the same record → convert → client → state → view chain as T37.29.1.
- Long values (ULID, rollout path) wrap: KeyValueGrid has no truncation mode.
Check:
- `cargo nextest run -p cox-app -p cox-ffi -p cox-store`: 91, including `info_counts_each_layer_that_set_a_key_with_its_file` and the live `info_names_the_session_its_cwd_rollout_and_the_user_config_it_read`; clippy and fmt clean.
- CoxUI 140 (Info snapshots), CoxModel 42 (3 new), CoxCore 10.
- After merging into `p37-desktop` with T37.29.2 and T37.29.6: cox-app, cox-ffi and cox-store 101/101; CoxCore 12/12; CoxModel 44/44; CoxUI InfoTab 2/2; CoxTranscript builds with its tests.
Not done: app wiring (T37.22.3).

#### T37.23.10 Thought duration in the thinking header

Depends: — · Size: ~150 · Files: `crates/cox-protocol/…`, `crates/cox-core/src/turn.rs`, `crates/cox-app/src/timeline.rs`, `desktop/macos/Packages/CoxTranscriptText/…`
Goal: a thinking block carries how long the model thought (from its first to its last reasoning delta, as `cox-app` folds the events), and the fold header reads "Thought for 12 s" once it ends and "Thinking" while it streams (DS§6.3).
Check: a `cox-app` test that a folded reasoning run records its duration and replay gives the same value; a snapshot of the header in both states.
Decided by the creator (A91): `cox-core` gives streamed reasoning its own `Thinking` item (`ItemStarted` → deltas → `ItemDone`) and emits `Event::ThinkingDone { item, duration_ms }` with the first-to-last-delta time, which the rollout keeps; `Timeline` folds the live deltas and the duration. Today live reasoning deltas are keyed to the reply's `AssistantMessage` item and `Timeline` drops them (`cox-core/src/turn.rs`). Regenerate `docs/protocol.jsonschema` through its drift test.
Status: done 2026-09-28
Result:
- `cox-protocol`: new `Event::ThinkingDone { item, duration_ms }` (A91); `docs/protocol.jsonschema` regenerated through its drift test.
- `cox-core/src/turn.rs`: `consume_provider` opens a `Thinking` item on the first reasoning delta and closes it with `ThinkingDone` then `ItemDone` before any other provider event, or at stream end; the duration runs from the first to the last delta (`Instant`, as tool durations do; cox-core has no clock trait). The rollout keeps `ThinkingDone`, so a replay reads the same value. The reply's `AssistantMessage` `ItemStarted` moved from `session.rs` into `consume_provider`, after the thought closes.
- cox-app (`patch.rs`, `timeline.rs`, `coalesce.rs`): `BlockKind::Thinking { text, duration_ms }`, set by `ThinkingDone`; cox-ffi mirrors the field.
- Swift: `.thinking(text:durationMs:)`; `TranscriptCards.thoughtTitle` reads "Thinking" while the thought streams and "Thought for N s" once it ends (rounded, never 0); `TranscriptView` passes it to `ThinkingHeader`. DT Thinking row names the new events.
Deviations:
- Every surface now lists the thought before the reply, and the TUI and plain surfaces no longer mix reasoning into the reply text; without reasoning the event order is unchanged. The TUI needed an explicit `ThinkingDone` arm.
- 22 files, +323/−36 with tests and snapshots.
Check:
- nextest: cox-protocol 102; cox-core and cox-app 337 (`streamed_thought_is_its_own_item_closed_before_the_reply`, `folded_thought_records_its_duration_and_replay_keeps_it`); cox-acp and cox-tui 265; cox 143; cox-ffi and cox-app 64 with `forward_only`. `cargo check` on every crate that matches on `Event`; clippy and fmt clean.
- CoxTranscriptText 32 (header-title test), CoxModel 39, CoxTranscript 28 (`ThoughtHeaderSnapshotTests` light/dark, both states), CoxCore 10.
- After merging into `p37-desktop`: cox-protocol, cox-core, cox-app and cox-ffi 457/457; with T37.23.12, cox-render, cox-app, cox-ffi and cox-tui 378/378; CoxCore 12/12 (dev-profile XCFramework), CoxModel 44/44, CoxPlatform 13/13, CoxTranscriptText 33/33, CoxTranscript 37/37.
Not done: an empty signed `Thinking` item (T39.2, Gemini over Chat) still opens a block with no `ThinkingDone` (T37.23.14).

#### T37.23.12 Heading and quote structure from StyledDoc

Depends: — · Size: ~150 · Files: `crates/cox-render/src/…` (`StyledDoc`), `crates/cox-ffi/src/types.rs`, `desktop/macos/Packages/CoxTranscriptText/…`
Goal (A92): each `StyledDoc` block carries its kind (heading, quote, list item, …), level or depth and marker apart from the text, so the desktop draws a heading without its `#` markers, a quote with a real bar at its depth and a list item with its marker in the gutter. The TUI keeps printing as today. "Copy as Markdown" still returns the source Markdown.
Check: a `cox-render` test that a heading, a nested quote and a list item carry level and marker and their text without them; a TUI snapshot unchanged; CoxTranscriptText light/dark snapshots of a reply with each kind; a copy test that returns the Markdown source.
Status: done 2026-09-28
Result:
- `cox-render/src/doc.rs`: each line of a `Block::Text` is a `TextLine { quote, depth, marker, spans }`; a heading's `#` run leaves the text and its level stays in `TextKind::Heading` (A92). `markdown.rs` fills the fields; the ratatui `render` puts the bars, marker and `#` run back, so the TUI prints as before. Two parser fixes: the kind resets after a heading ends, and a quote that goes on after a list inside it is a quote again.
- cox-ffi declares `TextLine` as `#[uniffi::remote(Record)]`.
- Swift: CoxClient `TextLine` and its decoding; CoxCore `Convert.swift`; `DocMarkdown` rebuilds `#`, `-` or the number and `>` from the fields (no more guessing from the text). CoxTranscriptText draws a heading without `#`, a list marker right-aligned in the gutter, and a quote line indented past one bar per level: a new `QuoteFragment` in `TranscriptStructure.swift`, hooked into `DecorLayout` (`TranscriptDecor.swift`), in the thought's rule colour and width.
Deviations:
- About 350 source lines in 10 files: the line type crosses cox-render, cox-ffi, CoxClient, CoxCore and CoxTranscriptText.
- The three fixtures re-recorded (and again at the merge); the cox-render doc snapshot and 12 cox-app scenario snapshots changed shape only.
Check:
- `cargo nextest run -p cox-render -p cox-app -p cox-ffi -p cox-tui`: 366, including `headings_quotes_and_items_carry_level_and_marker_apart_from_their_text` and `a_list_in_a_quote_keeps_its_bars_and_the_quote_resumes_after_it`; no cox-tui snapshot changed; clippy and fmt clean.
- CoxModel 39, CoxTranscriptText 32 (`aQuoteLineLaysOutWithItsBarsAndProseWithout`), CoxTranscript 32 (`replyWithEveryBlockKind` light/dark re-recorded and looked at; `copyAsMarkdownGivesTheStructureBack`, new `copyAsMarkdownOfALoadedReplyGivesItsSource`), CoxCore 10.
- After merging into `p37-desktop`: cox-render, cox-app, cox-ffi and cox-tui 378/378; CoxCore 12/12, CoxModel 44/44, CoxPlatform 13/13, CoxTranscriptText 33/33, CoxTranscript 37/37.
Not done: per-level heading sizes (pending token decision); the quote bar uses the thought's hairline, faint in light mode (token question in `ideas.md`).

#### T37.23.14 Empty signed thinking items close like streamed thoughts

Depends: — · Size: ~60 · Files: `crates/cox-core/src/turn.rs`, `crates/cox-app/src/timeline.rs`
Goal: T39.2 keeps a tool call's signature as an empty signed `Thinking` item (Gemini over Chat). Since T37.23.10 a streamed thought is closed with `ThinkingDone`, but these empty items open a Timeline thinking block that never gets a duration; the desktop only hides it because its text is empty. Either the core closes them the same way or `Timeline` does not open a block for a signature-only item.
Check: a cox-app test folding a signature-only thinking item leaves no open thinking block; the T39.2 chat-wire test still passes.
Status: done 2026-09-28
Result:
- `crates/cox-app/src/timeline.rs`: an `ItemStarted` of `ItemKind::Thinking` with empty text and `signature: Some(_)` (T39.2's signature carrier) opens no block, so its `ItemDone` is a no-op and nothing waits for a duration. A streamed thought (`signature: None`) still opens a block closed by `ThinkingDone` (T37.23.10). The core is unchanged, so the rollout and provider history keep the signature.
Deviations: the Timeline option, not a core `ThinkingDone` for these items (that would close an empty thought at 0 ms and still show it).
Check:
- `cargo nextest run -p cox-core -p cox-app`: 349 passed, 1 skipped, including `signature_only_thinking_item_leaves_no_open_thinking_block`; `-p cox-core --test chat_wire`: 1 passed; clippy and fmt clean.
Not done: how the TUI and ACP show these items was outside the card.

#### T37.29.7 Deleted files and created-file counts in the Changes tab

T37.29.1 left two gaps. CoxUI's `ChangedFileRow.Change` has no `deleted` case, so a `FileChange::Deleted` from `changes()` has no glyph. A `write` that creates a file carries no diff, so the row reads `+0 −0` instead of the new file's line count.

Done means: a `deleted` case with its glyph and snapshot in CoxUI, the mapping in `ChangesTabState`, and a created file counted as all-added lines in `crates/cox-app/src/changes.rs`. Check: the `changes.rs` unit test covers a created file's count; a CoxUI snapshot shows a deleted row.
Status: done 2026-09-28
Result:
- `crates/cox-app/src/changes.rs`: `written(events)` maps a `write` call to its `content` input's line count from the session's rollout (read from the store, not from disk); `build` counts a `Created` row without a diff from it. `LiveSession::changes()` reads the rollout as `open_task` does. A `write` over an existing file and a shell still add nothing.
- CoxUI `ChangedFileRow.Change.deleted` with SF Symbol `trash`, a `#Preview("deleted")`, `PreviewState.deletedFile` and 4 `changedFileRow-_.deleted-*` snapshots; DESIGN.md DS§3.7 row and the ChangedFileRow glyph list.
Deviations: `crates/cox-app/tests/app.rs`'s `changes_lists_…` now expects `new.rs` at +1; CoxModel needed a test change only (`ChangesTabState` already carried `FileChange.deleted`).
Check:
- `cargo nextest run -p cox-app`: 69/69, including `a_file_a_write_created_counts_its_content_as_added_lines`; clippy and fmt clean. CoxUI InspectorRow snapshots recorded and passed; CoxModel 44/44; swiftlint and swift-format clean.
- After merging into `p37-desktop`: cox-app 70/70.
Not done: nothing.

#### T37.29.8 Task kind in the Tasks tab

Depends: — · Size: ~40 · Files: `desktop/macos/Packages/CoxUI/…/Organisms/TasksTab.swift`, `desktop/macos/Packages/CoxModel/…/TaskRows.swift`
Goal: T37.29.6 gave `TaskRow` a `kind` (subagent or background shell); `TasksTab.Item` shows it as a glyph and a label ("Open transcript" for a subagent, "Open output" for a shell).
Check: TasksTab snapshots with one row of each kind.
Status: done 2026-09-28
Result:
- CoxUI `TasksTab.Kind` (`agent`, `shell`) in `Organisms/TasksTab.swift`: a subagent row shows `person.2` and "Open transcript", a shell row `terminal` and "Open output" with `doc.text`; the intent stays `.open(task:)`. The fixture's bash row is a shell, so the 4 `tasksTab` snapshots show one row of each kind; DESIGN.md's TasksTab row updated.
Deviations: CoxModel `TaskRows.swift` unchanged — `TaskRow.kind` already maps field for field.
Check: CoxUI `ChangesTab|InspectorRow|TasksTab|Inspector` 18/18 including `aShellRowOpensItsOutputUnderItsOwnGlyph`; CoxModel 44/44; swiftlint and swift-format clean.
Not done: app wiring (T37.22.3).

#### T37.28.1 Rewind timeline: restore code, conversation or both

Depends: — · Size: ~90 · Files: `…/Organisms/RewindTimeline.swift`, `CoxModel/…/Rewind.swift`, `crates/cox-app/tests/app.rs`
Goal: pick a checkpoint from `changes().checkpoints` and a scope and send the existing `Intent::Rewind`.
Check: a fixture rewind restores the expected files in a scratch tree.
Status: done 2026-09-28
Result:
- CoxUI `Organisms/RewindTimeline.swift`: the checkpoints oldest first as CheckpointRows, the selected one raised, each with Restore code (`doc.text`), Restore conversation (`text.bubble`) and Restore both (`arrow.uturn.backward`), reporting `.rewind(checkpoint:code:conversation:)`; previews in `Previews/PreviewState+Rewind.swift`; DESIGN.md DS§6.4 row.
- CoxModel `Rewind.swift`: `SessionStore.rewind(checkpoint:code:conversation:)` turns the Changes-tab checkpoint (the turn) into `Intent.rewind`; the core's rewind is reused as is.
- Bug found by the Check and fixed in `crates/cox-sandbox/src/path.rs`: `confine`'s lexical pre-check compared a checkpoint row's canonical path (`/private/var/…`) with the root as opened (`/var/…`), so every restore under a symlinked cwd was refused and reported as "too large to restore". The pre-check now also matches the canonical roots; the canonical check after it still decides.
Deviations: the trust-guard fix above (reviewed by the orchestrator).
Check:
- `cargo nextest run -p cox-sandbox -p cox-app -p cox-tools`: 210 passed, 1 skipped, including `rewinding_code_to_a_checkpoint_restores_its_files_and_keeps_the_conversation`, `rewinding_code_and_conversation_leaves_nothing_to_review` and `a_canonical_path_under_a_symlinked_root_is_confined`; clippy and fmt clean. CoxModel 45/45; CoxUI RewindTimeline, ChangesTab, InspectorRow 13/13; swiftlint and swift-format clean.
- After merging into `p37-desktop`: cox-sandbox and cox-app 87/87.
Not done: app wiring (T37.22.3, T37.32). Open questions: the scope of the Changes tab's plain Rewind; whether Review shows the net diff against disk or the model's calls after a code-only rewind; the "too large to restore" notice also covers any failed restore.

#### T37.28 Review pane and rewind timeline

Depends: T37.23, T37.21.9 · Size: split at claim · Files: `…/Organisms/ReviewPane.swift`, `…/Organisms/RewindTimeline.swift`
Goal: DT§5 review of the session's changes and rewind to a checkpoint (code, conversation or both).
Check: fixture rewind restores the expected files in a scratch worktree.
Status: done 2026-09-28
Split at claim into T37.28.1 (rewind timeline, done), T37.28.2 (review pane), T37.28.3 (per-file revert, needs an amendment) and T37.28.4 (line comments).

#### T37.30.5 Coloured page tiles in Settings

Depends: — · Size: ~80 · Files: `desktop/design/tokens/color.*.json` (and the generated token outputs), `desktop/design/DESIGN.md`, `desktop/macos/Packages/CoxUI/…` (Settings sidebar)
Goal (A96): `tile.settings.<page>.top/bottom/glyph` tokens mapped to macOS system colours (e.g. `systemBlue`, `systemGray`), with high-contrast variants; the Settings sidebar draws each page's symbol on its coloured tile as in the mockup instead of the plain symbol, and DESIGN.md drops the "need colour tokens that do not exist yet" note.
Check: the token build's own check; CoxUI snapshots of the Settings sidebar in light, dark and high contrast.
Status: done 2026-09-28
Result:
- `tile.settings.<page>.top/bottom/glyph` in `desktop/design/tokens/color.light.json` and `color.dark.json` (A96), flat as the mockup draws them, white glyph: General systemGray, Models & Providers systemPurple, Permissions systemOrange, Sandbox systemGreen, Budget systemTeal, MCP Servers systemBlue, Plugins systemIndigo, Appearance systemPink, Advanced systemBrown (the mockup has no Advanced tile). Values are macOS 27.0's resolved light, dark and increased-contrast system colours; regenerated `color.*-hc.json`, `tokens.css` and 27 colorsets.
- `IconTile.init(face:glyph:symbol:)`; `InspectorRow` takes a leading glyph view (its `symbol:` init still works through `SymbolGlyph`); `SettingsSidebar` leads each page with `SettingsPage.tile`; `#Preview("models, high contrast")`. DESIGN.md tile row, `IconTile`/`SettingsSidebar` rows and the Settings paragraph updated.
Deviations: `high-contrast.mjs` reads a pinned value from `$extensions.cox.highContrast` so High Contrast uses the system's own increased-contrast colour, and its check confirms the pin; in dark High Contrast the glyph (not the face) moves to reach 3:1 (mid-grey). `IconTile.swift` and `InspectorRow.swift` beyond the card's files; ~125 lines.
Check:
- `npm ci && npm run build`: light-hc and dark-hc 184 pairs pass, rebuild gives no diff; `npm run check` passes. `settingsSidebar` and every Settings window snapshot re-recorded on purpose; new `settingsSidebarInHighContrast`. CoxUI `Settings|AtomSnapshotTests|InspectorRow|ChangesTab|ToolMolecule` 31, `TasksTab|Inspector` 11; swiftlint and swift-format clean.
- After merging into `p37-desktop`: CoxUI `Settings|InspectorRow|ChangesTab|TasksTab|Inspector|RewindTimeline|AtomSnapshotTests` 40/40.
Not done: `mockups.html` keeps inline hex tile colours (T37.17.2).

#### T37.23.13 Transcript text size and line height from config and tokens

Depends: — · Size: ~100 · Files: `crates/cox-config/…` (`[desktop.transcript]`), `docs/config.jsonschema`, `docs/config.md`, `desktop/macos/Packages/CoxModel/…`, `desktop/macos/Packages/CoxTranscriptText/…`
Goal (A93): `[desktop.transcript]` gets `text_size` and `line_height`; the desktop builds `TranscriptStyle` from them together with `Appearance.textScale` (⌘+/⌘−) and applies the token line heights to the transcript text, restyling in place through T37.23.6's `restyle(_:)`.
Check: the config-schema drift test; a CoxModel test that the keys reach the style; CoxTranscriptText snapshots at two sizes and line heights.
Status: done 2026-09-28
Result:
- `[desktop.transcript]` gets `text_size` (points at 100 %, 10–24, default 13.5 — the `font.transcript` size) and `line_height` (a multiple of the size, 1–2.5, default 1.55) in `crates/cox-protocol/src/config.rs` and `default.toml` (A93), range-checked like the appearance keys; `docs/config.jsonschema` and `docs/config.md` regenerated.
- CoxModel `TranscriptSettings.swift`: `DesktopTranscript` and `SettingsStore.transcript` read the settings view the way `[desktop.appearance]` does; one shared `SectionRows` decoder.
- CoxTranscriptText `TranscriptLineHeights.swift`: `TranscriptStyle.LineHeights` (body, code, heading, thought) as paragraph line spacing, computed like CoxUI's `.textStyle`; every prose paragraph carries a paragraph style. CoxTranscript `TranscriptView.text(size:lineHeight:)` builds the style from textScale × text_size ÷ 13.5 and restyles in place through `restyle(_:)`.
Deviations: more than 3 files; `Decor.init` gives the bubble and thought paragraphs their spacing; `TailFollow` lays out the viewport before scrolling to the true bottom (the last line now has spacing below it); six transcript snapshot pairs re-recorded; the units (points, a multiple of the size) are the agent's choice.
Check:
- `cargo nextest run -p cox-protocol -p cox-config`: 120/120 with both drift tests; `-p cox-app -E 'test(settings)'` 5/5; clippy and fmt clean. CoxModel 46, CoxTranscriptText 33, CoxTranscript 39 (`TranscriptLineHeightTests`, snapshots at 13.5/1.55 and 17/2.0). Real binary under a scratch `COX_HOME`: `config set`/`show` give `text_size = 16.0`, `line_height 3` is rejected.
- After merging into `p37-desktop`: cox-protocol and cox-config 120/120; CoxModel 47, CoxTranscriptText 33, CoxTranscript 39.
Not done: the benchmark (skipped) should re-check the per-batch viewport layout; the app passing `SettingsStore.transcript` into the view waits on T37.32.

### T50.7. `just test` runs only what a change can break

Model: mid-tier · Status: done 2026-09-28 · Depends: — · Size: ~80 · Files: `justfile`, a script under `scripts/` if the recipe needs one, `AGENTS.md` (Commands), `toolchain.md` if a tool is added

Goal (A99): `just test` runs the nextest tests of the workspace crates changed since a git ref — `just test --changed-since <ref>`, default the merge-base with `origin/main`, committed and uncommitted changes both — plus every crate that depends on them (nextest's `rdeps()` filterset over the packages `cargo metadata` says own the changed files). A change outside every crate that can affect all of them (`Cargo.toml`, `Cargo.lock`, `.cargo/`, `mise.toml`, `justfile`, `rust-toolchain*`) runs the whole workspace; a change that touches no crate runs nothing and says so. Prefer nextest's own filtersets or a maintained tool over custom mapping code. The old full run (`cargo nextest run --workspace`, then `dunnage`) becomes `just check-all`; CI keeps running the whole workspace. Swift packages are out of scope.

Check: `just test --changed-since HEAD` with one edited leaf crate runs only it and its dependents; an edited `Cargo.lock` runs the workspace; `just check-all` runs the workspace; AGENTS.md lists both.

Done when: the Check passes and the three AGENTS.md commands are clean.

Result:
- `just test [--changed-since REF] [--dry-run] [nextest args…]` (A99): the recipe passes its arguments through `[positional-arguments]` to `scripts/changed_tests.py` (stdlib only, run with `uv run --no-project python`). It collects the files changed since REF (default the merge-base with `origin/main`; committed, staged, unstaged and untracked), maps each to the crate that owns it with `cargo metadata --no-deps`, and runs `cargo nextest run --workspace -E 'rdeps(=a) | rdeps(=b)'`. A change to `Cargo.toml`, `Cargo.lock`, `.cargo/`, `mise.toml`, `justfile` or `rust-toolchain*` runs the workspace; no crate changed prints "nothing to run" and exits 0.
- The old full run plus `dunnage` is `just check-all`; CI unchanged. AGENTS.md Commands and two `toolchain.md` rows updated; 6 stdlib unit tests in `scripts/test_changed_tests.py`.
- Ready tools checked 2026-09-28 (sources in the commit body): cargo-delta 0.4.0 (best-effort mapping, runs everything when it finds nothing, no prebuilt binary), cargo-affected (coverage builds, "extremely early"), cargo-rail (large, own config), cargo-test-changed (last release 2025-04-04); nextest has no git-based filter.
Deviations: a changed file outside every crate also selects any crate whose code names it by path (a `docs/config.jsonschema` change runs `rdeps(=cox-config) | rdeps(=cox-plugin-api)`), so a schema or fixture change alone still runs its drift test. `init.rs` and `evals/hooks/verify.sh` mention `just test` for other projects' commands and were left alone.
Check:
- Dry runs: a `cox-sanitize` edit gives `rdeps(=cox-sanitize)`; an untracked file in `cox-patch` gives `rdeps(=cox-patch)`; an edited `Cargo.lock` runs the workspace; `ideas.md` alone or a clean tree prints "nothing to run"; `just --dry-run check-all` expands to the old run plus dunnage; nextest parses the generated filter. Unit tests 6/6; fmt clean.
- After merging into `p37-desktop`: `python -m unittest test_changed_tests` OK; a `cox-sanitize` edit dry-runs `rdeps(=cox-sanitize)`.
Not done: `check-all` itself was not run (load). `scripts/leftovers.sh` (in `just check`) already fails on `p37-desktop` before this change, on done.md/compat.md entries.

#### T37.19.6 Increase Contrast in `Appearance`

Depends: — · Size: ~60 · Files: `desktop/macos/Packages/CoxUI/Sources/CoxUI/Foundations/Appearance.swift`
Goal (A100): when the system asks for more contrast (`colorSchemeContrast == .increased`), `Appearance` turns the specular sweep off (`MaterialToken.solidSpecular`) and raises window and pane opacity by A89's quarter rule — `opacity' = 1 − (1 − opacity) × glassKeep`, with a new `material.highContrast.glassKeep = 0.25` token in `base.json` regenerated into `MaterialToken`; Reduce Transparency still wins and forces Solid — the part of A89's High Contrast rule that lives in `material.*` numbers rather than colour tokens (T37.17.1 did the colours).
Check: a snapshot per material with increased contrast shows no sweep and a more opaque glass; the default snapshots are unchanged.
Status: done 2026-09-28
Result:
- `desktop/macos/Packages/CoxUI/Sources/CoxUI/Foundations/Appearance.swift`: `EffectiveAppearance` reads `\.colorSchemeContrast`; `effective(…, increaseContrast:)` (default `false`) records it; under Increase Contrast `specular` is `solidSpecular` and `backgroundOpacity` applies A100's rule `1 − (1 − opacity) × MaterialToken.highContrastGlassKeep` to window, pane and readable glass. Reduce Transparency still wins and forces Solid.
- `material.highContrast.glassKeep = 0.25` in `desktop/design/tokens/base.json`, generated into `Tokens.swift`; `high-contrast.mjs` reads it instead of its own `GLASS_KEEP` and rejects a value outside [0, 1) (the high-contrast palettes came out byte-identical). DESIGN.md §1.6, §3.5, §6.1 and §8 document it.
Deviations: `Specular.swift` checks the effective specular instead of the material (2 lines; Solid unchanged); 7 files, of which `Appearance.swift` and `Specular.swift` are hand-written source.
Check:
- `npm ci && npm run build` (184 pairs pass in each high-contrast palette) and `npm run check`. CoxUI Foundations, Appearance, ButtonStyle, CapsuleStyle, MaterialPicker, AppearancePopover 32/32 with 6 new `glassPaneIncreasedContrast` snapshots (looked at: no sweep, denser glass), default snapshots unchanged; SettingsScreen 7/7; 3 new unit tests; swiftlint and swift-format clean.
- After merging into `p37-desktop`: CoxUI `Foundations|Appearance|ButtonStyle|CapsuleStyle|MaterialPicker|Settings|GlassPane` 42/42.
Not done: nothing.

#### T37.27.8 One app-local key monitor for the composer and the decision bar

Depends: — · Size: ~40 · Files: `desktop/macos/Packages/CoxUI/…/Organisms/Composer.swift`, `desktop/macos/Packages/CoxUI/…/Organisms/DecisionBar.swift`
Goal: T37.24.5's private `ComposerPaste.Monitor` and T37.27.5's `WindowKeys` both install an app-local `NSEvent` key monitor scoped to one window; keep one helper (`WindowKeys`, moved to its own file) and let the ⌘V paste use it.
Check: `ComposerFlowTests` paste tests and `PinnedDecisionTests` pass unchanged.
Status: done 2026-09-28
Result:
- `WindowKeys` moved to `CoxUI/Sources/CoxUI/Foundations/WindowKeys.swift` with a shared `WindowKeys.holds(_:only:)` modifier check; the composer's private `ComposerPaste.Monitor` NSView is gone — `ComposerPaste` is an enum that builds the ⌘V handler and hands it to `WindowKeys`; `DecisionBar` uses the same file.
Deviations: none.
Check:
- CoxTranscript `ComposerFlowTests|PinnedDecisionTests` 11/11; swiftlint and swift-format clean.
- After merging T37.24.7, T37.23.9, T37.25.1 and T37.23.16 into `p37-desktop` (fixtures re-recorded): `just test --changed-since` 1511 passed, 5 skipped; clippy on the changed crates and fmt clean; CoxCore 12, CoxModel 48, CoxTranscriptText 34, CoxTranscript 45, CoxUI 158.
Not done: nothing.

#### T37.24.7 Composer status chips

Depends: — · Size: ~120 · Files: `desktop/macos/Packages/CoxUI/…/Composer.swift`, `desktop/macos/Packages/CoxModel/…`
Goal: the mode chip (⇧⇥ cycles), model · effort, and the think toggle under the composer (mockup), driven by a Swift mirror of `TimelinePatch::Status`.
Check: snapshots in the four cells; ⇧⇥ sends `setMode`.
Status: done 2026-09-28
Result:
- `cox_app::Status` gains `mode`, `next_mode`, `model` and `effort`, kept by `crates/cox-app/src/status.rs`: seeded from the session's config (the core writes the opening mode only to the rollout, T50.4), then `StateChanged`, `TurnStarted{Main}` and `ModelSwitched{Code}`; `Controller::open` puts it in the first pull. `next_mode` moved from cox-tui to cox-permission so the TUI and the desktop cycle in one order (cox-tui re-exports it). Mirrors in cox-ffi, CoxClient and CoxCore (`ConvertStatus.swift`).
- The composer shows a mode chip with a ⇧⇥ keycap and a model · effort chip; ⇧⇥ or a click sends `.setMode` with the core's `nextMode` — no new intent. DESIGN.md and desktop.md rows updated.
Deviations: ~34 files, mostly snapshots and fixtures; the three fixtures, the PinnedDecision snapshot and the chip-shortcut snapshot re-recorded.
Check:
- cox-app 72; cox-tui, cox-permission and cox-ffi 260; clippy and fmt clean. CoxModel 45, CoxCore 12 (dev XCFramework), CoxTranscript 38, targeted CoxUI suites; `shiftTabAsksForTheModeTheCoreNamesNext`.
- After merging T37.24.7, T37.23.9, T37.25.1 and T37.23.16 into `p37-desktop` (fixtures re-recorded): `just test --changed-since` 1511 passed, 5 skipped; clippy on the changed crates and fmt clean; CoxCore 12, CoxModel 48, CoxTranscriptText 34, CoxTranscript 45, CoxUI 158.
Not done: the think toggle — what it does needs the creator's decision (one turn per click, sticky, or extended thinking on/off through a new Submission); moved to T37.24.10.

#### T37.23.9 Prompt bubble glass, elevation and hover actions

Depends: — · Size: ~100 · Files: `desktop/macos/Packages/CoxTranscriptText/…`, `desktop/macos/Packages/CoxTranscript/…`
Goal: the user bubble drawn by `DecorFragment` gets DS§6.3's glass sweep and e2 elevation from tokens, with a gap between the prompt text and its tile row; hovering a prompt shows its Edit-and-resend and Copy actions, which reach the composer and the pasteboard.
Check: light/dark snapshots of a prompt at rest and hovered; a test that Copy puts the prompt text on the pasteboard and Edit fills the composer.
Status: done 2026-09-28
Result:
- `DecorFragment` draws the prompt bubble as `UserBubble` looks, from existing tokens: the readable `surface.window` face with `fill.primary`, the specular sweep (stops shared from `Specular.swift` through `Appearance.sweep`/`sweepStops`) and e2 elevation (`ElevationToken.layers(at:)`, shared with the SwiftUI `Elevation` modifier), over the whole bubble across its paragraph slices; slice clips snap to device pixels so no seam shows. A `space.m` gap (`Edge.tiles`) between the prompt text and its tile row.
- Hover shows the new CoxUI molecule `PromptActions` in the bubble's top trailing corner: Edit and resend (`pencil`) calls `ComposerStore.edit` through `TranscriptView.composer(_:)`, Copy (`doc.on.doc`) puts the prompt on the pasteboard; without a composer only Copy shows. The style input is one `TextStyling` (drawn appearance, text size, line height) after merging T37.23.13.
Deviations: ~17 source and test files (~294 added, 109 removed) plus 16 PNGs; a TextKit fragment cannot host a live glassEffect, so the bubble draws the readable face and the sweep.
Check:
- CoxTranscriptText 34 (the 10 000-block launch budget misses under load on `p37-desktop` too: 410–503 ms at load ~35), CoxTranscript 43, CoxUI 154, CoxModel 47; swiftlint clean.
- After merging T37.24.7, T37.23.9, T37.25.1 and T37.23.16 into `p37-desktop` (fixtures re-recorded): `just test --changed-since` 1511 passed, 5 skipped; clippy on the changed crates and fmt clean; CoxCore 12, CoxModel 48, CoxTranscriptText 34, CoxTranscript 45, CoxUI 158.
Not done: Edit and resend's conversation rewind (A102) is T37.23.18.

#### T37.25.1 Core emits the context window and its split

Depends: — · Size: ~150 · Files: `crates/cox-protocol/…` (event), `docs/protocol.jsonschema`, `crates/cox-core/…` (context, session), `crates/cox-app/…` (Meter fold, `MeterText`)
Goal (A98): after it assembles each request the core emits `Event::ContextBreakdown` with the model's context window from the catalog and the system, tools, instructions and history parts from `cox_core::context::breakdown` (today dead code), scaled to the last usage as that function already does. The rollout records it like any event; cox-app's Meter fold keeps the latest and `MeterText` formats the share of the window and each part.
Check: the protocol-schema drift test; a cox-core test over the Scripted provider that every request emits the event with a non-empty split and the catalog window; a cox-app test that `MeterText` formats it.
Status: done 2026-09-28
Result:
- `cox-protocol` `ContextBreakdown` (`window` optional, `total`, `system`, `tools`, `instructions`, `history`, `cached`) and `Event::ContextBreakdown { turn, breakdown }` (A98); `docs/protocol.jsonschema` regenerated.
- `cox-core` `Session::context_breakdown` emits it once per request, after the budget check and before the provider stream, so before that request's `Usage`: the window from the model catalog (else the provider's `max_context`), the split from `context::breakdown` (no longer dead code; `Breakdown::parts` folds nine segments into four), `cached` from the last usage. The request bytes are only read.
- cox-app's Meter keeps the latest; `MeterText` gains `context_share` ("7.6% of 1M") and `context_parts`, rescaled to the last call's reported context so the legend adds up to "Context · …". Headless stream-json prints the event on its own line.
Deviations: cox-ffi's `MeterText` mirror gains the two fields and a `ContextPart` record (it must list every field); cox-tui ignores the event until T37.25.3; the core's total uses compaction's bytes/4 estimate, hence the rescale; ~12 source files, ~220 lines.
Check:
- `protocol_jsonschema_matches_committed_file`; `turn_every_request_emits_its_context_breakdown`; `the_context_split_is_scaled_to_the_last_call_and_shared_of_the_window`; cox-core, cox-protocol, cox-app, cox-ffi, cox-tui and cox-acp 726; cox-session and cox-store 71; cox `run_cli` 19, `plain`/`ide` 4; clippy and fmt clean. Real binary: stream-json prints `context_breakdown` before each `usage` (window 1000000). Core scenario snapshots gain one block per request (token counts zeroed in the helper).
- After merging T37.24.7, T37.23.9, T37.25.1 and T37.23.16 into `p37-desktop` (fixtures re-recorded): `just test --changed-since` 1511 passed, 5 skipped; clippy on the changed crates and fmt clean; CoxCore 12, CoxModel 48, CoxTranscriptText 34, CoxTranscript 45, CoxUI 158.
Not done: the desktop popover (T37.25.2) and the TUI (T37.25.3); a subagent's event is not forwarded to the parent's stream.

#### T37.23.16 Theme colours for syntax runs in edit cards

Depends: — · Size: ~150 · Files: `crates/cox-app/…` (the `CodeRun` it sends), `crates/cox-ffi/src/types.rs` (mirror only), `desktop/macos/Packages/CoxTranscript/…`
Goal (A95): a diff's `CodeRun` carries the session theme's colour for its span in the theme's light and dark variants, taken from `cox-render`'s highlighter; the edit card draws the one matching the window's effective macOS appearance and redraws when the appearance changes, without a new fold. Runs without a colour stay `.plain`.
Check: a cox-app test that a Rust edit's keyword run carries both colours; CoxTranscript light/dark snapshots of that edit card; the three Swift fixtures re-recorded.
Status: done 2026-09-28
Result:
- `cox-render` `markdown::theme_variants(chosen)` pairs a theme with its dark/light sibling (`….dark`/`….light`, `… (dark)`/`… (light)`), serves an unpaired known theme to both, and falls back to base16-ocean for an unknown one; `diffmodel::model` highlights with both, `StyledSpan.light` beside `rgb` (dark) (A95). cox-ffi's `Span` conversion carries `light`.
- CoxClient `Span.light`, CoxCore `Convert.swift`; CoxUI `CodeRun.theme` becomes one dynamic `NSColor` that AppKit resolves against the view's effective appearance, so an appearance change redraws the card without a new fold; runs without `rgb` stay `.plain`. DESIGN.md `DiffLineView` row.
Deviations: 13 code and doc files (+216/−27, ~100 tests) plus fixtures and PNGs; the cox-render markdown snapshot gains `light: None`; one `swiftlint:disable:next no_literal_colour` citing A95; an extra test that a window turning dark redraws the card.
Check:
- `cargo nextest run -p cox-render -p cox-app -p cox-ffi -p cox-tui` 382/382 (`a_rust_edits_keyword_run_carries_the_dark_and_the_light_colour`, `a_theme_pairs_with_its_sibling_and_an_unpaired_one_serves_both`, `every_highlighted_run_carries_the_light_variant_too`); clippy (also `--no-default-features`) and fmt clean. CoxModel 44, CoxUI 149, CoxTranscript 40, CoxCore 12.
- After merging T37.24.7, T37.23.9, T37.25.1 and T37.23.16 into `p37-desktop` (fixtures re-recorded): `just test --changed-since` 1511 passed, 5 skipped; clippy on the changed crates and fmt clean; CoxCore 12, CoxModel 48, CoxTranscriptText 34, CoxTranscript 45, CoxUI 158.
Not done: code blocks in replies keep one (dark) colour — A95 covers edit cards.

#### T37.28.2 Review pane: files by turn and their diff

Depends: T37.28.1 · Size: ~180 · Files: `…/Organisms/ReviewPane.swift`, `crates/cox-app/src/review.rs` (new), CoxModel mapping
Goal (A101): DT§5.4's split view — on the left the files grouped by turn with +/− counts and the RewindTimeline ("Rewind to here"), on the right the selected file's unified `DiffModel` through `DiffHunkView`, with the ⌘⌥D side-by-side toggle. After a code-only rewind the diff is the net difference between the checkpoint copy and the file on disk, not the model's calls one by one. The Changes tab's plain Rewind (`.rewind(checkpoint:)`) restores code only (DT§5.2 "Restore code to here"); the three scopes stay in the timeline.
Check: a cox-app test gives the per-file diff after two edits; a snapshot per cell.
Status: done 2026-09-28
Result:
- `crates/cox-app/src/review.rs` and `LiveSession::review(path)`: the net diff A101 asks for — the first checkpoint copy of the path from the store's archive (empty if the session created it) against the file on disk, read through `GitCheckpointer::preimages` under `path::confine` against the session's workspace roots (now kept on `LiveSession`). `cox_render::diffmodel::between(path, old, new, theme)` feeds similar's unified text to the existing `model`, so word ranges and highlighting match the edit cards; no hunks when nothing differs, `None` for an unchanged, outside or over-cap path.
- cox-ffi `SessionHandle::review` (one-expression forward); `SessionClient.review(_:)` (the fixture takes `reviews:`), CoxCore `LiveSession.review`. CoxModel `ReviewState` groups `changes::build`'s files by turn with the checkpoints; `SessionStore.review(path:)`; `SessionStore.rewind(checkpoint:)` is the Changes tab's plain Rewind, code only.
- CoxUI `Organisms/ReviewPane.swift`: files by turn with +/− counts, the RewindTimeline under them, the open file's diff as `DiffHunkView`s; `Previews/PreviewState+Review.swift`, 6 snapshots, DESIGN.md DS§6.4 row.
Deviations: ~270 non-test lines in 13 files; the ⌘⌥D side-by-side toggle left out.
Check:
- `cargo nextest run -p cox-ffi -p cox-app`: 80/80 including `review_diffs_each_file_against_its_checkpoint_and_after_a_code_rewind_nets_to_nothing`; clippy and fmt clean. CoxModel 50, CoxUI ReviewPane/ChangesTab/RewindTimeline 12, CoxCore 12.
- After merging into `p37-desktop`: `just test --changed-since` 534 passed, 1 skipped; clippy and fmt clean; CoxCore 12, CoxModel 51, CoxUI ReviewPane/ChangesTab/RewindTimeline 12.
Not done: app wiring (⌘⇧R, `ReviewState` → `ReviewPane.State`, T37.32); the side-by-side toggle; the file list's +/− counts are still the model's calls, only the diff pane shows the net change.

#### T37.23.18 Edit and resend rewinds the conversation

Depends: — · Size: ~60 · Files: `desktop/macos/Packages/CoxModel/…` (`ComposerStore`, `SessionStore`), `desktop/macos/Packages/CoxTranscript/…`
Goal (A102): a prompt's Edit and resend (T37.23.9) fills the composer with the prompt and sends `Intent.rewind(toTurn:code: false, conversation: true)` to the turn before that prompt, so the resent prompt does not see the old reply and no file changes; restoring code stays an explicit choice in the rewind timeline.
Check: a CoxModel test over the fixture that Edit on the second prompt fills the composer and sends one conversation-only rewind to the turn before it.
Status: done 2026-09-28
Result:
- CoxModel `ComposerStore.resend(_ prompt: Block)` in `Rewind.swift` (A102): fills the draft through `edit(text)`, then sends `.rewind(toTurn: prompt.turn, code: false, conversation: true)` — the core's `to_turn` is the first turn cut (`cut_history` drops from the first turn mark with `seq >= to_turn`; the timeline removes blocks with `turn >= to_turn`), so the prompt's own turn removes it and its reply; a send error goes to `report`; a non-prompt block sends nothing. CoxTranscript `PromptActing.swift`: `.edit` calls `composer?.resend(block)`. No Rust change.
- New data-only scenario `crates/cox-ffi/fixtures/two-prompts.toml` and fixture `desktop/macos/Fixtures/two-prompts.json`, with its record command in the Fixtures README.
Deviations: the new fixture (no recorded fixture had a second prompt); it joins the fixture loops in CoxModel and CoxTranscriptText.
Check:
- CoxModel 53 (the Check test replays `two-prompts.json`, takes the second user block from the replay, and finds the composer filled and exactly one conversation-only rewind to its turn), CoxTranscript 45, CoxTranscriptText 33 of 34 (the load-sensitive launch budget); swiftlint clean.
- After merging into `p37-desktop`: CoxModel 53, CoxTranscript 45.
Not done: app wiring (T37.32). With a turn running the core refuses the rewind ("interrupt it first") and the draft is still filled; the client does not interrupt.

#### T37.25.3 Context split in the TUI

Depends: T37.25.1 · Size: ~120 · Files: `crates/cox-tui/src/…` (state, status, a `/context` overlay)
Goal (A98): the TUI shows what the desktop popover shows: its status-line context share takes the window from `Event::ContextBreakdown` instead of a fixed default, and a `/context` overlay lists the window, the share and the system, tools, instructions and history parts with a bar in the same colour roles.
Check: `insta` snapshots of the status line and the `/context` overlay in dark, light and no-colour; a state test that the event updates the window.
Status: done 2026-09-28
Result:
- The TUI `/context` overlay and the status line's context share read `Event::ContextBreakdown` (A98): the overlay lists each part with its tokens and share of the window, and the status share comes from the event's window rather than a local estimate (commit fb65f7fc).
- `cox-core/src/context.rs` drops the stale `/context` dead-code note and the unused `Breakdown::to_json`.
Deviations: none.
Check:
- `just test --changed-since` over the merged batch: 900 passed, 2 skipped; clippy on cox-core and cox-tui clean.
Not done: `cox --plain` still takes the window from its own estimate, not from `ContextBreakdown`.

#### T37.25.2 Context split in the desktop token popover

Depends: T37.25.1 · Size: ~100 · Files: `crates/cox-ffi/src/types.rs` (mirror only), `desktop/macos/Packages/CoxModel/…`, `desktop/macos/Packages/CoxUI/…` (token popover), `desktop/design/DESIGN.md`
Goal (A98): the live token popover shows the context share of the window and the StackedBar with its legend (`context.system/tools/instructions/history`) that the preview already draws, fed from T37.25.1's Meter; DESIGN.md's context-bar note stops claiming the TUI already showed the split.
Check: a CoxModel test that the fixture's breakdown reaches the popover state; a CoxUI snapshot of the live-fed popover; fixtures re-recorded.
Status: done 2026-09-28
Result:
- The desktop token popover shows the context split as a `StackedBar` with the window share (A98, commit 8e162483). The one mapping from `ContextPart` lives in CoxTranscript `TokenPopover.Part.init?(ContextPart)`.
- Meter types moved to `CoxClient/Meter.swift` and their conversion to `CoxCore/Convert+Meter.swift`.
- The pinned approval bar's snapshots were re-recorded, because the meter now shows the window share from the recorded fixture (commit 48762171).
Deviations: the meter types moved into their own files, because SwiftLint's 400-line limit was hit.
Check:
- After merging into `p37-desktop`: CoxCore 12 (dev XCFramework), CoxModel 54, CoxTranscriptText 35, CoxTranscript 48 (with Benchmark skipped; PinnedDecision re-recorded, and a second run passed), CoxUI 161.
Not done: none.

#### T37.23.15 Per-level transcript heading sizes

Depends: — · Size: ~60 · Files: `desktop/design/tokens/base.json` (and the generated token outputs), `desktop/design/DESIGN.md`, `desktop/macos/Packages/CoxTranscriptText/…`
Goal (A94): tokens `font.transcript.h1` (17 pt semibold) and `font.transcript.h4` (13 pt semibold) beside `font.transcript.h3`, documented in DESIGN.md's type table; T37.23.12's heading paragraphs take their size from the heading level as DT§5.9 maps it instead of one `h3` size.
Check: the token build's own check; a CoxTranscriptText snapshot of every heading level in light and dark.
Status: done 2026-09-28
Result:
- CoxTranscriptText maps markdown headings to three sizes (A94): level 1 → `font.transcript.h1` (17 pt), level 2 → h3, levels 3–6 → `font.transcript.h4` (13 pt semibold). Commits 14d9d1d9 and 8b73b8b0; the second fixed an earlier mapping in which `####` came out larger than `###`.
Deviations: none.
Check:
- After merging into `p37-desktop`: CoxTranscriptText 35, CoxTranscript 48 (with Benchmark skipped), CoxUI 161.
Not done: none.

#### T37.23.17 A stronger quote bar from its own token

Depends: — · Size: ~40 · Files: `desktop/design/tokens/*.json` (and the generated outputs), `desktop/design/DESIGN.md`, `desktop/macos/Packages/CoxTranscriptText/…/TranscriptStructure.swift`
Goal (A97): a `quote.bar` token (width about 3 pt, a colour stronger than the hairline, with light, dark and high-contrast variants) in DESIGN.md's tables; T37.23.12's `QuoteFragment` draws its bars from it instead of the thought's hairline.
Check: the token build's own check; CoxTranscriptText light and dark snapshots of a nested quote.
Status: done 2026-09-28
Result:
- New token `quote.bar` (A97), using the text.tertiary value: light #a1a1a6, dark #6c6c72, high contrast #8b8b90 / #7a7a7f. New size `size.quoteBar` = 3 pt. Block quotes in the transcript draw their bar with both (commit eb8a38d1).
Deviations: none.
Check:
- After merging into `p37-desktop`: CoxTranscriptText 35, CoxTranscript 48, CoxUI 161.
Not done: none.

#### T37.29.3.1 Context tab: context split, cache hit, Compact now

Depends: T37.25.1 · Size: ~190 · Files: `crates/cox-app/src/meter_text.rs`, `desktop/macos/Packages/CoxUI/…/Organisms/ContextTab.swift`, `desktop/macos/Packages/CoxModel/…`
Goal: the window as a StackedBar by part with a Free row in the legend, the share of the window, the turn's cache hit and Compact now, all from the Meter's latest `ContextBreakdown`.
Check: a CoxModel test that the fixture reaches the tab and that Compact sends one `Intent.compact`; snapshots.
Status: done 2026-09-28
Result:
- cox-app `MeterText` gains `context_free` (the window minus the context) and `cache_hit` (`94% this turn`); the footnote reuses the same cache-hit figure; cox-ffi mirrors both (commit 50eb2c0c).
- CoxModel `ContextSplit` is the one mapping of the split, shared by the token popover and the tab; `ContextTabState`, `SessionStore.contextTab` and `compactNow()`, which sends `.compact(focus: nil)`. CoxUI `Organisms/ContextTab.swift` with preview states; a DS§6.4 row and the Usage line in `docs/design/desktop.md`.
- Merged with T37.25.2: one set of meter types, in `CoxClient/Meter.swift` and `CoxCore/Convert+Meter.swift`; the popover now takes its parts from `ContextSplit` (commit f566cbf3).
Deviations:
- About 11 files, the same record → convert → client → state → view chain the other tabs needed.
- The legend uses cox-app's labels without the mockup's counts, because the breakdown doesn't carry them.
- No cache gauge: no CoxUI component draws one yet.
Check:
- In the branch: `just test --changed-since p37-desktop` 479 passed; CoxModel 54, CoxUI ContextTab 3, CoxCore 12.
- After merging into `p37-desktop` with fixtures re-recorded: `just test --changed-since` 1514 passed, 5 skipped; CoxCore 12, CoxModel 58, CoxTranscriptText 35, CoxTranscript 48, CoxUI 164.
Not done:
- Nothing feeds the tab in the app yet; that waits for the app wiring (T37.22.3).
- The mockup's "Auto-compact at 85%" is not in DT.
- Open questions for the creator: is the cache hit per turn (as now) or per session? Should Compact now be disabled while a turn runs?

#### T37.24.10 Think toggle in the composer

Depends: — · Size: ~60 · Files: `desktop/macos/Packages/CoxUI/…` (Composer), `desktop/macos/Packages/CoxModel/…`
Goal (A103): the composer's think toggle from DS/DT (left out of T37.24.7) works for one turn, like `/think`: the next send goes out with `confirm_think` (the think tier), then the toggle turns itself off.
Check: a CoxModel test that the toggle sends what the chosen behaviour needs; a CoxUI snapshot of both states.
Status: done 2026-09-28
Result:
- `Intent::Send` and `Intent::Queue` in cox-app carry `confirm_think` (default false); a queued send keeps the flag it was sent with; cox-ffi mirrors the field (A90). CoxClient/CoxCore `Intent.send`/`.queue` carry `confirmThink` (commit 37a34371).
- CoxModel `ComposerStore.think` and `toggleThink()`: the next turn, sent or queued, carries the flag, then the toggle turns off; a shell line or `/` command does not use it up (A103).
- CoxUI `ComposerChip.Kind.think(Bool)` and a `ThinkChip` next to the model chip; `TokenMeter` is fixed-size in the chip row so the model chip's label truncates instead of the meter wrapping. DESIGN.md and desktop.md updated.
Deviations: about 19 files plus 18 snapshots.
Check:
- In the branch: `just test --changed-since p37-desktop` 86 passed; CoxModel 52 (`theThinkToggleConfirmsOneTurnThenTurnsItselfOff`), CoxCore 12, CoxTranscript 45, CoxUI Composer/Token 9.
- After merging into `p37-desktop`: CoxModel 58, CoxTranscript 48, CoxUI 164.
Not done: `confirm_think` only passes the think tier's confirmation gate; it does not move a code-tier turn onto think, and the TUI `/think` has the same gap. Filed as T37.24.11.

#### T37.28.5 A skipped restore says why

Depends: — · Size: ~80 · Files: `crates/cox-protocol/…` (`Event::Rewound`'s skipped entries), `docs/protocol.jsonschema`, `crates/cox-core/src/rewind.rs`
Goal (A101): each file a code rewind could not restore carries its reason — too large, outside the workspace roots, or the I/O error — and the notice counts them by reason (`2 too large to restore, 1 failed: <error>`) instead of calling every failure too large.
Check: the protocol-schema drift test; a cox-core test where one file is over the size cap and one is unreadable gives two reasons and the matching notice.
Status: done 2026-09-28
Result:
- `Event::Rewound.skipped` entries are `SkippedFile { path, reason }` with `SkipReason::TooLarge | OutsideRoots | Failed { error }` (A101, commit 12e4fe20); `docs/protocol.jsonschema` regenerated.
- `cox-core/src/rewind.rs` gives each skipped file its reason and the notice counts them by reason (`1 too large to restore, 1 failed: io error`).
Deviations: a rollout that stored `skipped` as bare paths still loads, each read as failed with "no reason recorded", so an old rollout still resumes.
Check:
- cox-protocol 103 (drift test and `a_skipped_path_without_a_reason_still_loads`); cox-core rewind 8 (`a_skipped_restore_carries_its_reason`); clippy and fmt clean.
- After merging into `p37-desktop`: `just test --changed-since` 1521 passed, 5 skipped.
Not done: none.

#### T37.28.3 Revert one file to before turn N

Depends: T37.28.1 · Size: ~120 · Files: `crates/cox-protocol/…` (a new `Submission`), `crates/cox-core/src/rewind.rs`, the cox-app intent
Goal (A101): DT§5.4's per-file revert and ChangesTab's existing `.revert(path:)`: restore one file to its checkpoint before turn N, checkpointing it first so the revert can itself be undone. A new `Submission` approved by A101.
Check: a cox-app test reverts one file and leaves the other.
Status: done 2026-09-28
Result:
- `Submission::RevertFile { path, to_turn }` handled by `revert_file` in `cox-core/src/rewind.rs` (A101, commit aeb69eab): the rewind's restore limited to one path, checkpointing the file's current bytes first under a turn of their own, so `/redo` or a rewind undoes it; history stays append-only. The path is confined by the checkpointer's `preimages`; a path outside the roots or unreadable is refused with a warning notice.
- Wired through `Intent::RevertFile` (cox-app), the cox-ffi type mirror, `Intent.revertFile` (CoxClient/CoxCore) and CoxModel `SessionStore.revert(path:)`, which sends `to_turn` 1.
Deviations: more than 3 files, because the intent runs through cox-ffi and the Swift packages; no TUI slash command (the card asks for none).
Check:
- `reverting_one_file_restores_it_and_leaves_the_other`, `revert_file_restores_only_that_file`, `every_intent_maps_to_its_submission`; clippy and fmt clean; a real-binary scripted write under a scratch `COX_HOME`.
- After merging into `p37-desktop` (merge 8944da74): `just test --changed-since` 1521 passed, 5 skipped; clippy on cox-core, cox-app, cox-ffi and cox-protocol clean; CoxCore 12, CoxModel 59.
Not done: the headless surface has no way to send a revert, so the real-binary run covered the checkpoint, not the revert.

#### T37.24.11 `/think` and the think toggle run their turn on the think tier

Depends: T37.24.10 · Size: ~60 · Files: `crates/cox-core/src/router.rs`, `crates/cox-core/src/session.rs`
Goal: D5 and A103 — `UserTurn { confirm_think: true }` routes that one turn's main request to `Tier::Think`, then the session goes back to its own tier. Today `Router::pick` takes the main tier from the session override or the session tier and uses `confirm_think` only to pass the confirmation gate, so `/think` and the desktop toggle on a code-tier session still run on code; only `--deep` reaches think, through a session-wide `SwitchModel`. Architect mode (which already sets `confirm_think` while on the think tier) must keep working.
Check: a cox-core test that a code-tier session's `confirm_think` turn requests the think model and the next plain turn requests the code model again.
Status: done 2026-09-28
Result:
- `crates/cox-core/src/session.rs`: a turn with `confirm_think` on a session whose main tier is not think puts `Tier::Think` into `Inner::routed`, the per-turn slot `route` advice (T33.20) uses. Every request of that turn, tool-call follow-ups included, runs on think. `run_turn` clears the slot, so the next plain turn is back on the session tier. No `ModelSwitched` event. Ledger rows keep `job = main` with the think tier and model (commit ec852097).
- A session already on think (`--deep`, architect mode) gets no slot; its requests and cache prefix are unchanged.
- The `Router::pick`, `Inner::routed` and `confirm_think` docs are updated, and `docs/protocol.jsonschema` is regenerated.
Deviations: the fix is in `session.rs`, not `router.rs`. `step` and `switch_model` call `Router::pick` with `confirm_think = true` on every main request, so routing there would move every such request.
Check:
- `crates/cox-core/tests/router.rs` `confirm_think_runs_one_turn_on_think_then_the_session_tier_again`: it fails without the change. cox-core router 8/8.
- Protocol schema drift test passed.
- clippy and fmt are clean.
- Real binary, scripted provider: a plain run started on code, and a `--deep` run on think.
- After merging into `p37-desktop`: `just test --changed-since` 1522 passed, 5 skipped.
Not done:
- No real-binary run of a `confirm_think` turn on a code-tier session: only the TUI can send one.
- A think turn's thinking blocks stay in history for the next code-tier turn, the same as a turn `route` advice sent to cheap (T33.40.8).

#### T37.28.4 Line comments sent to the agent

Depends: T37.28.2 · Size: ~150 · Files: `…/Organisms/ReviewPane.swift`, `crates/cox-app/src/review.rs`
Goal: clicking a line number adds a comment to a draft; "Send to agent" posts one `Intent::Send` with `file:line` anchors, the message formatted in cox-app.
Check: a cox-app test of the message; a snapshot of a draft.
Status: done 2026-09-28
Result:
- cox-app `review.rs`: `LineComment { path, line, removed, text }` and `message(&[LineComment]) -> Option<String>`, the prompt with one `` `path:line` `` bullet per comment in draft order. A removed line is marked, and blank comments are skipped. cox-ffi has the record and a one-expression `review_message` forwarder (commit 8b532490).
- CoxClient `LineComment` and `SessionClient.reviewMessage`. CoxModel `ReviewDraft` (`pick`, `save`, `remove`) lives in `SessionStore.reviewDraft`, so it survives switching files. `sendReview()` posts one `Intent.send` and empties the draft.
- CoxUI `ReviewPane` has a draft panel under the diff with the anchors, a comment field, a count and "Send to agent". `DiffHunkView`/`DiffLineView` take a tap on the line number, with an accessibility action.
Deviations: 15 files. The message crosses from cox-app to Swift through the FFI record, the client protocol and its two conformers, the model and two molecules.
Check:
- In the branch: cox-app and cox-ffi 89 passed (`review::tests`); clippy and fmt clean; CoxModel 62, CoxCore 12, CoxUI 165, CoxTranscript 48; swiftlint and swift-format clean.
- After merging into `p37-desktop`: `just test --changed-since` 89 passed; CoxCore 12, CoxModel 62, CoxTranscript 48, CoxUI Review/Diff 5.
Not done:
- App wiring of the draft into `ReviewPane.State` waits for T37.32.
- Open question: "Send to agent" always sends; the composer queues a prompt while a turn runs. Should review comments queue too?

#### T37.17.2 `letterSpacing` in em, mockups on `tokens.css`

Depends: — · Size: ~40 · Files: `desktop/design/tokens/*.json`, `desktop/design/style-dictionary.config.*`, `desktop/design/mockups.html`
Goal: `letterSpacing` tokens say em, which is what they mean; `mockups.html` reads the generated `tokens.css` instead of its inline variables, as its README promises.
Check: generated `Tokens.swift` values are unchanged or the changed snapshots are re-recorded on purpose; the mockups render the same by eye.
Status: done 2026-09-28
Result:
- All 15 typography `letterSpacing` values in `desktop/design/tokens/base.json` say `em` (commit 01b7fc3f). `style-dictionary.config.mjs` has an `em()` helper for the Swift tracking value that fails the build on any other unit; a missing value still gives 0.
- `desktop/design/mockups/mockups.html` links `../tokens/tokens.css`, and its short colour names point at the `--c-*` tokens on both `:root` and `.dark`. Values with no token stay inline: wallpaper, window shadow, sidebar border, `--purple` and the glass materials. The mockups README says so.
Deviations:
- The mockups were compared with the project's `render.sh` and a byte comparison of the PNGs, not by eye.
- In dark mode `--blue` now follows the dark `status.plan` token; no screen shows it in dark mode.
Check:
- `just desktop-tokens`: both high-contrast checks pass (189 pairs each); `Tokens.swift`, `Colors.xcassets` and `tokens.css` are byte-identical to before.
- Negative check: a `rem` value fails the build naming the token.
- All 30 mockup screens render byte-identical before and after.
- No Swift tests: no generated Swift changed.
Not done: none.

#### T37.29.3.2 Context tab: per-turn cost history

Depends: T37.29.3.1 · Size: ~180 · Files: `crates/cox-app/…`, `crates/cox-ffi/src/types.rs`, `desktop/macos/Packages/CoxUI/…/Organisms/ContextTab.swift`
Goal: cox-app `LiveSession::turn_costs()` over the ledger's `usage` rows by turn (input, output, cache read and write, `$`, subagent rows indented) plus the session total, forwarded through cox-ffi into a KeyValueGrid "Cost by turn".
Check: a cox-app test over a scripted two-turn session; snapshots.
Status: done 2026-09-28
Result:
- cox-store `usage_ledger` (Diesel DSL; the rows in write order with `created_at`). `crates/cox-app/src/costs.rs` builds `TurnCosts` from the session's ledger rows and its child sessions' rows. A turn starts where the ledger's `turn` resets to 1; side calls (turn 0) join the turn they ran in; a subagent (job Explore/Shell/Agent) is a detail row under the last turn that started before it; forks and handoffs are left out; the total row is "Session" (commit 93ab45e8).
- `LiveSession::turn_costs` → cox-ffi `SessionHandle.turn_costs` → CoxModel `SessionStore.costHistory()`; CoxUI ContextTab shows a "Cost by turn" KeyValueGrid.
Deviations: about 17 files. A subagent row is labelled by its job alone, because job and tier wrapped at the inspector's width.
Check:
- In the branch: cox-store and cox-app 112, cox-ffi and cox 13; clippy and fmt clean; CoxModel 61, CoxCore 13, CoxUI 165 (4 new snapshots).
- After merging into `p37-desktop`: `just test --changed-since` 1546 passed, 6 skipped; clippy on cox-store, cox-app, cox-ffi, cox-config and cox-protocol clean; CoxCore 13, CoxModel 66, CoxTranscriptText 35, CoxTranscript 48, CoxUI Context/Token/Settings 20.
Not done: nothing calls `costHistory()` until the app target (T37.32.1, T37.22.3). Finding, not fixed: the ledger's `usage.turn` is the call number within a turn, so `cox stats --session` shows call indices as turns (ideas.md).

#### T37.29.3.3 Context tab: project totals

Depends: T37.29.3.2 · Size: ~120 · Files: `crates/cox-store/…`, `crates/cox-app/…`, `desktop/macos/Packages/CoxUI/…/Organisms/ContextTab.swift`
Goal: a cox-store ledger query for today's and this week's spend per project, shown as the tab's footnote.
Check: a cox-store or cox-app test; a snapshot.
Status: done 2026-09-28
Result:
- cox-store `Store::project_spend(root, since)`: usage joined with sessions and grouped by cwd, in Diesel DSL. `costs.rs` `periods(now)` gives the start of the local day and of the ISO week (Monday) in UTC; `footnote()` gives mockup 10's line, "Project X today: $… · this week: $…". The project root is the git root, else the canonical cwd. The footnote shows before the session has spent anything (commit fc024c1e).
Deviations:
- The project is matched by the session's cwd being under the root, not by `project_slug`, which the core writes empty.
- New dependency chrono 0.4.45 (no default features; `clock`, `std`) for local midnight and the week start. It was already in the lock and is listed in `rust.md`; rows in `toolchain.md` and §1.1.
Check:
- In the branch: cox-store, cox-app and cox-ffi 122; clippy and fmt clean; CoxCore 13, CoxModel 61, CoxUI 165.
- After merging into `p37-desktop`: the same runs as T37.29.3.2.
Not done: app wiring (T37.32.1, T37.22.3). Finding, not fixed: `project_totals(slug)` behind `/sessions` returns zeros because `project_slug` is always empty (ideas.md).

#### T37.29.3.5 Context tab: cache hit per turn or per session, Compact now waits for the turn

Depends: T37.29.3.2 · Size: ~120 · Files: `crates/cox-app/src/meter_text.rs`, `crates/cox-config/…`, `desktop/macos/Packages/CoxUI/…/Organisms/ContextTab.swift`
Goal: A104 — cox-app formats the cache hit for the turn and for the session (from the ledger's `usage` rows); a config key owned by `cox-config` (schema, drift test, so the generated Settings window shows it) picks which one the tab shows, per turn by default. A105 — Compact now is disabled while a turn runs, read from the session status the tab already has.
Check: a cox-app test of both figures over a scripted two-turn session; the config drift test; CoxUI snapshots of the session figure and of a disabled Compact now.
Status: done 2026-09-28
Result:
- A104: `MeterText.cache_hit_session` ("88% this session"), from the session tally, which sums one `Event::Usage` per ledger row. The new config key `[desktop.context] cache_hit = "turn" | "session"` (default turn) has the `CacheHitScope` enum in `cox-protocol` `config.rs` and a `default.toml` line; `docs/config.jsonschema` and `docs/config.md` are regenerated. It is not on the project-config guard list, because it is a display setting. `SettingsStore.cacheHitScope` and `SessionStore.contextTab(cacheHit:)` pick the figure (commit d5080dfb).
- A105: `ContextTabState.turnRunning` is true while the meter's current turn is not done, and ContextTab disables Compact now while it is.
Deviations: the cox-ffi record, the Swift `MeterText` and its conversion, and the four re-recorded fixtures. One snapshot set covers the session figure and the disabled button.
Check:
- cox-protocol and cox-config 124 (both drift tests and a `desktop.context.cache_hit` round trip); cox-app and cox-ffi 97 (`the_cache_hit_is_formatted_for_the_last_turn_and_for_the_session`); clippy and fmt clean.
- CoxModel 66, CoxCore 13, CoxUI 167 (4 new `contextTabWhileATurnRuns` snapshots).
- After merging into `p37-desktop`: the same runs as T37.29.3.2.
Not done: the app target passes `SettingsStore.cacheHitScope` into the tab (T37.22.3).

#### T37.28.6 Review comments queue while a turn runs

Depends: T37.28.4 · Size: ~80 · Files: `desktop/macos/Packages/CoxModel/…/ReviewDraft.swift`, `crates/cox-config/…`
Goal: A108 — "Send to agent" in the Review pane queues its message while a turn runs, the same way the composer queues a prompt (`Intent::Queue`), or sends it at once, as a config key owned by `cox-config` picks (queue by default; schema and drift test, so the generated Settings window shows it).
Check: CoxModel tests that a running turn queues the review message by default and sends it with the other setting; the config drift test.
Status: done 2026-09-28
**Result:** `[desktop.review] send = "queue" | "now"` (default `queue`, A108): `ReviewSend` and `DesktopReviewConfig` in `cox-protocol` config.rs, a `default.toml` line, regenerated `docs/config.jsonschema` and `docs/config.md`. CoxModel: `ReviewSend`, `SettingsStore.reviewSend`, `SessionStore.sendReview(_:)` posts `Intent.queue` while a turn runs and the setting is `queue`, else `Intent.send`; `SessionStore.isTurnRunning` is the one "turn running" check, which `ComposerStore.isRunning` now reads. Wiring the setting into the Review pane's button waits for the app target (T37.32.1/T37.22.3).

**Check:** config drift tests 2/2; `just test --changed-since p37-desktop` 1535 passed, 5 skipped (new `config_set_desktop_review_send_round_trips`); clippy and fmt clean; CoxModel 68 passed (`whileATurnRunsSendQueuesByDefaultAndSendsAtOnceWithNow`, `theReviewSendSettingReadsBackFromTheSettings`); swift-format and swiftlint strict clean. Three CoxTranscript tests failed under load (~36) with and without the change and pass on a quiet machine.

#### T37.32.1 App target and an unsigned dev build

Depends: T37.15 · Size: ~150 · Files: `desktop/macos/Cox.xcodeproj/…`, `desktop/macos/App/…`, `justfile`
Goal: A106 — the thin app target of DT§7 (`@main`, scenes, menus, entitlements, Info.plist, assets) over the local Swift packages, with the XCFramework from T37.15; `just desktop-app` builds an unsigned (ad-hoc signed) Debug `Cox.app` that launches and shows `MainScreen` on the fixture or live core. The `.xcodeproj` stays small and merge-friendly; if a generator (e.g. XcodeGen) is the best maintained fit, use it and add its row to `toolchain.md`. No Sparkle, no signing identity, no secrets.
Check: `just desktop-app` builds on a clean checkout; the app launches and a screenshot shows the main window; the macOS CI job builds the target.
Status: done 2026-09-28
**Result:** the app target in `desktop/macos/App/`: `CoxApp.swift` (`@main`, one `WindowGroup`), `LaunchCore.swift` (`-CoxFixture <path>` → `FixtureCoreClient`, else `LiveCoreClient` over `$COX_HOME` with `HostBridge(MacHost)`; `COX_KEYRING=off` → `MemorySecretStore`; `-CoxProject` sets the working directory), `SessionWindow.swift` (`MainScreen` with `TranscriptView` and `SessionComposer`), an empty `Cox.entitlements` (no App Sandbox, DT-6), `Info.plist`, `Assets.xcassets`. `desktop/macos/project.yml` is the XcodeGen spec; `Cox.xcodeproj` is generated and gitignored. `scripts/desktop/app.sh` (xcodegen → xcodebuild Debug, ad-hoc `CODE_SIGN_IDENTITY=-`, `-scmProvider system`) copies `desktop/macos/build/Cox.app`; `just desktop-app` builds the XCFramework first. CI `desktop-macos` builds the app with `CODE_SIGNING_ALLOWED=NO`; swiftlint and swift-format cover `App/`. New tool: XcodeGen 2.46.0 (mise `aqua:yonaskolb/XcodeGen`, toolchain.md row) — only the spec is in git, so there is no `.pbxproj` to conflict.

**Deviations:** the app uses `@testable import CoxUI` because `MainScreen` and its intents are still internal; T37.22.3 makes them public. Bundle id `io.github.listepo.cox` is derived from the repo; T37.32.2 confirms it before the first signed build. ~365 lines over 15 files (~120 are plist and JSON).

**Check:** `CARGO_BUILD_JOBS=4 just desktop-app` from a removed `build/` and `Cox.xcodeproj`; `codesign -dv` → `Signature=adhoc`, no TeamIdentifier; the CI variant builds; launched with a scratch `COX_HOME` and `COX_KEYRING=off`: the `approve-write.json` fixture renders transcript and composer, the live core without a key shows the provider-auth error, `COX_PROVIDER=scripted` opens an empty session; swiftlint and swift-format strict clean on `App/`.

#### T37.19.5 Foundation tokens: on-accent text, dark highlight, disabled controls

Depends: — · Size: ~120 · Files: `desktop/design/tokens/*.json`, `…/Foundations/…`
Goal: an on-accent text token replaces the `Color.white` constant on the primary button; a dark e1 highlight token so dark controls lose the bright rim; `surface.capsuleBorder` is either used by capsules or removed; disabled toggle and slider visuals; check the faint vertical bars at a stroked pill's ends on screen and fix them if they are real.
Check: snapshots re-recorded on purpose for the changed foundations, and every other suite passes unchanged; SwiftLint's no-literal rules stay clean.
Status: done 2026-09-28
**Result:** `text.onAccent` replaces the `Color.white` constants on the primary button, the filled (Bypass) segment and CountBadge; the HC rule asks 7:1 on accent, danger and warning (dark-hc #3e3e3e, light-hc #ffffff). The dark highlight (A109): `[desktop.appearance] dark_highlight = "none" | "subtle"` (default none) and `dark_highlight_scope = "controls" | "all"` (default controls) in cox-protocol with schema, default.toml, docs and a cox-config round-trip test; tokens `material.darkHighlight.{none,subtle}` = 0 and 0.1; CoxUI `Appearance.darkHighlight`, `.highlightScope`, `highlightStrength(level)`, applied by `Elevation` to the inset layers only, passed through the TranscriptView bubble and the MaterialPicker swatches; `SettingsStore.darkHighlight` and `.darkHighlightScope`. Capsules and CoxSegmented draw `surface.capsuleBorder` with `.hairline(in:color:)`. A disabled toggle shows a `fill.secondary` track and a `text.secondary` label, a disabled slider has no fill, and the knob loses its lift. DESIGN.md §3.4, §6.1 and §6.2 updated. Wiring `SettingsStore` into `coxAppearance` is T37.22.3's.

**Deviations:** more than 3 files (tokens, generated output, config, snapshots). The faint pill-end bars show only when a `.continuous` stroke is captured through `NSView.cacheDisplay`, not through `ImageRenderer`, `.drawingGroup()` or `.circular`; left as is, not checked on a physical screen.

**Check:** `just test --changed-since p37-desktop` 1535 passed, 5 skipped; clippy and fmt clean; schema drift passes. CoxModel 67; CoxUI 131 dark snapshots re-recorded on purpose (4 subtle-highlight sets and the dark e1 renders), second run 173 passed; CoxTranscript 2 dark approval snapshots re-recorded, 48 passed; no light snapshot changed. After the merge with T37.28.6: config crates 126 passed with both round-trip tests, CoxModel 69, CoxTranscript 48.

#### T37.20.5 Atom tokens: project purple, RiskChip colours, named constants

Depends: T37.19.5 · Size: ~100 · Files: `desktop/design/tokens/*.json`, `…/Atoms/…`
Goal: a purple `role.project` token for the project badge (mockup value), RiskChip low/medium/high colours, and tokens for the values the atoms keep as named constants (badge and inline-code radius 5, CountBadge height 16, StatusDot halo 3 and idle ring 1.5, project tint 0.13).
Check: the affected atom snapshots are re-recorded on purpose; no named constant remains for a value that now has a token.
Status: done 2026-09-28
**Result:** `role.project` and `role.projectSoft` (light: the mockup's `#8e44d8` at its 0.13 tint), `risk.low/medium/high` each with a `Soft` face, in `desktop/design/tokens/color.{light,dark}.json`; `radius.badge` 5, `size.countBadge` 16, `size.statusDotHalo` 3, `size.statusDotRing` 1.5 in `base.json`; `high-contrast.mjs` holds each new label to 7:1 on its tint and the pages. Generated through `just desktop-tokens` (Tokens.swift, 8 colorsets, tokens.css, the -hc JSON). Badge, RiskChip, CountBadge, InlineCode and StatusDot keep no named constant for these values; RiskChip goes through a Badge init with explicit colours. DESIGN.md §3.1, §3.3, §6.2 updated; the mockup reads the purple from the tokens.

**Deviations:** dark purple `#bc90e8` instead of the mockup value (3.1:1 on the dark window): the smallest lightening that holds 4.5:1 on every dark surface and its own tint. Light purple stays the mockup value (4.41:1 on its own tint). The mockup has only the medium risk colour, so the three risk roles keep the grey/orange/red DS§6.2 used; the chip looks the same. 7 hand-edited source files.

**Check:** `just desktop-tokens`; `node high-contrast.mjs --check` 216 pairs pass in light-hc and dark-hc. CoxUI: 15 snapshots re-recorded on purpose (the project badge, SettingRow read-only, three Settings screen tests), second run 173 passed. CoxTranscript 50 passed (two ComposerFlow/PinnedDecision tests fail only under full-suite load, pass alone 3/3). swiftlint and swift-format strict clean.

#### T37.22.3 App window setup and the public CoxUI surface

Depends: T37.32.1 (the app target, A106) · Size: ~100 · Files: `…/Screens/MainScreen.swift`, the app target
Goal: the app window has a hidden title bar and a behind-window blur; the screen, state and intent types the app target needs are `public`. The app wires the Appearance popover (T37.26) to `SettingsStore`. It draws blur and wallpaper tint through the behind-window view. It fills the value texts and closes the popover on click-outside or Esc. Controls locked by a higher config layer are disabled, with the layer named. The Settings screen (T37.30.1) opens from the app menu, with slider writes coalesced. The app passes `HostBridge(MacHost())` (T37.30.2) to `LiveCoreClient`, and a notification delegate handles clicks and foreground display. The app's View menu replaces the system sidebar and inspector command groups with items of the same titles and keys from `ShellShortcut` (T37.22.2), because the system commands act only on system-built panes and the shell is laid out by hand. The onboarding checklist (`App::checklist`, T37.31) is forwarded through `cox-ffi` and shown on first run.
Check: the app target builds against `CoxUI` with only public API; a screenshot of the running app matches mockup screen 28 by eye.
Status: done 2026-09-28
**Result:** CoxUI's screen, state and intent types the app needs are `public` (MainScreen, Sidebar, SessionToolbar, AppearancePopover, SettingsScreen, OnboardingScreen, ChecklistRow, SettingSource, SettingsPage, InspectorTab, ShellShortcut); the app has no `@testable` import. `App/WindowChrome.swift`: hidden title bar, a see-through NSWindow, a behind-window blur. `App/AppearanceState.swift`: the Appearance popover reads and writes `SettingsStore`, fills its value texts, coalesces slider writes (150 ms per key), closes on click-outside or Esc (`MainScreenIntent.dismissPopover`); a control locked by a higher config layer is disabled and names the layer. `App/SettingsWindow.swift`: Settings from the app menu, with `cacheHitScope` and `reviewSend` on its Appearance page. `NotificationResponder` handles a click and shows banners in the foreground. The View menu's sidebar and inspector items use `ShellShortcut` (⌃⌘S, ⌃⌘I). cox-ffi exports `App::checklist`; CoxClient `OnboardingClient`; `App/FirstRun.swift` shows it on first run.

**Deviations:** 30 files, +932/−139. Blur strength is the NSVisualEffectView alpha (AppKit has no radius); tint is `.saturation`; depth shows a percent where the mockup says "High"; in fixture mode Settings reads the live core at COX_HOME.

**Check:** `just desktop-app` builds with no warning in App/; CoxUI 174 (the lock snapshot re-recorded after the merge with T37.19.5's disabled styling), CoxModel 69, CoxCore 13, CoxTranscript 48; cox-ffi nextest 7/7, clippy and fmt clean; swiftlint and swift-format strict clean. The orchestrator ran the app on the approve-write fixture and compared it with mockup 28: the glass, margins, composer rate and model name differ (T37.22.4), and the toolbar, sidebar and inspector are still unwired (T37.22.5).

#### T37.22.5 App wiring: toolbar, sidebar and inspector from the live stores

Depends: T37.22.3 · Size: ~200 · Files: `desktop/macos/App/SessionWindow.swift`, `…/Screens/MainScreen.swift`, CoxModel stores
Goal: the parts of mockup 28 the app still shows empty (the creator's local look, 2026-09-28, and T37.22.3's Not done). Toolbar: the session title with project and worktree breadcrumb, the model pill with its popover, the cost and `ctx n%` meter (it shows an empty `· ctx`), Stop while a turn runs, the Appearance button; the `+` menu only if the mockup has it. Sidebar: the session list grouped as on the mockup (Needs you, Running, per project) with cost and status, and the providers footer. Inspector: the Changes, Plan, Context, Tasks and Info tabs read their stores; the Review pane sends through `SessionStore.sendReview(settings.reviewSend)` (A108). The app calls `loadLoginEnv` so the shell-environment row fills, and the stored-keys list refreshes after `storeKey`.
Check: a screenshot of the app on a fixture with a few sessions matches mockup 28's toolbar, sidebar and inspector by eye; a CoxModel test for each new store read.
Status: done 2026-09-28
**Result:** Toolbar: the title and project/worktree breadcrumb from the session's entry and Info, the model and mode pills, the `$x.xx · ctx n%` pill (opens the Context tab), Stop while a turn runs, Appearance. Sidebar: `SidebarStore` over a new `WorkspaceClient` (the existing FFI `App.projects`, `sessions`, `activity`) plus the inbox — Needs you (with a count), Running, one group per project; rows show status, age and cost; filter and fold work; a row switches to or resumes its session; the footer reads "N providers" with the provider-key check as its dot. Inspector: `SessionInspector` (CoxTranscript) fills Changes, Plan, Context, Tasks and Info from SessionStore; revert, rewind and compact are wired. Review: `SessionReview` replaces the transcript column and sends through `sendReview(settings.reviewSend)` (A108). `LiveCoreClient.loadLoginEnv()` is a one-line forward (A90) run before any session opens; `SettingsStore.storedKeys` is re-read after load, `storeKey` and `removeKey`; `hasKey` reads it.

**Deviations:** two commits, 19 files +835 and 15 files +574; public inits in CoxUI for the toolbar, sidebar, tabs and rows. Not done, carried to T37.22.6: the model popover (the core exports no model catalog), a shell task's output viewer, the session list polls every 2 s (no app patches yet), untitled sessions ("New session" in the toolbar, "Untitled session" in the sidebar; `cox run` sessions get no title), and the footer counts configured provider sections (9 on defaults).

**Check:** CoxModel 76, CoxCore 13, CoxUI 174 (snapshots unchanged), CoxTranscript 52 with 2 new (PinnedDecision and ComposerFlow fail only in parallel runs under load and pass alone); swiftlint and swift-format strict clean; the app builds; screenshots of a live core with 3 projects (Changes, Plan, Context, Info, Review) and the fixture checked against mockup 28 by the orchestrator.

#### T37.22.4 App glass, window chrome and composer stats match mockup 28

Depends: T37.22.3 · Size: ~150 · Files: `desktop/macos/App/WindowChrome.swift`, `…/Screens/MainScreen.swift`, the composer stats view
Goal: fixes the running app's differences from mockup 28 that the creator's local look (2026-09-28) found. (1) The window strip behind the toolbar is fully clear: whatever is behind the window reads sharp through it; the behind-window blur and wallpaper tint must cover the whole window, as on the mockup. (2) The panes render nearly opaque white; they must follow `window transparency` (58 % on the mockup) and frost so the wallpaper colour shows through, while text blocks stay readable (the popover note: at least 80 % opaque). (3) The sidebar pane encloses the traffic lights, as on the mockup, and every pane keeps the mockup's margin to the window edges; the composer does not touch the bottom edge. (4) The composer's rate reads `183763 tok/s` on the approve-write fixture: a rate over a near-zero duration must not show (the mockup shows `71 tok/s` with its sparkline). (5) The model pill reads `claude-…t-5 · high`; it shows the catalog display name (`Sonnet 5 · high`).
Check: a screenshot of the app on the approve-write fixture next to mockup 28 shows the glass, margins, rate and model name matching by eye; a unit test for the rate guard.
Status: done 2026-09-28
**Result:** `WindowChrome.swift` spreads the behind-window blur (`.fullScreenUI`) over the whole window, title-bar strip included; an empty `.unified` toolbar puts the traffic lights inside the sidebar pane. `ShellPane` uses `glassPane(frosts: false)` (`GlassPane.swift`): the panes tint the blur instead of stacking a second glass that read white. `AppearanceState.blurFraction` is a strength, so Frosted's default blurs fully. `SessionComposer` keeps the composer off the column edges and 16 pt off the bottom. `usage.rs` shows no rate measured over less than 100 ms (`MIN_SPAN`; `a_rate_over_a_near_zero_span_does_not_show`); all four fixtures re-recorded under the guard.

**Deviations:** 7 source files. The model pill shows the full `claude-sonnet-5 · high`: the catalog has no display name yet (a creator decision). The traffic lights sit ~6 pt above the sidebar toggle row's centre; tertiary text ("NEEDS YOU", the filter placeholder) reads faint over the glass (T37.21.11's contrast questions).

**Check:** `cargo nextest run -p cox-app -p cox-ffi` 98 passed, clippy and fmt clean; CoxUI 174 (17 frosted snapshots re-recorded on purpose), CoxModel 76, CoxPlatform 13, CoxTranscript 50 (2 PinnedDecision solid snapshots re-recorded); swiftlint and swift-format strict clean. After the merge and the fixture re-record: CoxTranscript 50, CoxModel 76, CoxCore 13. The app, over a colourful backdrop, compared with mockup 28 by the orchestrator: the whole window is glass, the wallpaper colour shows through the panes, the lights sit in the sidebar, no bogus rate.

#### T37.22.6 App wiring leftovers: model popover, session titles, provider count, live session list

Depends: T37.22.5 · Size: ~150 · Files: `crates/cox-ffi`, `crates/cox-app`, CoxClient, `…/Screens/…`
Goal: what T37.22.5 left. The toolbar's model pill opens its popover from a model catalog `cox-ffi` exports (a one-expression forward, A90). A session gets a title the same way the TUI titles one, also for `cox run`; the toolbar and the sidebar name an untitled session the same way. The sidebar's footer counts the providers the user can use (a key present or a local server), not the configured sections (9 on defaults) (A110). The session list follows app patches instead of a 2 s poll. A shell task's output opens in a viewer.
Check: a screenshot on a live core with a few titled sessions; a test for each new FFI export and the provider count.
Status: done 2026-09-28
**Result:** `App::models` (`crates/cox-app/src/models.rs`), forwarded one-to-one through cox-ffi: the toolbar's model pill opens CoxUI `ModelPopover`, one section per tier, each row the model id and its efforts with the running model marked; a pick sends `SwitchModel{tier, model}` like the TUI's `/model`. The sidebar footer counts a provider only when the doctor's key check finds its key or its loopback server accepts a TCP connect within 300 ms, probed off the main thread (A110). `App::workspace_changed` wakes on a commit to cox.db from another connection or when a session in this app starts, stops or begins waiting; `SidebarStore.watch` replaces the 2 s poll (a 2 s retry if a wait fails). A shell task in the Tasks tab opens its archived output in `TaskOutputSheet` (`LiveSession::output`, sanitized; cox-app now depends on the workspace crate cox-sanitize). The toolbar and sidebar both name an untitled session "Untitled session". The traffic lights sit on the centre of the sidebar toggle row, re-placed after each resize.

**Deviations:** ~500 LOC over 30 files. No title is generated: that is the unapproved ideas.md entry "Session titles", so it stays there. With the sidebar hidden the traffic lights sit ~4 pt below the toolbar's centre.

**Check:** `just test --changed-since p37-desktop` 1559 passed, 6 skipped; clippy on cox-app and cox-ffi and fmt clean. CoxModel 79, CoxUI 179 (11 new snapshots: the model popover and output sheet; none changed), CoxCore 13; CoxTranscript: PinnedDecision and a ComposerFlow test fail only in the full run under load and pass alone, also without this change. swiftlint and swift-format strict clean; the app builds and runs.

#### T37.21.11 Molecule legibility and small fixes

Depends: T37.19.5 · Size: ~100 · Files: `…/Molecules/…`, `desktop/design/DESIGN.md`
Goal: `clock` joins the DS§3.7 symbol table; `SessionRow` reuses `InspectorRow`'s selected-row styling; `DiffStat` hides "−0" for add-only files; section headers and the filter prompt stay readable on Frosted (not `text.tertiary` there); the `KeyCap` inside `StopButton` is visible on the inverted capsule.
Check: the changed snapshots are re-recorded on purpose; each fixed text pair meets DS§8 contrast on all three materials.

Decided (A112): a new placeholder token for the filter prompt; contrast on the glass over the window fill.
Status: done 2026-09-28
**Result:** `clock` joins the DS§3.7 symbol table; `SessionRow` and `InspectorRow` share one `rowSelection` modifier (`InspectorRow.swift`); `DiffStat` shows no "−0" for an add-only change; `SectionHeader` (so the sidebar's "Needs you" too) uses `text.secondary`; the `KeyCap` inside `StopButton` is an outline in `surface.window` on the dark capsule. A112: a new `text.placeholder` token (`#69696e` light, `#a1a1a8` dark, light-hc `#48484c` derived) for `SessionFilter`'s prompt and magnifier, `text.secondary` unchanged; DS§8 measures Frosted and Glossy on the glass over the window fill, and `ContrastTests.swift` checks each fixed pair at 4.5:1 on Solid, Frosted and Glossy, light and dark, with the `frosts: false` compositing (worst: filter prompt 4.50/4.75/4.81, sidebar section header 4.61/4.88/4.93, inspector header 5.07 light and 6.40 dark, Stop key cap 16.8 and 14.7).

**Deviations:** none. The selected session row (`text.secondary` on `accent.soft`) is 3.99:1 in light Solid, unchanged by this card; it waits for a colour decision.

**Check:** `just desktop-tokens` twice, no diff the second time; `node desktop/design/high-contrast.mjs --check` 226 pairs pass. CoxUI 60 snapshots re-recorded on purpose, second run 177; after the merge with T37.22.6, CoxUI 182 passed. swiftlint and swift-format strict clean; the app builds.

#### T37.22.8 Session titles: generated after the first turn, behind a setting

Depends: — · Size: ~200 · Files: `crates/cox-core`, `crates/cox-store` (a migration and a column), `crates/cox-protocol` config
Goal: A113. After a session's first turn, when `[session] auto_title` is on (default on), cox-core runs one low-cost `Job::Title` request (routing D5) on the first prompt and emits `Event::TitleSet`; the store keeps the title in a `sessions` column (Diesel migration, typed DSL); the call is a `usage` row in the ledger like any other. A title set by the user is never overwritten. Scripted scenarios and tests run with it off unless a test is about it. Config key with default.toml, docs and schema regenerated.
Check: a cox-core test that a scripted session with the setting on emits one `TitleSet` after turn 1 and none after turn 2, and with it off none; a cox-store test that the title round-trips; a real-binary run with `COX_HOME=/tmp/…` and `COX_PROVIDER=scripted`.
Status: done 2026-09-28
Result: `[session] auto_title` (default on in `default.toml`, A113). After the first turn of a top-level session, `cox-core` `title.rs` sends one cheap `Job::Title` side request on the first prompt (≤ 2000 chars) and emits `Event::TitleSet`, sanitized and capped at 80 chars; a failure is logged and skipped, history and the cache-stable prefix are untouched, and the call writes a `usage` row. Shared `Session::side_call` (`side.rs`) now also serves `/init`'s README summary (fixing a byte `truncate` on a multi-byte char and a stall on a cut stream). Store migration 5 adds `sessions.title_source` (`auto`|`user`); `rollout_append` stores `TitleSet`; `Store::session_title_set` never lets an auto title replace a user one.

Deviations: Rust `SessionConfig::default()` is off (schema shows `default: false`) so `Config::default()` tests make no model call; scripted `[[turn]]` takes `job = "title"` (cox-provider-testkit, cox-provider); > 3 files (~215 +/66 −); stream-json does not print `TitleSet`, which arrives after `TurnDone`.

Check: cox-core `title::tests`, cox-store `session_title_round_trips_and_keeps_a_user_title`, `just test --changed-since p37-desktop` 1553 passed; clippy on the six crates and fmt clean; real binary under a scratch `COX_HOME` stored title "Fix the ledger sum" (source `auto`) with usage rows `main|code` and `title|cheap`. On the merged tree: 13 title/schema/migration/round-trip tests passed.

Not done: TUI/app display and rename (T37.22.9); `subagent::summarize` and `memory_extract` not moved onto `side_call`.

#### T37.22.7 Model display names from models.dev

Depends: — · Size: ~120 · Files: `scripts/vendor/…`, `crates/cox-models`, the vendored model data
Goal: A111. The `scripts/vendor` script that builds the model catalog also takes each model's `name` from models.dev; `ModelRow` gains `display_name` (with its schema and drift test regenerated); the TUI and the app read it; the app's model pill drops the vendor prefix (`Sonnet 5 · high`). A model without a name falls back to its id.
Check: the vendor script's tests; a cox-models test that `claude-sonnet-5` reads `Claude Sonnet 5`; the pill shows `Sonnet 5 · high` on the approve-write fixture.
Status: done 2026-09-28
Result: `cox-vendor` writes each model's models.dev `name` into `crates/cox-protocol/default.toml` as `display_name` (new `cox-vendor model-names` writes only names; 19 added, prices untouched; User-Agent `cox-dev (https://github.com/listepo/cox)`). `ProviderModel` and `ModelRow` carry an optional `display_name`. cox-app's session status sends `model_name` and `ModelChoice` carries `display_name`; cox-ffi forwards both as new fields (A90). CoxModel `ModelName.short` drops the `Claude ` prefix (A111) and falls back to the id; the composer chip, the toolbar pill and the popover rows use it.

Deviations: > 3 files (ModelChoice request from T37.22.6 and the Swift wiring); the TUI still shows ids (threading the catalog into cox-tui is a separate change).

Check: vendor tests 48 passed, `model-names --check` up to date; nextest cox-protocol/cox-models/cox-app/cox-ffi/cox-config 256 passed, cox-core/cox-session/cox-provider-openai 381, `cox --test docs --test deps` 10; config drift tests pass; clippy and fmt clean; Swift CoxModel 82, CoxCore 13, CoxPlatform 13, CoxTranscriptText 36, PinnedDecision snapshots (pill `Sonnet 5 · high`), swiftlint and swift-format strict clean. On the merged tree: cox-config, cox-models, cox-app 140 passed.

Not done: a full `cox-vendor models` run (would add `medium` efforts and move one OpenRouter price) was left for the creator; only the `Claude ` prefix is dropped, so haiku reads `Haiku 4.5 (latest)` as models.dev names it.

#### T37.44.1 Figma file from the design tokens and mockups

Depends: — · Size: ~150 (a generator script) · Files: `desktop/design/figma/…`, `desktop/design/DESIGN.md`
Goal: A114. The Figma file `cox desktop` (https://www.figma.com/design/KA9a0R7n6P0QbwDn92e167) mirrors the repository's design: variable collections from `desktop/design/tokens/*.json` (colours with Light, Dark, Light HC and Dark HC modes; sizes, radii, spacing, type) built by a saved, tested generator script that turns the tokens into the Figma Plugin API code `use_figma` runs, so a token change re-syncs by re-running it; a page per mockup group holding every screen of `mockups.html` rendered at 2x (`render.sh`) as a reference frame; the main screen 28 rebuilt as editable layers (auto layout) whose fills, radii and spacing are bound to the variables. DESIGN.md says the repository stays the source and how to re-sync.
Check: the generator's test; `get_variable_defs` on the rebuilt screen 28 returns token names, not raw values; a Figma screenshot of the rebuilt screen 28 next to the rendered mockup matches by eye.
Status: done 2026-09-28
Result: Figma file https://www.figma.com/design/KA9a0R7n6P0QbwDn92e167 mirrors the repository (A114): 7 variable collections (Color with 4 modes, Spacing, Radius, Size, Type, Motion, Material), `font/*` text styles and `elevation/*` effect styles, generated by `desktop/design/figma/variables.mjs` (`npm run figma`, tested by `figma/variables.test.mjs`). Pages Main, Approvals, Composer, Inspector & review, Navigation, Settings & onboarding, Later (M2, M3), Glass hold all 30 rendered screens as 1520×980 frames; page "Screen 28 · editable" rebuilds screen 28 as 405 layers with 220 fills bound to colour variables (root node 4:2). `DESIGN.md` §2 "Figma mirror" gives the link, the re-sync steps and the font gap.

Deviations: Motion and Material collections beyond the card; ~190 LOC plus tests; generator sets `showShadowBehindNode:false` as CSS does and probes font rendering (`unrenderedFonts`); the screen-28 extraction tooling stayed in scratch, so that rebuild is not reproducible from the repository.

Check: `cd desktop/design && mise exec -- npm test` 6/6 pass (also on the merged tree); `npm run figma -- --out <dir>` writes one script; `get_variable_defs` on 4:2 returns the token names; about 15 Figma calls.

Not done: fonts — Figma has no SF Mono (mono styles skipped, Roboto Mono stand-in on screen 28) and SF Pro loads but does not render (115 of 135 text layers `hasMissingFont`); the choice is the creator's. The Frosted tile's selection ring does not show; screens other than 28 are images only.

#### T37.22.9 Session titles in the TUI and the app, with rename

Depends: T37.22.8 · Size: ~150 · Files: `crates/cox-tui`, `crates/cox` (a `cox rename` or `/rename`), cox-app/cox-ffi, CoxModel, the app's toolbar and sidebar
Goal: A113. The TUI shows the session title where it shows the session today and in the resume list; `/rename <title>` in the TUI and a rename in the app (double-click the toolbar title or a sidebar row's context menu) set it through one `Submission` that marks the title as the user's. The app's toolbar and sidebar read the title from the store and follow `TitleSet`.
Check: a TUI snapshot with a title; a test that a user rename survives a later generated title; a screenshot of the app with titled sessions.
Plan: add `Submission::Rename { title }` in cox-protocol → core stores it via `Store::session_title_set(.., User)` and emits `TitleSet`; TUI shows the title in the header and resume list and gets `/rename`; cox-app/cox-ffi expose rename and forward `TitleSet`; CoxModel updates the session title; the app renames by double-clicking the toolbar title and from a sidebar row's context menu. Verify: scoped nextest, a TUI insta snapshot, a store/core test that a user title survives a generated one, Swift tests of the touched packages, one app screenshot.
Status: done 2026-09-28
Result: `Submission::Rename { title }`: core cleans it (`title::user_title`), stores it as `TitleSource::User` and emits `TitleSet { by_user: true }`; no generated title follows. TUI: `/rename <title>`, the title in the status line before the mode badge (first segment dropped when narrow), seeded from the store on resume. App: double-click the toolbar title for an inline edit, or a sidebar row's "Rename…" menu, both through `Intent.rename`; the sidebar refreshes on `TitleSet`.

Deviations: a closed session has no core, so cox-app `App::rename` (cox-ffi `rename(session:title:)`) writes the user title to the store directly with the same cleaning; the TUI has no header, so the title sits in the status line; 31 files, ~+566/−88; `docs/protocol.jsonschema` regenerated, `/rename` added to the help snapshot and SVG.

Check: nextest cox-protocol, cox-core, cox-store, cox-tui, cox-app, cox-ffi, cox 934 passed (incl. `status_line_shows_the_session_title_and_rename_submits_it`, `a_user_rename_survives_a_later_generated_title`, `rename_is_a_user_title_and_no_generated_title_follows`, `a_rename_reaches_the_session_list_open_or_closed`); clippy and fmt clean; Swift CoxModel 80, CoxUI 182, CoxCore 13, swift-format and swiftlint strict clean; screenshot of three titled sessions in the sidebar (scripted provider). On the merged tree (with T37.22.7): 18 title/rename/schema tests and cox-app + cox-ffi 110 passed, clippy clean.

Not done: the screenshot shows no in-app rename (osascript keystrokes did not reach the app); rename in the app is covered by tests only.

#### T37.22.12 Model pill drops a trailing "(latest)"

Depends: — · Size: ~15 · Files: `CoxModel/ModelName.swift`, its tests
Goal: A116. `ModelName.short` also drops a trailing ` (latest)`, so models.dev's "Claude Haiku 4.5 (latest)" reads `Haiku 4.5`.
Check: a `ModelNameTests` case for the haiku name and one where "(latest)" is not at the end and stays.
Plan: strip a trailing " (latest)" in `ModelName.short`, add two `ModelNameTests` cases; verify with CoxModel tests and the linters. Done by the same agent as T37.22.11.
Status: done 2026-09-28
Result: `ModelName.short` (`CoxModel/ModelName.swift`) drops a trailing " (latest)" before the vendor prefix (A116), so models.dev's "Claude Haiku 4.5 (latest)" reads `Haiku 4.5`; a name that is only " (latest)" is kept.

Deviations: none.

Check: `aTrailingLatestIsDroppedButOneInsideTheNameStays` (haiku, and "GPT (latest) Mini" stays whole); CoxModel 84 passed; swift-format and swiftlint strict clean.
