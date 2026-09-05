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

# Optimized single-binary build (fat LTO, one codegen unit, stripped) and its
# size. Same profile cargo-dist ships, so what you measure is what users get.
# Ask cargo for the target dir rather than assuming ./target — a shared
# build.target-dir in ~/.cargo/config.toml moves it.
release:
    mise exec -- cargo build --profile dist -p cox
    @ls -lh "$(mise exec -- cargo metadata --format-version 1 --no-deps | tr ',' '\n' | grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4)/dist/cox" | awk '{print "cox  " $5}'

# $CARGO_HOME sizes (no deletes) and ./target
cache:
    mise exec -- cargo-cache
    du -sh target 2>/dev/null || echo "target: (missing)"

# drop extracted crate/git checkouts; keep archives
cache-autoclean:
    mise exec -- cargo-cache --autoclean
