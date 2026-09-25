

#### T30.5 Price every provider call

Model: claude-opus-5-5 · Status: done 2026-09-25 · Blocks: T30.3 · Size: ~90 · Priority: P0 · Complexity: 2
Goal: every `usage` row carries the cost from `prices.toml`. Today no production path calls `usage::ledger_row` (only its tests do), so every row is written with `cost_usd = 0`: `cox stats` reads $0, budget caps never fire, and T30.3 has no cost to record. Found by the T30.4 live check (8 317 cache-write tokens, `cost_usd: 0.0`).
Files: `crates/cox-provider/src/usage.rs`, `crates/cox/src/session.rs`.
Plan: (1) `PriceTable::apply(&model, &mut Usage)` — the priced/unknown rule `ledger_row` already has, extracted so both share it; (2) `usage::Priced`, a `Provider` decorator: forwards every `ProviderEvent`, pricing the `Usage` event and the returned `Usage` by `req.model`, so the five core call sites (turn, compaction, memory, init, subagent) get cost without touching them; (3) `session::provider_for` wraps every real provider in `Priced` with `PriceTable::load(<COX_HOME>/prices.toml)` (embedded table when absent); test doubles stay unwrapped, their scenarios script their own cost; (4) regression test: a scripted inner provider through `Priced` yields a non-zero cost on both the event and the return value, and an unknown model is `estimated`; (5) live: `cox run -p "say hi"` shows a non-zero `cost_usd` and `cox stats` agrees.
Check:
```bash
mise exec -- cargo nextest run -p cox-provider usage
```
Done when: the live run's `cost_usd` is non-zero and matches `cox stats`.
What landed (`1a0bc87`): `PriceTable::apply` (the priced/unknown rule, now shared by `ledger_row`) and `PriceTable::embedded`; `usage::Priced`, a `Provider` decorator that forwards every event and prices both the `Usage` event and the returned `Usage` by `req.model`; `session::provider_for` wraps every real client in `Priced` (the match moved to `backend_for`), test doubles stay unwrapped. Tests `priced_provider_costs_both_the_event_and_the_return`, `priced_provider_flags_an_unknown_model_as_estimated`.
Deviations: prices come from the embedded table only; the card's `<COX_HOME>/prices.toml` override was dropped as unrequested (no doc or task names it). No wiring test in `crates/cox`: `provider_for` returns `Arc<dyn Provider>` and a real client needs a key or a server, so the wiring is proven by the live run below.
Check:
```text
$ mise exec -- cargo nextest run -p cox-provider usage
     Summary [ 0.017s] 10 tests run: 10 passed, 89 skipped
$ COX_HOME=<scratch> cox run -p "say hi" --output-format json --provider anthropic --tier code=claude-sonnet-5
{... "usage":{"input_tokens":2,"output_tokens":18,"cache_read_tokens":0,"cache_write_tokens":8317},"cost_usd":0.0209765, ... "exit_code":0}
$ COX_HOME=<scratch> cox stats
all code main 1 2 0 $0.0210
$ mise exec -- cargo nextest run --workspace
     Summary [ 44.427s] 865 tests run: 865 passed, 3 skipped
$ mise exec -- cargo clippy --workspace --all-targets -- -D warnings
     clean
$ mise exec -- cargo fmt --check
     clean
```
