# Lint fixtures

Swift files that prove the custom rules in `../.swiftlint.yml` (T37.18, DS§9). They belong to
no package, so no build compiles or lints them; CI's `desktop-macos-lint` job lints them by path.

- `Accepted/` must pass `swiftlint lint --strict`: token-only views, a literal zero, literals
  inside comments and strings, and literals under `Tokens/` and `Foundations/`.
- `Rejected/<rule>.swift` must fail with the custom rule its file name gives.
- Both must pass `swift-format lint --strict`.
