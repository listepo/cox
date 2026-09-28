# cox — task runner. Every target runs through `mise exec` so the pinned
# toolchain (mise.toml) is used, never whatever `cargo` happens to be on PATH.

check:
    bash scripts/leftovers.sh
    mise exec -- cargo fmt --check
    mise exec -- cargo clippy --workspace --all-targets -- -D warnings

# Only the tests a change can break (A99): the crates that own the files
# changed since REF, and every crate depending on them. REF defaults to the
# merge-base with origin/main; uncommitted and untracked files count. A
# Cargo.toml/Cargo.lock/.cargo/mise.toml/justfile change runs everything.
# `just test --changed-since HEAD`, `just test --dry-run` (print the command),
# other flags go to nextest.
[positional-arguments]
test *args:
    @uv run --no-project python scripts/changed_tests.py "$@"

# The whole workspace, then the dunnage cleanup; CI runs the same suite.
check-all: && dunnage
    mise exec -- cargo nextest run --workspace

# The guest workspace (plugins/, PL§9): pure/host-target tests only — no
# wasm32 build here, that is CI's separate step (T33.40.2).
plugin-test:
    mise exec -- cargo test --manifest-path plugins/Cargo.toml --workspace

# Build a guest-language example (PL§13) with its own toolchain (installed
# from plugins/mise.toml) and run its ignored e2e test, e.g.
# `just plugin-examples dart`. The `plugin-examples` CI job runs this for
# every language; a missing toolchain there fails the job.
plugin-examples lang:
    #!/usr/bin/env sh
    set -eu
    case "{{lang}}" in
      dart)
        cd plugins && mise install dart@3.13.4
        cd examples/dart
        mise exec -- dart pub get
        mkdir -p build
        mise exec -- dart compile exe bin/server.dart -o build/example_dart
        ;;
      *)
        echo "plugin-examples: no {{lang}} example yet" >&2
        exit 1
        ;;
    esac
    cd "{{justfile_directory()}}"
    mise exec -- cargo nextest run -p cox --run-ignored only -E 'test(plugin_example_{{lang}})'

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
    uv run --project evals --extra tbench pytest evals/tests -q

# Vendor a file no package manager fetches (plan.md A48), e.g.
# `just vendor anthropic-spec` or `just vendor anthropic-spec --check`.
vendor *ARGS:
    uv run --project scripts/vendor cox-vendor {{ARGS}}

# Tests for the vendor package; no network.
vendor-test:
    uv run --project scripts/vendor pytest scripts/vendor/tests -q

# Token economy (R§4.6), then the PL§11 plugin timings over the Rust
# reference plugin (R§4.7; release, because they are latencies; needs the
# wasm32 target from mise.toml).
bench:
    mise exec -- cargo run -q -p cox --example bench
    mise exec -- cargo run -q --release -p cox --example plugin_bench

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

# The macOS app's Rust core (T37.15, DT§7): desktop/macos/build/CoxFFI.xcframework
# (aarch64-apple-darwin only, macOS 26.0) plus the generated Swift bindings.
desktop-xcframework:
    mise exec -- bash scripts/desktop/xcframework.sh

# The macOS app, Debug and ad-hoc signed (T37.32.1, DT§7): the XCFramework, then XcodeGen's
# thin Cox.xcodeproj from desktop/macos/project.yml, then desktop/macos/build/Cox.app. No
# signing identity; Developer ID and notarization are T37.32.2. Run it on a fixture with
# `desktop/macos/build/Cox.app/Contents/MacOS/Cox -CoxFixture desktop/macos/Fixtures/edit.json`.
desktop-app: desktop-xcframework
    mise exec -- bash scripts/desktop/app.sh

# The desktop design tokens (T37.17, DS§2): Style Dictionary regenerates CoxUI's
# Tokens.swift and Colors.xcassets and the mockups' tokens.css from
# desktop/design/tokens/*.json. CI runs the same build and fails on any diff.
desktop-tokens:
    cd desktop/design && mise exec -- npm ci --no-fund --no-audit && mise exec -- npm run build

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
