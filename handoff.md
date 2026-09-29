# Handoff: cox desktop work (for the next agent, Cursor)

State as of 2026-09-29. Read `AGENTS.md`, then `plan.md` (§0 decisions, the task table, A127–A130). This file only says where things stand and how the previous agent worked; `plan.md` stays the source of truth.

## Where the work lives

- Feature branch `p37-desktop`, PR https://github.com/listepo/cox/pull/65. Every task branch merges into `p37-desktop`; the creator merges the PR. Do not merge the PR, enable auto-merge, publish, or tag.
- Worktrees: one per task via the `worktrees` skill (`wt.sh new <task-id>`, `wt.sh done <path>`, `wt.sh clean <path>`) under `_worktrees/cox-<task-id>`. Never `rm -rf` a worktree; never touch a worktree locked by another owner (for example `rtok-*`).
- Git in these worktrees hangs on fsmonitor. Prefix every git command with `export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.fsmonitor GIT_CONFIG_VALUE_0=false;`.
- Disk is tight (a worktree's `target/` is 15–40 GB). Run `wt.sh clean` on a worktree as soon as you stop building in it; never share one cargo target dir between worktrees.

## How a task is claimed and closed

1. Claim: in `p37-desktop`, set the table row to `in progress` with your provider and model, write the execution plan into the card, sync `todo.md`, commit `<id>: claim`, push. Then `wt.sh new <id>` and `git merge --ff-only p37-desktop` in the new worktree.
2. Work in the worktree, one commit per card: `<id>: <title>`. No `Co-Authored-By`, no "Generated with" line — the creator is the only author.
3. Verify only what the change can break (other agents share the machine):
   - `CARGO_BUILD_JOBS=4 mise exec -- cargo nextest run -p <crates>` plus the card's Check;
   - `mise exec -- cargo clippy -p <crates> --all-targets -- -D warnings`, `mise exec -- cargo fmt --check`;
   - `swift test` in each touched package under `desktop/macos/Packages/`;
   - `just desktop-app` when Swift or `cox-ffi` changed.
4. Close: merge the branch into `p37-desktop`, re-run the checks there, move the card entirely from `plan.md` (row and card) and `todo.md` into `done.md` with Result / Deviations / Check / Not done, commit `<ids>: close`, push, add one line to the PR body, then `wt.sh done` and delete the branch.

## Open work

### Running when this was written (Claude Code / opus-5.5)

T58.4.4–6, T58.4.14–15 (sidebar filter, short model names), T58.4.20–25 (inspector and tasks logic into `cox-app`), T58.4.26–28 (Markdown writer into `cox-render`), T52.23.1–2 (plugin `tool:`/`item:` renderers). If a row still says `in progress` with that agent and no commits land for a long time, ask the creator before taking it over.

### Waiting on the creator

- T51.23 is merged and verified (CoxUI 238/238) and sits at 95%. One question: on glass, the pane tint is fixed at `glass.fill`'s alpha and the transparency slider moves only the window tint. Should the slider scale the panes again? Close the card or adjust it after the answer.
- T37.29.3.4 (budget cap source), T37.32.2 (Developer ID, notarization, Sparkle — needs the creator's certificates), T53.5 (ABI freeze confirmation), T53.9 (the creator publishes the SDKs), T56.x (Cursor Cloud terms go-ahead, A123), T35.10 and T39.7 (need the creator's own API keys).

### Waiting on an upstream release

- T33.43: an extism release after v1.30.0 that pins wasmtime ≥ 48. It unblocks T33.14 → T33.18, T33.34, T33.36, T33.40.x and then T53.x. The bump itself needs the creator's permission.

### Windows (P57/P58 except T58.4)

Built and tested only by an agent on a Windows host (A130). The CI `windows` job stays `if: false` until that agent has verified the work; never enable it from macOS. T57.1/T57.7 carry "Remaining (A130)" lines for that host.

### Skipped by rule

Benchmarks and measurement cards (T37.33, T43.6) — the creator asked not to take them.

## Rules the previous agent followed

- No version bumps, new dependency only if maintained (reason in the commit, row in `toolchain.md` and `plan.md` §1).
- Tests never touch the real keychain; dev runs of the binary use a scratch `COX_HOME`.
- UI: tokens and components from CoxUI; a visual change re-records the affected snapshots and, for a mockup, re-renders it (`desktop/design/mockups`). Figma writes go only to file `KA9a0R7n6P0QbwDn92e167`.
- Never kill the app by name (`pkill -f Cox` hits other sessions); quit it by PID.
- Anything that needs the creator's decision: stop and ask, do not guess.
