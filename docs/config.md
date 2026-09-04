# cox configuration reference

Generated from `config/default.toml` by a test in `cox-protocol/src/config.rs`; do not hand-edit.

## `[core]`

- `home` = `"~/.cox"` — COX_HOME overrides
- `workspace_roots` = `[]` — empty = git root of cwd, else cwd; extra roots via --add-dir
- `max_turns` = `200` — per UserTurn, counts provider calls
- `parallel_tools` = `4`
- `log_level` = `"info"` — tracing filter; file log at ~/.cox/logs/cox.log
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
- `theme` = `"auto"` — auto | dark | light
- `inline` = `true`
- `show_thinking` = `"collapsed"` — collapsed | hidden | full
- `mouse` = `true`
- `glyphs` = `"auto"` — auto | unicode | ascii
- `icons` = `{}` — [tui.icons] name = "glyph" overrides one symbol
- `color` = `"auto"` — auto | none | 16 | 256 | true (NO_COLOR forces none)
## `[hooks]`

- `timeout_s` = `60`
- `fail_open` = `true`
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
