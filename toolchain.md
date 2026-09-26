# Toolchain

Programs the project uses and the direct packages from its manifests.

## Programs

| Program | How to install | Why here | Source |
| --- | --- | --- | --- |
| mise | brew / curl, then `mise install` | Pins tool versions | https://github.com/jdx/mise |
| cargo-cache | mise | `just cache` / `just cache-autoclean`; the shared cargo home fills up | https://github.com/matthiaskrgr/cargo-cache |
| rust | mise (with the `wasm32-unknown-unknown` target) | Compiler and std; the target builds the guest workspace `plugins/` (T33.27) | https://github.com/rust-lang/rust |
| rustc | mise (pin rust) | Rust compiler | https://github.com/rust-lang/rust |
| cargo | mise (pin rust) | Rust builds and dependencies | https://github.com/rust-lang/cargo |
| cargo-nextest | global (cargo install / brew) | Parallel test runner | https://github.com/nextest-rs/nextest |
| just | cargo install just / brew | Command recipes | https://github.com/casey/just |
| ketch | see its README | Installs dunnage | https://github.com/listepo/ketch |
| dunnage | ketch | `just test` ends with a lossless cleanup of `target/` | https://github.com/listepo/dunnage |
| uv | global (curl installer / brew) | Runs the `evals/` and `scripts/vendor/` packages (`just eval`, `just vendor`) and locks their Python deps | https://github.com/astral-sh/uv |
| python | uv (`evals/.python-version`, `scripts/vendor/.python-version`) | Eval harness, the Terminal-Bench agent (must be a Python class), and `cox-vendor` (T30.19: vendored files no package manager fetches) | https://github.com/python/cpython |
| zig | mise (`mise.toml`) | Linker for `cargo zigbuild`: the Linux cox the Terminal-Bench containers run (T30.9) | https://github.com/ziglang/zig |
| cargo-zigbuild | mise (`mise.toml`, aqua) | Cross-builds that Linux cox from macOS without a Docker build step | https://github.com/rust-cross/cargo-zigbuild |
| colima | global (mise) | Docker runtime for Terminal-Bench (the creator's choice) | https://github.com/abiosoft/colima |
| LM Studio (`lms`) | desktop app | Local model server for the eval matrix (`cox-bench`, provider `lmstudio`): OpenAI Chat and Anthropic Messages endpoints on :1234 | https://lmstudio.ai/docs/developer |
| docker-cli, docker-compose, docker-buildx | global (mise) | Harbor drives task containers through `docker compose` and `docker buildx build` | https://github.com/docker/cli , https://github.com/docker/compose , https://github.com/docker/buildx |

## ketch

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| dunnage | global | https://github.com/listepo/dunnage | Lossless `target/` cleanup after tests |

## cargo

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| agent-client-protocol | local | https://crates.io/crates/agent-client-protocol | cox-acp |
| anyhow | local | https://crates.io/crates/anyhow | CLI errors |
| arboard | local | https://crates.io/crates/arboard | Rust dependency |
| assert_cmd | local | https://crates.io/crates/assert_cmd | Rust dependency |
| assert_fs | local | https://crates.io/crates/assert_fs | Rust dependency |
| async-openai | local (`response-types` only) | https://github.com/64bit/async-openai | cox-provider: OpenAI Responses request and stream event types (T30.11); transport stays ours |
| async-trait | local | https://crates.io/crates/async-trait | Rust dependency |
| bytes | local | https://crates.io/crates/bytes | T1.2: turns a reqwest byte stream into SSE frames (`sse.rs`) and drives the in-memory fixture parser (`parse_sse_str`) through the same code path. |
| clap | local | https://crates.io/crates/clap | cox (CLI) |
| crossterm | local | https://crates.io/crates/crossterm | Terminal I/O |
| diesel | local | https://crates.io/crates/diesel | cox-store diesel/diesel_migrations pinned "2.2" per plan.md D9 resolve to the latest 2.x compatible release (2.3.x) on crates.io as of 2026-09-02; verified no semver-breaking API change vs. 2.2 for the sqlite backend used here. |
| diesel_migrations | local | https://crates.io/crates/diesel_migrations | SQLite migrations |
| diffy | local | https://crates.io/crates/diffy | Rust dependency |
| directories | local | https://crates.io/crates/directories | Rust dependency |
| dotenvy | local | https://crates.io/crates/dotenvy | T0.7: load local .env files without overriding the process environment. |
| eventsource-stream | local | https://crates.io/crates/eventsource-stream | Rust dependency |
| extism | local (default features off) | https://github.com/extism/extism | cox-plugin: the WASM plugin host (A52, T33.3); no ureq, no URL or file module loading |
| extism-pdk | local, `plugins/` guest workspace (default features off) | https://github.com/extism/rust-pdk | cox-plugin-sdk: the official Rust PDK the guest SDK wraps (exports, `cox:host/v1` imports, extism memory; T33.27) |
| figment | local | https://crates.io/crates/figment | Config loading |
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
| pathdiff | local | https://crates.io/crates/pathdiff | Relative path between two paths |
| portable-pty | local | https://crates.io/crates/portable-pty | Rust dependency |
| predicates | local | https://crates.io/crates/predicates | Rust dependency |
| pretty_assertions | local | https://crates.io/crates/pretty_assertions | Rust dependency |
| proptest | local | https://crates.io/crates/proptest | Rust dependency |
| pulldown-cmark | local | https://crates.io/crates/pulldown-cmark | T5.3: plan.md says pulldown-cmark 0.10; 0.13 is the current line with the same Tag/TagEnd API. syntect without onig (pure-Rust fancy-regex engine). Lives in `cox-render` (T32.2). |
| ratatui | local | https://crates.io/crates/ratatui | cox-tui |
| reqwest | local | https://crates.io/crates/reqwest | cox-provider |
| rmcp | local | https://crates.io/crates/rmcp | cox-mcp |
| rstest | local | https://crates.io/crates/rstest | Rust dependency |
| schemars | local | https://crates.io/crates/schemars | Rust dependency |
| schemars 0.8 | local (build-dependency) | https://crates.io/crates/schemars | cox-provider `build.rs`: typify 0.8's `TypeSpace` takes schemars 0.8 schema types |
| seccompiler | local | https://crates.io/crates/seccompiler | Rust dependency |
| serde | local | https://crates.io/crates/serde | cox-protocol |
| serde_json | local | https://crates.io/crates/serde_json | preserve_order: agent-client-protocol-schema requires it, which unifies the feature into every workspace build anyway; pinning it here makes JSON key order (snapshots, rollout lines, ledger JSON) identical for per-crate and full-workspace runs instead of depending on the invocation. |
| serde_yaml | local | https://crates.io/crates/serde_yaml | cox-ext |
| sha2 | local | https://crates.io/crates/sha2 | Rust dependency |
| shlex | local | https://crates.io/crates/shlex | Rust dependency |
| similar | local | https://crates.io/crates/similar | Rust dependency |
| syntect | local | https://crates.io/crates/syntect | Rust dependency Lives in `cox-render` (T32.2). |
| two-face | local | https://crates.io/crates/two-face | T24.3: extended syntax definitions Lives in `cox-render` (T32.2). |
| terminal-colorsaurus | local | https://crates.io/crates/terminal-colorsaurus | T22.6: OSC 11 background colour query for `tui.theme = "auto"` Lives in `cox-render` (T32.2). |
| tempfile | local | https://crates.io/crates/tempfile | Rust dependency |
| thiserror | local | https://crates.io/crates/thiserror | Error enums |
| tiktoken-rs | local | https://crates.io/crates/tiktoken-rs | Rust dependency |
| tokio | local | https://crates.io/crates/tokio | cox-core |
| tokio-util | local | https://crates.io/crates/tokio-util | Rust dependency |
| toml_edit | local | https://crates.io/crates/toml_edit | Rust dependency |
| tracing | local | https://crates.io/crates/tracing | Logging |
| tracing-appender | local | https://crates.io/crates/tracing-appender | Rust dependency |
| tracing-opentelemetry | local | https://crates.io/crates/tracing-opentelemetry | Rust dependency |
| tracing-subscriber | local | https://crates.io/crates/tracing-subscriber | Rust dependency |
| tree-sitter | local | https://crates.io/crates/tree-sitter | Rust dependency |
| tree-sitter-bash | local | https://crates.io/crates/tree-sitter-bash | Rust dependency |
| tree-sitter-go | local | https://crates.io/crates/tree-sitter-go | Rust dependency |
| tree-sitter-python | local | https://crates.io/crates/tree-sitter-python | Rust dependency |
| tree-sitter-rust | local | https://crates.io/crates/tree-sitter-rust | Rust dependency |
| tree-sitter-typescript | local | https://crates.io/crates/tree-sitter-typescript | Rust dependency |
| tui-textarea-2 | local | https://crates.io/crates/tui-textarea-2 | Rust dependency |
| typify | local (build-dependency) | https://github.com/oxidecomputer/typify | cox-provider `build.rs`: Anthropic request and stream types generated from the vendored `schema/anthropic-openapi.json` (T30.10, T30.12) |
| ulid | local | https://crates.io/crates/ulid | Identifiers |
| unicode-width | local | https://crates.io/crates/unicode-width | Rust dependency |
| vt100 | local | https://crates.io/crates/vt100 | Rust dependency |
| wasmtime | local (`anyhow` feature only) | https://github.com/bytecodealliance/wasmtime | cox-plugin: the runtime under extism; declared only to enable the `anyhow` feature extism 1.30.0 needs with its default features off (T33.3) |
| wiremock | local | https://crates.io/crates/wiremock | Rust dependency |

## uv (`evals/`)

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| pyyaml | local | https://github.com/yaml/pyyaml | Reads `evals/tasks/*.yaml` |
| tomli-w | local | https://github.com/hukkin/tomli-w | Writes scripted scenarios and the verify hook config |
| pytest | local (dev) | https://github.com/pytest-dev/pytest | Tests for the eval package |
| harbor | local (extra `tbench`) | https://github.com/laude-institute/harbor | Terminal-Bench 2.0 harness; `cox_evals.tbench:CoxAgent` is a Harbor agent |

## uv (`scripts/vendor/`)

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| pytest | local (dev) | https://github.com/pytest-dev/pytest | Tests for the vendor package |
| tomlkit | local | https://github.com/sdispater/tomlkit | Comment-preserving TOML edits for `cox-vendor models` (T30.20) |
