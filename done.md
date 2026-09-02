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
