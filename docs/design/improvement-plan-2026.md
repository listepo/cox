//! Rationale for plan.md P22–P30 (A26 proposed, A27 approved): why these phases,
//! the pitch, what is deliberately not proposed, and the falsifiers. The task
//! cards themselves live in plan.md §3 — this file never duplicates them.

# Improvement plan 2026 — the surface a 2026 user expects, on the core cox already has

Status: **approved 2026-09-22 (plan.md A27); tasks live in `plan.md` §3 P22–P30** · Evidence: `research.md` §8 (field survey 2026-09-22, cox column verified in code) · Decisions D1–D16 unchanged.

## 1. Where cox stands

The core is ahead of the field: lossless archive + `expand`, read dedup, deferred tools, explicit tier routing that never goes up, a usage row per request, `cox mcp`, `cox acp`, any provider including local, sandbox on by default. No vendor ships all of that; most ship none of the first four.

The surface is a generation behind, and some of it is silently broken (research §8.5 #32):

| Found in code | Effect on a user |
|---|---|
| `ask_user` in the TUI is `Answers::Fixed` | the model cannot ask a question; the answer is canned |
| `tui.mouse = true` is read, nothing enables mouse capture | a documented key does nothing |
| `tui.theme = "auto"` is `dark` unless the string is `light` | light terminals get a dark syntax theme |
| skills index and `.claude/commands` reach only `cox ext list` | skills never enter `system[2]`; `/name` commands are not in the palette |
| `SessionStart` and `Notification` hooks never fire | a Claude Code hook set imported per D4 is partly dead |
| MCP OAuth is a header comment; a 401 server is skipped | HTTP servers with auth silently vanish |
| no checkpoint, no message queue, no themes, no notifications, no keybindings, no plan-mode view | the 2026 table stakes in research §8.1 |

## 2. Principles

1. **Trust before beauty.** P22 closes every documented-but-dead key first; a config key that does nothing is a bug (D13 "every flag is a key").
2. **Copy conventions verbatim, as D4 does for file formats.** `Shift+Tab` for mode, `/rewind`, `/fork`, `/context`, `!` for shell, `?` for help, `Ctrl+Enter` send-now. A user coming from Claude Code or Codex must not relearn keys.
3. **Win where the leaders have known gaps**: rewind that also tracks shell-caused changes (Claude Code does not), approvals labelled by source agent (Codex complaint), a measured RSS and startup time (OpenCode complaint), a ≤ 1 000-token minimal profile (Pi's lesson), unconditional secret redaction of transcripts (Warp).
4. **Terminal features degrade to nothing, never to garbage.** One capability probe (`cox_tui::term::Caps`) decides Kitty keys, OSC 8/9/52, images; `cox doctor` prints it.
5. **Every state has a snapshot** (D10, D12). A task that adds a screen adds its `insta` frame and its `docs/screenshots` SVG.
6. **Task discipline unchanged**: ≤ 200 LOC, ≤ 3 files, a Check, `done.md` on close. New dependencies listed in §7 need creator approval and a §1.1 row.

## 3. Phases

The 48 task cards (T22.1–T30.3) with steps, files, checks, priority and complexity are in `plan.md` §3 (phases P22–P30) and its top table; `todo.md` mirrors the ids. Read them there — this section only records the shape so the rationale below has context.

| Phase | Goal | Priority | Tasks |
|---|---|---|---|
| P22 Trust | nothing documented is a no-op | P0 | 7 |
| P23 Terminal capabilities | use what the terminal offers, silently degrade | P1 | 8 |
| P24 Looks | themes, cards, diffs, help — the "beautiful" half | P0/P1 | 8 |
| P25 Composer and flow | queue, send-now, `Shift+Tab`, `!`, vim, keybindings, `/init`, `/context` | P0/P1 | 8 |
| P26 Checkpoints and rewind | `/rewind` incl. shell-caused changes, `/fork`, `/handoff` | P0 | 4 |
| P27 Agents you can see | `bash` tasks, `Ctrl+B`, labelled approvals, worktrees, `/loop` | P1/P2 | 4 |
| P28 Context and cost visibility | status segments, project cost, pre-emptive compaction, redaction | P1 | 4 |
| P29 Accessibility | `--plain` screen-reader surface, reduced motion, daltonized themes | P2 | 2 |
| P30 Lean profile and footprint | ≤ 1 000-token prefix profile, published RSS/startup, eval run | P2 | 3 |

Critical path and the dependency order are in `plan.md` §3.0.

## 4. Sizing summary

Sum of complexity (1–5, from the `plan.md` table): P0 13 tasks / 37; P1 21 tasks / 45; P2 12 tasks / 24; P3 2 tasks / 3. With the ≤ 200 LOC rule, P0 is about 13 commits; a v0.2 that ships P0 plus T23.0–T23.3 and T24.3–T24.6 is the "competitive TUI" milestone (M6 in `plan.md` §5).

## 5. What cox will be able to say afterwards (the pitch)

- The only agent whose rewind also covers what the shell changed.
- Approvals that say which agent is asking; agents you can watch and background with one key.
- Every token accounted for: `/context` shows where the prompt goes, the status line shows what is cached, the ledger shows what it cost — and the model never changes without you.
- Themes from files, syntax themes from any `.tmTheme`, a system theme derived from your terminal.
- One static binary with a published startup time and RSS.
- Talks to Zed and JetBrains (ACP), lends its tools to Claude Code and Codex (`cox mcp`), runs any provider including local.

## 6. Deliberately not proposed

| Item | Why not now |
|---|---|
| Voice input | needs an audio dependency and a speech provider; no D-decision covers it; propose after v0.2 |
| Remote control / cloud sessions / phone app | a server surface and an account system; contradicts the single-binary, local-first shape until a hosted story exists |
| Consumer-subscription OAuth for Anthropic | policy (research §8.2, ledger #34); API keys + keyring remain |
| Windows sandbox | D7 keeps the loud warning; Codex's restricted-token approach is a v0.3 scope gate, not a task |
| Agent teams with a shared task file, orchestration DSL | valuable, but each is a phase of its own after P27 proves background agents |
| MCP Apps (`ui://` resources), elicitation | after T22.5 OAuth; elicitation maps onto T22.1's question modal — a follow-up card |
| Repo map, LSP diagnostics, images, WASM | already gated (T19.1–T19.6); P26/P27 do not depend on them |

## 7. Dependencies that need approval

| Crate / feature | Task | Why | Alternative |
|---|---|---|---|
| ratatui feature `scrolling-regions` | T23.2 | flicker-free `insert_before` | none (feature flag, no new crate) |
| crossterm feature `osc52` | T23.4 | clipboard over SSH | `arboard` (native only) |
| `terminal-colorsaurus` (or `termbg`) | T22.6 | OSC 11 background query with timeout | hand-rolled query (~80 LOC, tmux edge cases) |
| `two-face` | T24.3 | 250-language syntax set for syntect | stay at ~40 languages |
| `similar` word diff | T24.5 | already a workspace dependency (`Cargo.toml` line 69) — no approval needed | — |
| `ratatui-image` | images gate T19.4 only | not needed by this plan | — |

## 8. Falsifiers (what would change this plan)

- If the PTY flicker test (T23.2) shows no repaint reduction on Ghostty/Kitty/WezTerm/tmux, drop the feature and keep the current renderer.
- If T26.1's bash pre/post hashing costs > 200 ms on a 50 k-file workspace, fall back to git-tracked files only and say so in the card.
- If the minimal profile (T30.1) does not measurably raise the cache-read ratio in `just bench`, keep it as a config example, not a first-class profile.
