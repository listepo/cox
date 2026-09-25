# cox — task runner. Every target runs through `mise exec` so the pinned
# toolchain (mise.toml) is used, never whatever `cargo` happens to be on PATH.

check:
    bash scripts/leftovers.sh
    mise exec -- cargo fmt --check
    mise exec -- cargo clippy --workspace --all-targets -- -D warnings

test: && dunnage
    mise exec -- cargo nextest run --workspace

# Lossless cleanup of ./target (compress + dedupe); never deletes. A no-op without dunnage.
dunnage:
    #!/usr/bin/env sh
    command -v dunnage >/dev/null || { echo "dunnage not found; install it with: ketch install dunnage"; exit 0; }
    [ -d target ] || exit 0
    dunnage run target || test $? -eq 2

snap:
    mise exec -- cargo insta review

eval *args:
    uv run --project evals cox-evals {{args}}

# Tests for the eval package; no network, no key (the e2e ones need `cargo build -p cox`).
test-evals:
    uv run --project evals pytest evals/tests -q

bench:
    mise exec -- cargo run -q -p cox --example bench

# Footprint benchmark (plan.md T30.2): cold start, first frame, replay RSS
# peak and binary size. `--write` refreshes scripts/footprint.json (commit
# the result); `--check` fails on a >20% regression vs the baseline (CI).
footprint *args:
    bash scripts/footprint.sh {{args}}

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

# Re-render docs/screenshots/*.svg from the whole-screen snapshot tests
# (crates/cox-tui/tests/screenshots.rs); the same frames insta compares.
screenshots:
    COX_SCREENSHOTS="{{justfile_directory()}}/docs/screenshots" mise exec -- cargo test -p cox-tui --test screenshots
