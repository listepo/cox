# Design: provider registry (two types, opencode-shaped data)

## Problem

Adding a provider today costs a code change: `ProviderId` has three
variants, `Router::pick` matches three names, `provider_for` builds three
clients, and prices live in a 4-row table. Each new API (DeepSeek,
OpenRouter, Kimi, GLM, …) would repeat that trail, although all of them
speak the same OpenAI Chat Completions wire format cox already implements.
Measurable question: can a user add DeepSeek or OpenRouter with config
lines only, zero new Rust, and still get a priced ledger row?

## The field

**OpenCode + models.dev (evidence 2026-09-04, `~/.cache/opencode/models.json`,
213 providers).** One registry row per provider: `api` base URL, `env` key
name, `npm` adapter (`@ai-sdk/anthropic`, `@ai-sdk/openai`,
`@ai-sdk/openai-compatible`), and a `models` map each carrying
`limit.context/output`, `cost.*` per MTok, `tool_call`, and
`reasoning_options.effort` values. A custom provider is the same shape
hand-written: nuclear option is `npm: @ai-sdk/openai-compatible` +
`options.baseURL` + a `models` map (verified in the user's own
`opencode.json`: `headroom`, `teamorouter`). Native code exists only per
*adapter*, never per vendor: DeepSeek/OpenRouter/MoonShot are all
`openai-compatible`.

**Aider / Pi.** Same split: a short list of hand-rolled clients plus a
generic OpenAI-compatible endpoint taking base URL + key + model name.

## cox

Two types, split by wire protocol, not by vendor:

- **Type 1 — native (`Provider` impl).** Only when the wire format is new:
  `AnthropicProvider` (Messages), `OpenAiResponsesProvider` (Responses),
  `OpenAiChatProvider` (Chat Completions). A new vendor on an existing
  protocol adds no code — writing a `DeepseekProvider` struct over the
  Chat URL would duplicate `OpenAiChatProvider` line for line.
- **Type 2 — compatible (`[providers.<name>]` table).** Pure data, the
  opencode custom-provider shape: `base_url`, `api_key_env`, `api = "chat"`,
  a default `model`, a fallback `context_window`, and a `models` list where
  each entry carries the model id, its context window and the efforts it
  understands (models.dev `reasoning_options.effort` mapped to cox
  `Effort`: `low→low`, `medium/high→high`, `xhigh/max→xhigh`; `toggle`-only
  models accept all three). Costs stay in `prices.toml` — the one file the
  ledger reads — extended with the same ids from models.dev. Runtime use of
  the list is real, not decorative: per-model context resolution feeds
  `Caps::max_context`, which drives the compaction trigger.

Custom providers report `ProviderId::Local` — the ledger's
"OpenAI-compatible family" id (precedent: `Scripted`/`Replay` already do),
so no storage migration; the model string disambiguates the row. Unknown
tier names still fail closed (`RouteError::UnknownProvider`); a typo'd
`[providers.*]` table can only become a routable name by being referenced
from `[tiers.*]`.

Seed: `deepseek` (3 models), `openrouter` (curated coding subset),
`moonshot` (Kimi K2.x), `z-ai` (GLM-5.x) — all `tool_call=true` in
models.dev, all Chat-compatible. Candidates deliberately deferred (same
shape, one table each when asked): `groq`, `togetherai`, `fireworks-ai`,
`nvidia`, `x-ai`, `cerebras`, `minimax`.

## Falsifier

Add a fifth compatible provider (e.g. `groq`) using only config lines; if
any `.rs` file must change, the registry leaked. Conversely, if a vendor
ships a wire format none of the three clients parses (fixtures fail to
produce `ToolUseStart`), that vendor graduates to Type 1 — the split is
decided by fixture, not by brand.
