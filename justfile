# cox — task runner. Every target runs through `mise exec` so the pinned
# toolchain (mise.toml) is used, never whatever `cargo` happens to be on PATH.

check:
    mise exec -- cargo fmt --check
    mise exec -- cargo clippy --workspace --all-targets -- -D warnings

test:
    mise exec -- cargo nextest run --workspace

snap:
    mise exec -- cargo insta review

eval *args:
    mise exec -- python3 evals/run.py {{args}}

bench:
    mise exec -- cargo run -q -p cox --example bench
