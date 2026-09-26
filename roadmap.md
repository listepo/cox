# Roadmap

Approved work that is not yet in the active plan.

## v0.2

- LSP diagnostics
- Gemini
- images
- worktrees
- repo map
- architect/editor mode
- ~~TypeSafe Jev decision model (System One: Choice/Score/Noul) — scope gate T21.0~~
- T30.13 cox vs Claude Code vs Terminus 2 on the same local model — moved from `plan.md` by the creator on 2026-09-26 (the run was stopped part-way; uncommitted `evals/` work is kept in `_worktrees/cox-t30.13`). The full card (plan, Check, Done when) is in `plan.md` at commit 855fe68; it goes back unchanged when this is scheduled.

## Plugins (after P33 ships)

- Publish `cox-plugin-api` / `cox-plugin-sdk` / a Go module, once the ABI is stable.
- Plugin install from git or a URL (v1 installs from a local folder only, `docs/design/plugins.md` §1).

## After the optimization and refactoring pass

- Re-run T30.13 (cox vs Claude Code vs Terminus 2, same 12 Terminal-Bench 2.0 tasks, same local model) and compare with its baseline in `research.md` §5.3. The pass itself is still to be described by the creator.
