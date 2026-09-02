# cox — finished tasks

Tasks move here verbatim from `plan.md` §3 when their Check passes, with `Status: done <date>` and the Check output. Newest last.

#### T0.1 Workspace scaffold
Model: sonnet · Status: done 2026-09-02
Goal: ten crates build empty, CI runs fmt/clippy/nextest on macOS and Linux, dependency direction is enforced.
Files: `Cargo.toml` (workspace), `crates/*/Cargo.toml` + `src/lib.rs` (each with a `//!` header), `crates/cox/src/main.rs`, `crates/cox/tests/deps.rs`, `justfile`, `deny.toml`, `.github/workflows/ci.yml`.
Note: `cox --version` prints `cox 0.1.0` (not `0.1.0-dev` as plan.md's "Done when" literally says — `0.1.0-dev` is not valid Cargo semver; task instructions for this run explicitly specified `version = "0.1.0"`). Path dependencies between workspace crates carry an explicit `version = "0.1.0"` alongside `path = ...` so `cargo deny check` does not flag them as wildcard deps.
Check:
```
$ mise exec -- cargo build --workspace && mise exec -- cargo test --workspace && mise exec -- cargo clippy --workspace --all-targets -- -D warnings && mise exec -- cargo fmt --check
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.34s
test no_crate_below_cox_depends_on_core ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
(all lib/doc-test suites: 0 passed; 0 failed)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s
EXIT:0

$ mise exec -- cargo deny check
advisories ok, bans ok, licenses ok, sources ok
```

#### T0.2 Protocol types
Model: sonnet · Status: done 2026-09-02
Goal: every type in §1.2 exists with serde round-trips and schemars for tool schemas.
Files: `crates/cox-protocol/src/{lib,ids,types,errors,traits}.rs` (5 source files, see deviation below), `crates/cox-protocol/Cargo.toml`, `Cargo.toml` (workspace deps: added `ulid`, `tokio-util`), `docs/protocol.jsonschema` (generated + committed).
Notes / deviations:
- **5 files instead of ≤3.** The task instructions explicitly pre-authorized this ("acceptable to use up to 5 files here since the plan lists them"): `ids.rs` (ULID newtypes), `types.rs` (the full `Submission`/`Event`/`Request` type graph — the bulk of the LOC), `errors.rs` (§1.14 taxonomy), `traits.rs` (`Provider`/`Tool`/`ToolCx`/`Store`/`Hook`/`Archive`), `lib.rs` (docs, re-exports, the `docs/protocol.jsonschema` generator test). Total is well over the nominal ~200 LOC size guidance in `plan.md`/AGENTS.md — the sheer number of named types in §1.2 (60+) with full doc comments and tests made that unavoidable while staying faithful to the listing; flagging per AGENTS.md rather than silently exceeding it.
- **`tokio` feature scope.** Asked for `tokio` with the `sync` feature only (no I/O in this crate). Cargo's workspace-dependency inheritance forbids a member from setting `default-features = false` when the workspace's own `[workspace.dependencies.tokio]` doesn't (`error inheriting tokio ... default-features = false cannot override workspace's default-features`), so `cox-protocol` inherits the workspace's `features = ["full"]` via `tokio = { workspace = true }`. No behavioural difference since Cargo unifies features per build anyway; noted here since it doesn't match the literal instruction.
- **`Archive` trait invented.** Not spelled out in plan.md's pseudocode (only referenced as `Arc<dyn Archive>` in `ToolCx` and via `Store::archive_put`/`archive_get`). Added a narrow async trait (`put`/`get`) separate from the sync `Store` trait, since tool execution is async and `Store` is deliberately sync (D9); the concrete `cox-store` implementation is expected to dispatch onto a blocking task.
- **`Item`/`ItemKind` shape invented.** Plan.md names `Item`/`ItemKind` in the crate's "owns" list and gives `ItemKind`'s variant names via `ItemStarted{item, kind}`, but never a field-level schema. `ItemKind` variants carry what's needed to rebuild history on resume (per §1.7: "resume rebuilds `history` from `ItemStarted`/`ItemDone` pairs"); `Item{id, turn, kind}` is the obvious minimal wrapper.
- **`SessionRow`/`UsageRow`/`ArchivePut`/`MemoryHit` shapes invented** to match the `Store` trait signature and the §1.7 SQL schema's columns (session id/created_at/cwd/project_slug/title/parent_id/rollout_path; usage's job/tier/provider/model/usage; archive's session/call/tool/subject/bytes; memory's name/path/snippet). Timestamps are `String` (RFC 3339) rather than adding a `chrono`/`time` dependency not in `plan.md` §1.1.
- **Error taxonomy fields.** Where plan.md's table cell gives only a bare variant name with no field list (`Timeout`, `NotFound`, `Io`, `Sqlite`, `Binary`, `Cancelled`, …), kept them as unit variants exactly as written — in particular `Io`/`Sqlite` carry no wrapped message, since the underlying `std::io::Error`/sqlite error types aren't `Clone`/`Serialize` and errors here must be both (per the task's requirement that they can live inside `Event`). Where a variant names a field (`RateLimited { retry_after }`, `Parse { line }`, …) chose the obvious type (`retry_after: Option<u64>` seconds, `line: u64` line number, etc.).
- **`ProviderError`/`ToolError`/`StoreError` also derive `schemars::JsonSchema`** (beyond what T0.2's step 4 explicitly asked for) because `CoreError` — which wraps them — is reachable from `Event::Error` and the task requires `docs/protocol.jsonschema` to cover `Event` fully.
- No `unwrap`/`expect`/`panic!`/`todo!` outside `#[cfg(test)]` blocks (checked by hand; every occurrence is inside a `mod tests` at the bottom of its file, per AGENTS.md convention).
Check:
```
$ mise exec -- cargo test -p cox-protocol
running 43 tests
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests cox_protocol
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ mise exec -- cargo doc -p cox-protocol --no-deps
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.09s
   Generated /Users/listepo/.cargo/shared_target/doc/cox_protocol/index.html
(no missing-docs warnings)

$ mise exec -- cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.18s
(clean)

$ mise exec -- cargo fmt --check
(clean, after `cargo fmt`)

$ mise exec -- cargo test --workspace
(all crates: ok; cox-protocol 43 passed, 0 failed)

$ mise exec -- cargo deny check
advisories ok, bans ok, licenses ok, sources ok
```

#### T0.6 Protocol design doc
Model: sonnet · Status: done 2026-09-02
Goal: `docs/design/protocol.md` per D15 — problem in one measurable number, what the field does, what cox does and why, falsifiers.
Files: `docs/design/protocol.md`, `research.md` (one-line link at end of §1.2).
Check: file exists, ≤ 1 page, four sections; think review pending.

#### T1.1 Anthropic request translation
Model: opus · Status: done 2026-09-02 · Depends: T0.2
Goal: a `Request` becomes a byte-exact Anthropic Messages body with cache breakpoints, thinking, effort, fallbacks and tool results.
Files: `crates/cox-provider/src/lib.rs`, `crates/cox-provider/src/anthropic/{mod,request}.rs`, `crates/cox-provider/Cargo.toml`, `docs/design/provider.md`.

What landed: `AnthropicProvider { base_url, api_key, ttl, fallbacks, http }` with a headers builder (`anthropic-version: 2023-06-01`, sensitive `x-api-key`, `anthropic-beta` assembled only from enabled features), `Caps`, and `resolve_api_key()` (`ANTHROPIC_API_KEY`, else keyring entry `cox/anthropic`, else `ProviderError::Auth` — never a panic). `request::build_body(&Request, BuildCfg) -> Value` is pure: system text blocks, `tools` + `tool_choice: auto`, `thinking: {"type":"adaptive"}` on 4.6+/5 families, `output_config.effort`, `max_tokens`, `stop_sequences`, `stream: true`, `fallbacks: "default"`. `Provider::stream`/`count_tokens` return `Unsupported` until T1.2.

Breakpoint indexing (documented in the module header): `cache_breakpoints` index the concatenation `system ++ messages`; a system index marks that text block, a message index marks that message's *last* content block. Out-of-range indices and indices naming a `SystemBlock { cache: false }` are skipped rather than failing the turn; placement clamps at four (`MAX_BREAKPOINTS`).

Deviations:
- **No `produced_by` added to `Content::Thinking`.** The task allowed adding the field to `cox-protocol`, but that crate is owned by parallel work and its shape is pinned by the committed `docs/protocol.jsonschema` test. Provenance is carried instead as `BuildCfg::thinking_model: Option<&ModelId>` — the caller saw `ModelSwitched` and knows it. A block replays only when a signature is present *and* `thinking_model == req.model`; `None` is treated as a switch (never guess).
- **`keyring` is pulled in without a platform-store feature**, so the keyring branch resolves against its mock store until `cox-store` enables `apple-native`/`windows-native`/`sync-secret-service` (features union across the workspace). Marked with a `ponytail:` comment; the env var is the working path today.
- Deps added to `cox-provider` only (all already workspace-declared, no new rows in plan.md §1): `async-trait`, `keyring`, `reqwest`, `serde_json`, `tokio`, `tokio-util`, dev `insta`. Workspace `Cargo.toml` untouched.
- Messages are translated in place, not merged: parallel tool results are expected to arrive as several `Content::ToolResult`s inside one user `Message` (which is what context assembly builds), so merging consecutive same-role messages — which would also break breakpoint indices — is not done.

Sources consulted (bundled `claude-api` skill, 2026-09-02): `curl/examples.md` (`anthropic-version: 2023-06-01`, `cache_control: {"type":"ephemeral","ttl":"1h"}`, `tool_result` shape, `thinking: {"type":"adaptive"}`, `budget_tokens` is a 400 on 4.7+/5); `shared/prompt-caching.md` (max **4** `cache_control` breakpoints per request; render order `tools → system → messages`); `python/claude-api/README.md` (`output_config: {"effort": "low|medium|high|xhigh|max"}`); `shared/model-migration.md` §"Migrating to Claude Opus 5 → New API features" (`fallbacks: "default"` scalar form, beta header **`server-side-fallback-2026-07-01`**, distinct from the array form's `-2026-06-01`) and §1591/1601 (a thinking signature binds the block to the model and the prefix that produced it); `shared/token-counting.md` (`POST /v1/messages/count_tokens`); `shared/tool-use-concepts.md` (forced `tool_choice` is a 400 on Fable/Mythos 5.1 → always `auto`).

Check:
```bash
$ mise exec -- cargo test -p cox-provider anthropic_request_
running 3 tests
test anthropic::request::tests::anthropic_request_plain_text ... ok
test anthropic::request::tests::anthropic_request_after_compaction ... ok
test anthropic::request::tests::anthropic_request_parallel_tool_results ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.02s

$ mise exec -- cargo clippy -p cox-provider --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.41s
(clean)

$ mise exec -- cargo fmt --check -p cox-provider
(clean)

$ mise exec -- cargo test -p cox-provider
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
