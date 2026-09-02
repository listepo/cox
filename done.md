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

#### T0.4 Store: Diesel schema, migrations, rollout files
Model: sonnet · Status: done 2026-09-02 · Depends: T0.2
Goal: §1.7 schema opens, migrates and round-trips rows through Diesel models; rollouts append and read back.
Files: `crates/cox-store/src/{lib,schema,models,rollout}.rs`, `crates/cox-store/migrations/00000000000001_init/{up,down}.sql`, `crates/cox-store/Cargo.toml`, `Cargo.toml` (workspace), `crates/cox/src/{main,doctor}.rs`, `crates/cox/tests/deps.rs`.

What landed: `Store { home, conn: Mutex<SqliteConnection>, rollouts: Mutex<HashMap<SessionId, RolloutWriter>> }` implementing `cox_protocol::{Store, Archive}`. `Store::open` creates `sessions/archive/logs/projects/cassettes` under `home`, opens `cox.db`, sets `journal_mode=WAL; foreign_keys=ON; busy_timeout=5000` via `batch_execute`, then `run_pending_migrations` from `embed_migrations!("migrations")`. `schema.rs` is hand-written `table!` macros (no `diesel_cli` install) for the four non-virtual tables (`migrations`, `sessions`, `usage`, `archive`, `memory`); FTS5 tables have no `table!` entry and are queried via `sql_query`/`QueryableByName` (`memory_search`, currently unexercised — no writer populates `memory`/`memory_fts` yet, `ponytail:`-flagged). `models.rs` holds `Insertable` structs plus one narrow `Queryable` (`ArchiveBytes`: `inline`/`path`/`sha256` only, selected explicitly rather than hydrating the whole row). `archive_put` inlines payloads ≤ 16 KiB, else writes `archive/<id>` and stores the relative path; `archive_get` recomputes the sha256 and returns `StoreError::Corrupt` on mismatch (tested by tampering with an on-disk archive file). `rollout.rs`'s `RolloutWriter` keeps one open `File` + `next_seq` + an unsynced-line counter per session, `fsync`ing every 16 lines and on `Event::TurnDone`; `read_lines` tolerates exactly one truncated trailing line (anything earlier that fails to parse is a hard `Io` error, not silently dropped) and reopening a writer resumes `next_seq` from what it can already parse. `crates/cox/src/doctor.rs` is the minimal T0.4 stub the task asked for — resolves `COX_HOME` (env override else `Store::default_home()`, `~/.cox`) and prints `db: ok`/`db: fail <reason>`; T0.5 replaces its `run()` with the full check list. `only_store_depends_on_diesel` added to `crates/cox/tests/deps.rs`, walking *unfiltered* `cargo metadata` dependency names (workspace_deps() filters to workspace crates only, so a new helper `all_deps()` was added) to assert no crate but `cox-store` names `diesel`/`diesel_migrations`/`libsqlite3-sys`.

No timestamp/date crate was added: `now_rfc3339()` in `lib.rs` formats `SystemTime::now()` with Howard Hinnant's public-domain `civil_from_days` days→Y-M-D algorithm (stdlib only), assuming non-negative days-since-epoch (`ponytail:` comment on the ceiling — real "now" values never hit it).

Deviations:
- **Dependency versions**: plan.md D9/§1.1 says "Diesel 2.2" / "diesel_migrations 2.2" / "libsqlite3-sys 0.30". Verified live against crates.io 2026-09-02: `diesel = "2.2"` and `diesel_migrations = "2.2"` (semver ranges, same as every other workspace dep) both resolve to the newest compatible 2.x releases — `diesel 2.3.12` / `diesel_derives 2.3.9` / `diesel_migrations 2.3.2` — because no `2.2.x` patch exists beyond `2.2.0`; confirmed via a standalone scratch crate that this resolves and builds cleanly (including an FTS5 smoke test through `diesel::sql_query`) before touching the workspace. `libsqlite3-sys = "0.30"` was tried pinned as specified and also resolves cleanly (`0.30.1`) against `diesel 2.3.12`'s `sqlite` feature, so it is pinned as the plan says (no conflict to route around) — `sha2 = "0.10"` added, not in plan.md §1.1's row but implied by "sha256 verified on read"; noted here as the one-line reason.
- **No `cox-protocol` edits.** `SessionRow`/`UsageRow`/`ArchivePut`/`Archive` needed nothing extra: `sessions` columns absent from `SessionRow` (`updated_at`, `turns`, `cost_usd`, `state`) are creation-time defaults (`updated_at = created_at`, `turns = 0`, `cost_usd = 0.0`, `state = "open"`), and `usage.context_tokens`/`created_at` are derived (`Usage::context_tokens()`, `now_rfc3339()`) rather than caller-supplied.
- `crates/cox/Cargo.toml` was not touched (out of scope per the task) — `doctor.rs` reaches the store purely through `cox-store`/`cox-protocol`, already path-deps of `crates/cox`.

Check:
```bash
$ mise exec -- cargo test -p cox-store
test rollout::tests::append_and_read_round_trip ... ok
test rollout::tests::truncated_last_line_is_dropped_not_fatal ... ok
test rollout::tests::writer_resumes_seq_after_reopen ... ok
test tests::archive_get_detects_corrupt_bytes ... ok
test tests::archive_roundtrip_inline_and_file ... ok
test tests::migrations_are_idempotent ... ok
test tests::rollout_survives_truncated_tail ... ok
test tests::schema_snapshot_matches ... ok
test tests::usage_insert_and_sum ... ok
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

$ COX_HOME=$(mktemp -d) mise exec -- cargo run -q -- doctor | grep 'db: ok'
db: ok

$ mise exec -- cargo clippy -p cox-store --all-targets -- -D warnings
(clean)
$ mise exec -- cargo clippy -p cox --all-targets -- -D warnings
(clean)
$ mise exec -- cargo fmt --check -p cox-store && mise exec -- cargo fmt --check -p cox
(clean)
$ mise exec -- cargo test -p cox --test deps
test only_store_depends_on_diesel ... ok
test no_crate_below_cox_depends_on_core ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Out of scope (per task): FTS indexing of rollouts (T10.3); full `cox doctor` check list (T0.5); `memory_*` writers (a later task — `memory_search` is real but untested against live data).

#### T0.3 Config loading and provenance
Model: sonnet · Status: done 2026-09-02
Goal: layered config (default/user/project/env/flag) with per-key provenance, project-config guard list, and a `cox config` subcommand.
Files: `config/default.toml` (verbatim §1.6, `include_str!`'d), `crates/cox-protocol/src/config.rs` (`Config` struct tree, one struct per table, `deny_unknown_fields, default` + hand-written `Default` impls matching default.toml), `crates/cox-protocol/src/lib.rs` (export `Config`/`DEFAULT_CONFIG_TOML`), `docs/config.md` (generated + committed, create-on-first-run test keeps it in sync with default.toml), `crates/cox/Cargo.toml` (added `figment`, `toml_edit`, `anyhow`; moved `serde_json` to normal deps; `tempfile` dev-dep), `crates/cox/src/cli.rs` (clap `Cli`/global flags/`Config` subcommand), `crates/cox/src/config_load.rs` (figment layering via a `Named<P>` metadata wrapper for provenance, `COX_HOME` special-casing, project guard list, `flag_key_map()`), `crates/cox/src/config_cmd.rs` (`show`/`get`/`set`/`path`, `toml_edit` comment-preserving writes), `crates/cox/src/main.rs` (thin dispatch).
Notes / deviations:
- **`HooksConfig`/`McpConfig` skip `deny_unknown_fields`.** Both use `#[serde(flatten)]` for their dynamic maps (`events: HashMap<String, Vec<HookConfig>>`, `servers: HashMap<String, McpServerConfig>`), which serde forbids combining with `deny_unknown_fields` on the same struct. Documented with a doc comment at each struct.
- **`HooksConfig`/`McpConfig` each got an extra `enabled: bool` field** (default `true`, not present in default.toml) to back `--no-hooks`/`--no-mcp`.
- **Flag-key map carries a `runtime.*` namespace** for CLI flags that map to `RunArgs` rather than a persisted config key (`prompt`, `output-format`, `max-turns`, `allowed-tools`, `answer`, `continue`, `resume`, `deep`) so `every_flag_has_a_config_key` has a real entry for every flag without inventing persisted config surface for run-only options.
- **`Toml::file()` not `Toml::file_exact()`** for user/project layers — `file_exact` hard-errors when the file is absent; `file()` on an absolute path checks existence first and returns empty data, which is what "optional user/project config" needs.
- **Guard-list reversion computed in Rust, not in figment's Value tree** — build two figments (`default+user+project+env+flag` and `default+user+env+flag`), extract both into `Config`, diff the 6 guarded keys, and revert violations on the struct directly; `LoadedConfig::source_of()` consults whichever figment matches for provenance on a reverted key.
Check:
```
$ mise exec -- cargo test -p cox-protocol config_
running 4 tests
test config::tests::config_default_matches_hand_built_defaults ... ok
test config::tests::config_hooks_deny_unknown_but_accept_event_arrays ... ok
test config::tests::config_json_roundtrip ... ok
test config::tests::config_docs_config_md_matches_default_toml ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 43 filtered out; finished in 0.00s

$ mise exec -- cargo test -p cox config_
running 8 tests
test config_load::tests::config_every_flag_has_a_config_key ... ok
test cli::tests::config_cli_parses_run_and_config_subcommands ... ok
test config_cmd::tests::config_set_preserves_comments ... ok
test cli::tests::config_cli_command_builds_without_panicking ... ok
test config_cmd::tests::config_set_creates_missing_file_and_parents ... ok
test config_load::tests::config_defaults_parse ... ok
test config_load::tests::config_project_cannot_raise_budget ... ok
test config_load::tests::config_env_overrides_project ... ok
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

$ COX_TIERS_CODE_MODEL=claude-opus-5 mise exec -- cargo run -q -- config show --sources | grep 'tiers.code.model = "claude-opus-5"  # env'
tiers.code.model = "claude-opus-5"  # env
```
`cargo fmt --check -p cox-protocol -p cox`: clean. `cargo clippy -p cox-protocol -p cox --all-targets -- -D warnings`: at commit time this transitively fails inside `cox-provider` (`clone_on_copy` on `Usage` in `crates/cox-provider/src/anthropic/stream.rs:111,243`) — that crate is mid-edit by the parallel T1.2 task and outside T0.3's file scope; clippy on `cox-protocol`/`cox`'s own code has no findings once `cox-provider` builds.
Manually smoke-tested `cox config path/get/set/show --sources` against a scratch `COX_HOME`.

#### T3.1 Path confinement and `ToolCx`
Model: sonnet · Status: done 2026-09-02 · Depends: T2.2
Goal: no path from the model escapes the workspace roots.
Files: `crates/cox-tools/src/{lib,path}.rs`, `crates/cox-tools/tests/confine.rs`, `crates/cox-tools/Cargo.toml`.

What landed: `cox_tools::path::confine(roots: &[PathBuf], cwd: &Path, input: &str) -> Result<PathBuf, ToolError>`. Order: (1) reject NUL and any `:` (blanket-bans Windows drive/ADS syntax, `\\?\`, plus a leading `\\` for UNC) — cheaper and more conservative than pattern-matching each Windows form, `ponytail:`-flagged in the doc comment. (2) expand a leading `~`/`~/…` via `$HOME`, join relative to `cwd`. (3) a filesystem-free lexical `.`/`..` collapse (`PathBuf::pop`, a no-op at the root, so a `..` chain clamps at `/` instead of underflowing) checked against lexical roots — a cheap first reject. (4) the authoritative check: walk the *raw*, un-collapsed joined path (via its `Component` list, not `Path::pop`/`file_name`, which return `None` once the trailing component is `.`/`..` and would cut a mid-walk `..` short) down to the deepest existing ancestor, `canonicalize` only that ancestor, reattach the non-existent tail, lexically collapse once more, and check containment against canonicalized roots. Canonicalizing the raw (not lexically-pre-collapsed) path is what catches `linkdir/../secret.txt` where `linkdir` is a symlink pointing outside every root: a purely lexical check cancels `linkdir/..` to nothing and would let it through; letting the OS resolve the symlink first (by checking `.exists()`/`canonicalize` on the un-collapsed prefix) resolves `..` against where the symlink really points. `ToolError::Confined` reports whichever configured root shares the longest component prefix with the offending path.

`tool_cx()` in `lib.rs` is a thin named constructor (`roots, cwd, sandbox, archive, cancel, output, session, call) -> ToolCx`) — every `ToolCx` field is already `pub`, so this isn't a real builder, just one place callers look instead of repeating the struct literal. **Session-config wiring (T2.2/T0.3) is out of scope here** — every argument is a plain value the caller must already have; no default-filling from `Config` was added.

`tests/confine.rs`: 20 `confine_*` tests (plain functions, not `rstest` — the fixture setup differs enough per case, symlinks vs. plain dirs vs. `$HOME`, that a single parametrized table added more ceremony than it removed) covering: plain relative path, root itself, non-existent leaf in an existing dir, deeply non-existent nested path, `./a/../b`-style collapse, trailing slash, `~` expansion (against the real `$HOME`, no env mutation — avoids a race with parallel test threads), `cwd` vs. root distinction, a second root, plain `..` escape above root, absolute path outside roots, symlink-to-outside, `..` through a symlink, NUL, `C:\x`, `\\?\C:\x`, `file.txt:stream`, a bare `\\server\share` UNC prefix, and empty `roots`. Plus `confine_is_the_only_path_constructor`, the done-when grep guard: walks `crates/cox-tools/src`, fails if any file but `path.rs` contains `Path::new(input` or `PathBuf::from(input`. Every fixture root is canonicalized once at setup (not compared against the raw tempdir path) because macOS tempdirs sit behind `/tmp` → `/private/tmp`-style symlinks that `confine`'s own resolution step would otherwise turn into a spurious mismatch.

Deviations:
- Deps added to `cox-tools` only (all already workspace-declared, no new §1 rows needed): `tokio`, `tokio-util` (for `ToolCx`'s `mpsc::Sender`/`CancellationToken` fields in `tool_cx()`); dev-deps `async-trait` (only to implement the `Archive` trait for a `NoopArchive` test double), `tempfile`, `rstest` (pulled in per the task but not used as a parametrizing macro — see above).
- No `read`/`edit`/`write`/etc. tool exists yet to call `confine`, so `confine_is_the_only_path_constructor` is trivially green; it starts pulling weight from T3.2 onward.

Check:
```
$ mise exec -- cargo test -p cox-tools confine_
running 3 tests
test tests::tool_cx_wires_every_field_through ... ok
test path::tests::confine_rejects_dotdot_escape_above_root ... ok
test path::tests::confine_plain_relative_path_stays_in_root ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 20 tests (tests/confine.rs)
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ mise exec -- cargo clippy -p cox-tools --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s)
(clean)

$ mise exec -- cargo fmt -p cox-tools --check
(clean)
```
