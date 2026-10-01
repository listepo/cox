# Ratatui CI dashboard example

A small standalone [Ratatui](https://ratatui.rs) TUI: a scrollable table of recent
pull requests and their CI runs, one row per PR with its number, a green/red
status and its title. The data is mock (hard-coded in `src/main.rs`); nothing
talks to GitHub.

The crate is a workspace of its own, so the root `cargo` commands never build it.

## Run

From the repository root:

```sh
cargo run --manifest-path examples/ratatui-ci-dashboard/Cargo.toml
```

## Keys

| Key               | Action      |
| ----------------- | ----------- |
| `Up` / `k`        | scroll up   |
| `Down` / `j`      | scroll down |
| `q` / `Esc`       | quit        |
