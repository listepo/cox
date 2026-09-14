# Toolchain

Project programs and direct packages from manifests.

## Programs

| Program | How to install | Why here | Source |
| --- | --- | --- | --- |
| mise | brew / curl, then `mise install` | Pinned tool versions | https://github.com/jdx/mise |
| cargo-cache | mise | `just cache` / `just cache-autoclean`; the shared cargo home fills up | https://github.com/matthiaskrgr/cargo-cache |
| rust | mise | Compiler and std | https://github.com/rust-lang/rust |
| rustc | mise (pin rust) | Rust compiler | https://github.com/rust-lang/rust |
| cargo | mise (pin rust) | Rust build and dependencies | https://github.com/rust-lang/cargo |
| just | cargo install just / brew | Command recipes | https://github.com/casey/just |

## cargo

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| agent-client-protocol | local | https://crates.io/crates/agent-client-protocol | cox-acp |
| anyhow | local | https://crates.io/crates/anyhow | CLI errors |
| arboard | local | https://crates.io/crates/arboard | Rust dependency |
| assert_cmd | local | https://crates.io/crates/assert_cmd | Rust dependency |
| assert_fs | local | https://crates.io/crates/assert_fs | Rust dependency |
| async-trait | local | https://crates.io/crates/async-trait | Rust dependency |
| bytes | local | https://crates.io/crates/bytes | T1.2: turns a reqwest byte stream into SSE frames (`sse.rs`) and drives the in-memory fixture parser (`parse_sse_str`) through the same code path. |
| clap | local | https://crates.io/crates/clap | cox (CLI) |
| crossterm | local | https://crates.io/crates/crossterm | Terminal |
| diesel | local | https://crates.io/crates/diesel | cox-store diesel/diesel_migrations pinned "2.2" per plan.md D9 resolve to the latest 2.x compatible release (2.3.x) on crates.io as of 2026-09-02; verified no semver-breaking API change vs. 2.2 for the sqlite backend used here. |
| diesel_migrations | local | https://crates.io/crates/diesel_migrations | SQLite migrations |
| diffy | local | https://crates.io/crates/diffy | Rust dependency |
| directories | local | https://crates.io/crates/directories | Rust dependency |
| divan | local | https://crates.io/crates/divan | Dev-deps: microbench harness for the permission engine (`cox-core/benches/permission.rs`). |
| dotenvy | local | https://crates.io/crates/dotenvy | T0.7: load local .env files without overriding the process environment. |
| eventsource-stream | local | https://crates.io/crates/eventsource-stream | Rust dependency |
| figment | local | https://crates.io/crates/figment | Config |
| futures | local | https://crates.io/crates/futures | Rust dependency |
| globset | local | https://crates.io/crates/globset | Rust dependency |
| grep-regex | local | https://crates.io/crates/grep-regex | T3.3: plan.md names "grep-regex + grep-searcher sinks"; grep-regex (the RegexMatcher grep-searcher needs) was missing from this list. |
| grep-searcher | local | https://crates.io/crates/grep-searcher | Rust dependency |
| ignore | local | https://crates.io/crates/ignore | cox-tools |
| insta | local | https://crates.io/crates/insta | dev-deps |
| keyring | local | https://crates.io/crates/keyring | Rust dependency |
| landlock | local | https://crates.io/crates/landlock | Rust dependency |
| libfuzzer-sys | local | https://crates.io/crates/libfuzzer-sys | Rust dependency |
| libsqlite3-sys | local | https://crates.io/crates/libsqlite3-sys | Rust dependency |
| nix | local | https://crates.io/crates/nix | Rust dependency |
| nucleo | local | https://crates.io/crates/nucleo | Rust dependency |
| opentelemetry | local | https://crates.io/crates/opentelemetry | D16/T13: OTLP/HTTP keeps telemetry vendor-neutral (SigNoz, Jaeger, Grafana/Tempo and hosted collectors) without adding a backend SDK. |
| opentelemetry-appender-tracing | local | https://crates.io/crates/opentelemetry-appender-tracing | Rust dependency |
| opentelemetry-otlp | local | https://crates.io/crates/opentelemetry-otlp | Rust dependency |
| opentelemetry_sdk | local | https://crates.io/crates/opentelemetry_sdk | Rust dependency |
| portable-pty | local | https://crates.io/crates/portable-pty | Rust dependency |
| predicates | local | https://crates.io/crates/predicates | Rust dependency |
| pretty_assertions | local | https://crates.io/crates/pretty_assertions | Rust dependency |
| proptest | local | https://crates.io/crates/proptest | Rust dependency |
| pulldown-cmark | local | https://crates.io/crates/pulldown-cmark | T5.3: plan.md says pulldown-cmark 0.10; 0.13 is the current line with the same Tag/TagEnd API. syntect without onig (pure-Rust fancy-regex engine). |
| ratatui | local | https://crates.io/crates/ratatui | cox-tui |
| reqwest | local | https://crates.io/crates/reqwest | cox-provider |
| rmcp | local | https://crates.io/crates/rmcp | cox-mcp |
| rstest | local | https://crates.io/crates/rstest | Rust dependency |
| schemars | local | https://crates.io/crates/schemars | Rust dependency |
| seccompiler | local | https://crates.io/crates/seccompiler | Rust dependency |
| serde | local | https://crates.io/crates/serde | cox-protocol |
| serde_json | local | https://crates.io/crates/serde_json | preserve_order: agent-client-protocol-schema requires it, which unifies the feature into every workspace build anyway; pinning it here makes JSON key order (snapshots, rollout lines, ledger JSON) identical for per-crate and full-workspace runs instead of depending on the invocation. |
| serde_yaml | local | https://crates.io/crates/serde_yaml | cox-ext |
| sha2 | local | https://crates.io/crates/sha2 | Rust dependency |
| shlex | local | https://crates.io/crates/shlex | Rust dependency |
| similar | local | https://crates.io/crates/similar | Rust dependency |
| syntect | local | https://crates.io/crates/syntect | Rust dependency |
| tempfile | local | https://crates.io/crates/tempfile | Rust dependency |
| thiserror | local | https://crates.io/crates/thiserror | Errors |
| tiktoken-rs | local | https://crates.io/crates/tiktoken-rs | Rust dependency |
| tokio | local | https://crates.io/crates/tokio | cox-core |
| tokio-util | local | https://crates.io/crates/tokio-util | Rust dependency |
| toml_edit | local | https://crates.io/crates/toml_edit | Rust dependency |
| tracing | local | https://crates.io/crates/tracing | Logs |
| tracing-appender | local | https://crates.io/crates/tracing-appender | Rust dependency |
| tracing-opentelemetry | local | https://crates.io/crates/tracing-opentelemetry | Rust dependency |
| tracing-subscriber | local | https://crates.io/crates/tracing-subscriber | Rust dependency |
| tree-sitter | local | https://crates.io/crates/tree-sitter | Rust dependency |
| tree-sitter-bash | local | https://crates.io/crates/tree-sitter-bash | Rust dependency |
| tree-sitter-go | local | https://crates.io/crates/tree-sitter-go | Rust dependency |
| tree-sitter-python | local | https://crates.io/crates/tree-sitter-python | Rust dependency |
| tree-sitter-rust | local | https://crates.io/crates/tree-sitter-rust | Rust dependency |
| tree-sitter-typescript | local | https://crates.io/crates/tree-sitter-typescript | Rust dependency |
| trycmd | local | https://crates.io/crates/trycmd | Dev-deps: CLI snapshot fixtures for headless `cox run -p` output (`cox/tests/trycmd/*.toml`). |
| tui-textarea-2 | local | https://crates.io/crates/tui-textarea-2 | Rust dependency |
| ulid | local | https://crates.io/crates/ulid | Ids |
| unicode-width | local | https://crates.io/crates/unicode-width | Rust dependency |
| vt100 | local | https://crates.io/crates/vt100 | Rust dependency |
| wiremock | local | https://crates.io/crates/wiremock | Rust dependency |
