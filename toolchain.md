# Toolchain

Программы проекта и прямые пакеты из манифестов.

## Программы

| Программа | Как ставить | Зачем здесь | Источник |
| --- | --- | --- | --- |
| mise | brew / curl, затем `mise install` | Пины версий инструментов | https://github.com/jdx/mise |
| cargo-cache | mise | `just cache` / `just cache-autoclean`; the shared cargo home fills up | https://github.com/matthiaskrgr/cargo-cache |
| rust | mise | Компилятор и std | https://github.com/rust-lang/rust |
| rustc | mise (pin rust) | Компилятор Rust | https://github.com/rust-lang/rust |
| cargo | mise (pin rust) | Сборка и зависимости Rust | https://github.com/rust-lang/cargo |
| cargo-nextest | global | https://github.com/nextest-rs/nextest | Параллельный прогон тестов |
| just | cargo install just / brew | Рецепты команд | https://github.com/casey/just |

## cargo

| Пакет | Где | Источник | Зачем здесь |
| --- | --- | --- | --- |
| agent-client-protocol | локально | https://crates.io/crates/agent-client-protocol | cox-acp |
| anyhow | локально | https://crates.io/crates/anyhow | Ошибки CLI |
| arboard | локально | https://crates.io/crates/arboard | Зависимость Rust |
| assert_cmd | локально | https://crates.io/crates/assert_cmd | Зависимость Rust |
| assert_fs | локально | https://crates.io/crates/assert_fs | Зависимость Rust |
| async-trait | локально | https://crates.io/crates/async-trait | Зависимость Rust |
| bytes | локально | https://crates.io/crates/bytes | T1.2: turns a reqwest byte stream into SSE frames (`sse.rs`) and drives the in-memory fixture parser (`parse_sse_str`) through the same code path. |
| clap | локально | https://crates.io/crates/clap | cox (CLI) |
| crossterm | локально | https://crates.io/crates/crossterm | Терминал |
| diesel | локально | https://crates.io/crates/diesel | cox-store diesel/diesel_migrations pinned "2.2" per plan.md D9 resolve to the latest 2.x compatible release (2.3.x) on crates.io as of 2026-09-02; verified no semver-breaking API change vs. 2.2 for the sqlite backend used here. |
| diesel_migrations | локально | https://crates.io/crates/diesel_migrations | Миграции SQLite |
| diffy | локально | https://crates.io/crates/diffy | Зависимость Rust |
| directories | локально | https://crates.io/crates/directories | Зависимость Rust |
| dotenvy | локально | https://crates.io/crates/dotenvy | T0.7: load local .env files without overriding the process environment. |
| eventsource-stream | локально | https://crates.io/crates/eventsource-stream | Зависимость Rust |
| figment | локально | https://crates.io/crates/figment | Конфиг |
| futures | локально | https://crates.io/crates/futures | Зависимость Rust |
| globset | локально | https://crates.io/crates/globset | Зависимость Rust |
| grep-regex | локально | https://crates.io/crates/grep-regex | T3.3: plan.md names "grep-regex + grep-searcher sinks"; grep-regex (the RegexMatcher grep-searcher needs) was missing from this list. |
| grep-searcher | локально | https://crates.io/crates/grep-searcher | Зависимость Rust |
| ignore | локально | https://crates.io/crates/ignore | cox-tools |
| insta | локально | https://crates.io/crates/insta | dev-deps |
| keyring | локально | https://crates.io/crates/keyring | Зависимость Rust |
| landlock | локально | https://crates.io/crates/landlock | Зависимость Rust |
| libfuzzer-sys | локально | https://crates.io/crates/libfuzzer-sys | Зависимость Rust |
| libsqlite3-sys | локально | https://crates.io/crates/libsqlite3-sys | Зависимость Rust |
| nix | локально | https://crates.io/crates/nix | Зависимость Rust |
| nucleo | локально | https://crates.io/crates/nucleo | Зависимость Rust |
| opentelemetry | локально | https://crates.io/crates/opentelemetry | D16/T13: OTLP/HTTP keeps telemetry vendor-neutral (SigNoz, Jaeger, Grafana/Tempo and hosted collectors) without adding a backend SDK. |
| opentelemetry-appender-tracing | локально | https://crates.io/crates/opentelemetry-appender-tracing | Зависимость Rust |
| opentelemetry-otlp | локально | https://crates.io/crates/opentelemetry-otlp | Зависимость Rust |
| opentelemetry_sdk | локально | https://crates.io/crates/opentelemetry_sdk | Зависимость Rust |
| pathdiff | локально | https://crates.io/crates/pathdiff | Относительный путь между двумя путями |
| portable-pty | локально | https://crates.io/crates/portable-pty | Зависимость Rust |
| predicates | локально | https://crates.io/crates/predicates | Зависимость Rust |
| pretty_assertions | локально | https://crates.io/crates/pretty_assertions | Зависимость Rust |
| proptest | локально | https://crates.io/crates/proptest | Зависимость Rust |
| pulldown-cmark | локально | https://crates.io/crates/pulldown-cmark | T5.3: plan.md says pulldown-cmark 0.10; 0.13 is the current line with the same Tag/TagEnd API. syntect without onig (pure-Rust fancy-regex engine). |
| ratatui | локально | https://crates.io/crates/ratatui | cox-tui |
| reqwest | локально | https://crates.io/crates/reqwest | cox-provider |
| rmcp | локально | https://crates.io/crates/rmcp | cox-mcp |
| rstest | локально | https://crates.io/crates/rstest | Зависимость Rust |
| schemars | локально | https://crates.io/crates/schemars | Зависимость Rust |
| seccompiler | локально | https://crates.io/crates/seccompiler | Зависимость Rust |
| serde | локально | https://crates.io/crates/serde | cox-protocol |
| serde_json | локально | https://crates.io/crates/serde_json | preserve_order: agent-client-protocol-schema requires it, which unifies the feature into every workspace build anyway; pinning it here makes JSON key order (snapshots, rollout lines, ledger JSON) identical for per-crate and full-workspace runs instead of depending on the invocation. |
| serde_yaml | локально | https://crates.io/crates/serde_yaml | cox-ext |
| sha2 | локально | https://crates.io/crates/sha2 | Зависимость Rust |
| shlex | локально | https://crates.io/crates/shlex | Зависимость Rust |
| similar | локально | https://crates.io/crates/similar | Зависимость Rust |
| syntect | локально | https://crates.io/crates/syntect | Зависимость Rust |
| tempfile | локально | https://crates.io/crates/tempfile | Зависимость Rust |
| thiserror | локально | https://crates.io/crates/thiserror | Ошибки |
| tiktoken-rs | локально | https://crates.io/crates/tiktoken-rs | Зависимость Rust |
| tokio | локально | https://crates.io/crates/tokio | cox-core |
| tokio-util | локально | https://crates.io/crates/tokio-util | Зависимость Rust |
| toml_edit | локально | https://crates.io/crates/toml_edit | Зависимость Rust |
| tracing | локально | https://crates.io/crates/tracing | Логи |
| tracing-appender | локально | https://crates.io/crates/tracing-appender | Зависимость Rust |
| tracing-opentelemetry | локально | https://crates.io/crates/tracing-opentelemetry | Зависимость Rust |
| tracing-subscriber | локально | https://crates.io/crates/tracing-subscriber | Зависимость Rust |
| tree-sitter | локально | https://crates.io/crates/tree-sitter | Зависимость Rust |
| tree-sitter-bash | локально | https://crates.io/crates/tree-sitter-bash | Зависимость Rust |
| tree-sitter-go | локально | https://crates.io/crates/tree-sitter-go | Зависимость Rust |
| tree-sitter-python | локально | https://crates.io/crates/tree-sitter-python | Зависимость Rust |
| tree-sitter-rust | локально | https://crates.io/crates/tree-sitter-rust | Зависимость Rust |
| tree-sitter-typescript | локально | https://crates.io/crates/tree-sitter-typescript | Зависимость Rust |
| tui-textarea-2 | локально | https://crates.io/crates/tui-textarea-2 | Зависимость Rust |
| ulid | локально | https://crates.io/crates/ulid | Идентификаторы |
| unicode-width | локально | https://crates.io/crates/unicode-width | Зависимость Rust |
| vt100 | локально | https://crates.io/crates/vt100 | Зависимость Rust |
| wiremock | локально | https://crates.io/crates/wiremock | Зависимость Rust |
