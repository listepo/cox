# bench-fixture

A tiny widget library used as the replay workspace for the token-economy
bench (`evals/token`). It is deliberately boring: a `Canvas` of `Widget`s
plus an `auth` module with login helpers.

## Layout

- `src/lib.rs` — `Widget` and `Canvas`
- `src/auth.rs` — `authenticate`, `is_valid`, `refresh`
- `src/main.rs` — example binary
- `data/big.rs` — generated large file for truncation tests
