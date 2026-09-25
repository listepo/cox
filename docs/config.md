# cox configuration reference

Generated from `config/default.toml` by a test in `cox-protocol/src/config.rs`; do not hand-edit.

## `[core]`

- `home` = `"~/.cox"` — COX_HOME overrides
- `workspace_roots` = `[]` — empty = git root of cwd, else cwd; extra roots via --add-dir
- `max_turns` = `200` — per UserTurn, counts provider calls
- `parallel_tools` = `4`
- `log_level` = `"info"` — tracing filter; file log at ~/.cox/logs/cox.log
- `profile` = `""` — "" (default) | "minimal" (T30.1: the lean prefix); also `cox --profile minimal`
## `[tiers.cheap]`

- `provider` = `"anthropic"`
- `model` = `"claude-haiku-4-5"`
- `effort` = `"low"`
- `max_tokens` = `4096`
## `[tiers.code]`

- `provider` = `"anthropic"`
- `model` = `"claude-sonnet-5"`
- `effort` = `"high"`
- `max_tokens` = `16384`
- `thinking` = `"adaptive"`
## `[tiers.think]`

- `provider` = `"anthropic"`
- `model` = `"claude-fable-5-1"`
- `effort` = `"high"`
- `max_tokens` = `32768`
- `thinking` = `"adaptive"`
- `confirm` = `true` — cannot be set false in project config
## `[jobs]`

- `main` = `"code"`
- `plan` = `"think"`
- `compact` = `"cheap"`
- `title` = `"cheap"`
- `summarize` = `"cheap"`
- `commit` = `"cheap"`
- `memory` = `"cheap"`
- `explore` = `"cheap"`
- `shell` = `"cheap"`
- `hook` = `"cheap"`
## `[providers.anthropic]`

- `base_url` = `"https://api.anthropic.com"`
- `api_key_env` = `"ANTHROPIC_API_KEY"` — else keyring entry "cox/anthropic"
- `cache_ttl` = `"5m"` — "5m" | "1h"
- `fallbacks` = `true` — fallbacks: "default" + beta header
- `timeout_s` = `120`
- `max_retries` = `4`
- `models` = `[{id="claude-haiku-4-5", context_window=200000, efforts=["low"]}, {id="claude-sonnet-5", context_window=1000000, efforts=["low", "high"]}, {id="claude-opus-5", context_window=1000000, efforts=["high", "xhigh"]}, {id="claude-fable-5-1", context_window=1000000, efforts=["high"]}]` — id, context window, efforts per model (effort values from models.dev)
## `[providers.openai]`

- `base_url` = `"https://api.openai.com/v1"`
- `api_key_env` = `"OPENAI_API_KEY"`
- `api` = `"responses"` — "responses" | "chat"
- `models` = `[{id="gpt-5.1", context_window=400000, efforts=["low", "high"]}, {id="gpt-5.5", context_window=1050000, efforts=["low", "high", "xhigh"]}, {id="gpt-5.6-sol", context_window=1050000, efforts=["low", "high", "xhigh"]}]` — id, context window, efforts per model (effort values from models.dev)
## `[providers.local]`

- `base_url` = `"http://localhost:11434/v1"`
- `api` = `"chat"`
- `model` = `"qwen3-coder"`
- `context_window` = `32768` — local servers do not report it
- `models` = `[{id="qwen3-coder", context_window=32768, efforts=["low", "high", "xhigh"]}]` — id, context window, efforts per model
## `[providers.typesafe]`

- `base_url` = `"https://api.typesafe.ai"` — client appends /v1/systemone
- `api_key_env` = `"TYPESAFE_API_KEY"` — else keyring entry "cox/typesafe"
- `model` = `"jev-latest"`
- `timeout_s` = `30`
- `max_retries` = `2`
- `models` = `[{id="jev-latest", context_window=128000, efforts=["low"]}]` — decisions are cheap-tier only
## `[providers.deepseek]`

- `base_url` = `"https://api.deepseek.com"` — client appends /chat/completions
- `api_key_env` = `"DEEPSEEK_API_KEY"`
- `api` = `"chat"`
- `model` = `"deepseek-v4-pro"`
- `context_window` = `1000000`
- `models` = `[{id="deepseek-v4-flash", context_window=1000000, efforts=["low", "high", "xhigh"]}, {id="deepseek-v4-pro", context_window=1000000, efforts=["high", "xhigh"]}, {id="deepseek-v4-flash-vision-exp", context_window=1000000, efforts=["low", "high", "xhigh"]}]` — id, context window, efforts per model (effort values from models.dev)
## `[providers.openrouter]`

- `base_url` = `"https://openrouter.ai/api/v1"`
- `api_key_env` = `"OPENROUTER_API_KEY"`
- `api` = `"chat"`
- `model` = `"anthropic/claude-sonnet-5"`
- `context_window` = `1000000`
- `models` = `[{id="anthropic/claude-sonnet-5", context_window=1000000, efforts=["low", "high", "xhigh"]}, {id="anthropic/claude-opus-5", context_window=1000000, efforts=["low", "high", "xhigh"]}, {id="deepseek/deepseek-v4-pro", context_window=1048576, efforts=["low", "high", "xhigh"]}, {id="qwen/qwen3-coder-plus", context_window=1000000, efforts=["low", "high", "xhigh"]}, {id="x-ai/grok-4.3", context_window=1000000, efforts=["low", "high", "xhigh"]}]` — curated coding subset; the full 359-model list lives in models.dev
## `[providers.moonshot]`

- `base_url` = `"https://api.moonshot.ai/v1"`
- `api_key_env` = `"MOONSHOT_API_KEY"`
- `api` = `"chat"`
- `model` = `"kimi-k2.6"`
- `context_window` = `262144`
- `models` = `[{id="kimi-k2.6", context_window=262144, efforts=["low", "high", "xhigh"]}, {id="kimi-k2.7-code", context_window=262144, efforts=["low", "high", "xhigh"]}]` — id, context window, efforts per model
## `[providers.z-ai]`

- `base_url` = `"https://api.z.ai/api/paas/v4"`
- `api_key_env` = `"ZHIPU_API_KEY"`
- `api` = `"chat"`
- `model` = `"glm-5.2"`
- `context_window` = `1000000`
- `models` = `[{id="glm-5.2", context_window=1000000, efforts=["low", "high", "xhigh"]}, {id="glm-5.3", context_window=1000000, efforts=["low", "high", "xhigh"]}]` — id, context window, efforts per model
## `[context]`

- `compact_at` = `0.75` — fraction of max_context
- `keep_turns` = `2`
- `microcompact_after_turns` = `6`
- `tool_output_visible_bytes` = `8192`
- `tool_output_head_lines` = `60`
- `tool_output_tail_lines` = `20`
- `dedup_window_turns` = `8`
- `instruction_budget_tokens` = `8000`
- `memory_budget_tokens` = `800`
- `deferred_tools` = `true`
- `system_prompt` = `"default"` — default | minimal (T30.1); `core.profile = "minimal"` implies it
## `[permissions]`

- `mode` = `"default"` — default | plan | auto | bypass (bypass only via flag)
- `approval` = `"on-request"` — untrusted | on-request | on-failure | never
- `allow` = `[]` — rule strings, §1.8
- `ask` = `[]`
- `deny` = `["Read(~/.ssh/**)", "Read(~/.aws/**)", "Bash(rm -rf /*)"]`
- `import_claude_settings` = `true`
- `allow_for_session_persists` = `false`
## `[sandbox]`

- `mode` = `"workspace-write"` — read-only | workspace-write | danger-full-access
- `network` = `false`
- `writable` = `[]` — extra writable roots
- `readonly_in_workspace` = `[".git", ".cox", ".claude"]`
- `linux_backend` = `"auto"` — auto | bwrap | landlock | none
## `[budget]`

- `session_usd` = `5.0`
- `monthly_usd` = `100.0`
- `warn_at` = `0.8`
- `cheap_counts` = `true`
## `[tui]`

- `vim` = `false`
- `theme` = `"auto"` — auto | dark | light | a built-in (cox-dark, cox-light, system) or a `~/.cox/themes/<name>.toml` file's stem; `auto` queries the terminal's OSC 11 background colour once, before raw mode, with a 100 ms timeout (T22.6) — tmux, a query error, or no reply within the timeout falls back to `dark`, same as before this query existed. An explicit `dark`/`light` (config file or `COX_TUI_THEME`) always wins over detection; a named theme follows the same read unless its file pins a `variant`. `cox doctor` reports what `auto` resolved to. `/theme` previews and writes this (T24.2).
- `inline` = `true`
- `show_thinking` = `"collapsed"` — collapsed | hidden | full
- `screen_reader` = `false` — the plain surface (T29.1): flat labelled lines, numbered prompts, no cursor movement, BEL when a turn ends; same as `--plain` or `COX_PLAIN=1`
- `mouse` = `true` — the wheel scrolls the transcript, the diff view and pickers 3 lines/rows a tick; no in-app toggle key, so false leaves the terminal's own mouse reporting (and text selection) untouched (T22.4)
- `glyphs` = `"auto"` — auto | unicode | ascii
- `icons` = `{}` — [tui.icons] name = "glyph" overrides one symbol
- `color` = `"auto"` — auto | none | 16 | 256 | true (NO_COLOR forces none)
- `syntax_theme` = `""` — syntect theme for code, diffs and file output ("" follows theme); a `.tmTheme` file in `~/.cox/themes/` is merged in at startup and offered by `/theme` under a `syntax: ` prefix (T24.2)
- `diff` = `"auto"` — auto | side | stacked — edit cards, the approval modal and Ctrl+G split old and new side by side from 120 columns (auto and side alike; narrower stays stacked), stacked never splits; replaced lines highlight the changed words (T24.5)
- `git` = `true` — branch and +n -m in the status line, polled every 2 s
- `notify` = `"auto"` — auto | always | off — OSC 9 (OSC 777 on VTE) plus BEL when a turn ends, an approval waits or ask_user asks; auto only while the terminal is unfocused (focus reporting), always regardless, off never (T23.5)
- `motion` = `"full"` — full | reduced — reduced draws a running tool's spinner as one still glyph and replaces its ticking elapsed time with `running` (T24.7)
- `caps` = `{}` — [tui.caps] name = bool overrides one detected cox_tui::term::Caps field (truecolor, kitty_keyboard, osc8, osc52, osc9, osc9_4, focus, images) for a terminal detection guesses wrong about; unset fields are auto-detected, `cox doctor` shows the source of each (T23.0)
## `[hooks]`

- `timeout_s` = `60` — seconds per [[hooks.<Event>]] process (a hook's own timeout_s overrides); stdin carries the Claude Code JSON payload, exit 2 blocks, stdout may carry updatedInput or additionalContext
- `fail_open` = `true` — a hook that crashes, times out or has an invalid matcher regex is warned about and skipped, never fatal (D14). matcher is an exact tool name, or — when it carries a regex metacharacter — a regex over the tool name (T22.3). Events: UserPromptSubmit, PreToolUse, PostToolUse, PostToolUseFailure, Stop, PreCompact, PostCompact, SessionStart (payload source: startup | resume | clear; stdout additionalContext joins the volatile system block), SessionEnd, PermissionRequest, SubagentStart, SubagentStop, Notification (observe-only kind/message/title payload on ApprovalRequired, TurnDone and ask_user)
## `[mcp]`

- `timeout_s` = `30`
- `deferred` = `true`
- `servers` = `{}` — [mcp.servers.<name>] command/args/url/env — same shape as .mcp.json
## `[memory]`

- `enabled` = `true`
- `extract` = `false` — end-of-session extraction on cheap tier
- `dir` = `""` — default ~/.cox/projects/<slug>/memory
## `[telemetry]`

- `otel` = `false`
- `endpoint` = `""`
## `[record]`

- `redact` = `true`
## `~/.cox/keybindings.toml`

Rebinds the TUI's keys (T25.5). Each line is an action id and a key, or a list of keys; dotted ids may be written as TOML tables. The keys you give replace the action's defaults, in every context the action has (`idle`, `running`), and take the key from whatever action held it by default. A missing file means the defaults in `docs/getting-started.md`.

```toml
send = "ctrl+enter"
newline = ["enter", "shift+enter"]
mode.cycle = "shift+tab"
```

- Actions: `send`, `newline`, `send.now`, `interrupt`, `mode.cycle`, `transcript`, `help`, `thinking`, `expand`, `diff`, `background`, `unqueue`, `quit`, `copy`, `copy.all`. `@`, `/`, `Ctrl+R` and the keys inside a picker or overlay are fixed; so is `Ctrl+C`.
- Keys: modifiers `ctrl`, `alt` (`opt`, `meta`), `shift`, `cmd` (`super`), then one key: a character, `enter`, `esc`, `tab`, `space`, `backspace`, `delete`, arrows, `pageup`, `pagedown`, `home`, `end`, `f1`–`f12`. Any case. Chords (`ctrl+x ctrl+s`) are not supported.
- A plain terminal sends the same byte for `Enter` and `Ctrl+Enter`; `ctrl+enter` needs a terminal that reports it (kitty keyboard protocol, see `cox doctor`).
- `~/.claude/keybindings.json` is read first, for the actions both tools have: `chat:submit` → `send`, `chat:newline` → `newline`, `chat:sendNow` → `send.now`, `chat:cancel` → `interrupt`, `chat:cycleMode` → `mode.cycle`, `app:toggleTranscript` → `transcript`, `task:background` → `background`, `app:exit` → `quit`. Its keys are added beside the defaults; this file still wins. Other Claude actions and chords are skipped.
- An unknown action, a bad key or a file that is not TOML is a warning in the transcript and is skipped. `cox doctor` lists those and any key two of your bindings both claim.

## Accessibility

- `--plain` (or `tui.screen_reader = true`, or `COX_PLAIN=1`) swaps the TUI for flat labelled lines a screen reader can follow: numbered prompts, no cursor movement, and a BEL when a turn ends.
- `tui.motion = "reduced"` stops everything that moves by itself. A running tool shows one still glyph and `running` instead of a spinner and a ticking clock.
- `tui.theme = "cox-dark-daltonized"` or `"cox-light-daltonized"` are the built-in themes without a red/green pair. Added lines and success are blue; removed lines and failure are orange. Every state also keeps its glyph (`✓`, `✗`, `+`, `−`), so colour is never the only signal. `/theme` previews both.
- `NO_COLOR` (set and non-empty, while `tui.color` is `"auto"`), or `tui.color = "none"`, prints no colour at all and leaves the terminal's own.
