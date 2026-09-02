# cox — task runner. Every target runs through `mise exec` so the pinned
# toolchain (mise.toml) is used, never whatever `cargo` happens to be on PATH.

check:
    mise exec -- cargo fmt --check
    mise exec -- cargo clippy --workspace --all-targets -- -D warnings

test:
    mise exec -- cargo nextest run --workspace

snap:
    mise exec -- cargo insta review

eval:
    echo "not yet"

bench:
    echo "not yet"
