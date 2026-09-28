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

## Plugins (after P33 ships)

- Publish `cox-plugin-api` / `cox-plugin-sdk` / a Go module, once the ABI is stable.
- Plugin install from git or a URL (v1 installs from a local folder only, `docs/design/plugins.md` §1).

## Desktop (after P37 ships)

- M2: integrated terminal pane (SwiftTerm), browser preview pane the agent can screenshot and read, pop-out session windows and native tabs, menu-bar extra with running sessions and waiting approvals (global hotkey via KeyboardShortcuts, `research.md` §9.5.7), Spotlight and App Intents, per-hunk revert (`docs/design/desktop.md` DT§3.2)
- M3: ACP host for Claude Code, Codex, Gemini CLI and Cursor sessions in the same sidebar (moves into `plan.md` as soon as a planned card is blocked by it, A67), best-of-n across models in worktrees, plugin panels from the `Widget` tree, remote sessions over SSH through `cox app-server` (DT§3.3)
- Glass theme in dark appearance: the dark values in `desktop/design/tokens/color.dark.json` for glass surfaces are not mocked yet
