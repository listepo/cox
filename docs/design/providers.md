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

## Target shape: one model of providers, models, prices and effort (T30.17)

**What the registry left split** (R§4.3.3). The registry unified *which* providers exist. It did not unify *how* each one is built or where model facts live:

- Only two of five families have retry and timeout knobs.
- Three key paths exist, and one of them ignores its own `api_key_env`.
- There are three sources for context and capability: `ProviderModel`, the `Caps.max_context` literals, and `ADAPTIVE_THINKING_PREFIXES`.
- Every wire maps effort ad hoc.

Routing (`Router::pick`) and costing (`Priced`) are already single and stay that way.

**Target.**

1. **One transport descriptor per section.** Every `[providers.*]` table, native or compatible, flattens the same `Transport { base_url, api_key_env, timeout_s, max_retries }`. Section-specific knobs stay beside it, for example Anthropic's `cache_ttl` and the `api` shape. Every constructor takes a `&Transport`. `backend_for` becomes one lookup from the `api` shape to a constructor, not one arm per family.

   Implemented (T30.22): config side. `cox_protocol::config::Transport` is the one type; every section (`AnthropicProviderConfig`, `OpenAiProviderConfig`, `LocalProviderConfig`, `JevProviderConfig`, `CompatibleProviderConfig`) exposes it via a `transport()` accessor rather than `#[serde(flatten)]` — serde refuses to combine `flatten` with `deny_unknown_fields` on the same struct, and keeping the four fields flat on each section (`base_url = …` stays a section-level TOML key) preserves the typo check. `openai`, `local` and every compatible section gained `timeout_s`/`max_retries` (matching `retry::Policy::default()`'s `max_retries: 4` and Anthropic's `timeout_s: 120` convention — neither is wired into a client yet, so behaviour is unchanged); `local` also gained `api_key_env` (default empty = no key; LM Studio's `LM_API_TOKEN` will use it, T30.15).

   Implemented (T30.23): constructors. `AnthropicProvider::new`, `JevProvider::new`/`with_key`, `OpenAiChatProvider::new` and `OpenAiResponsesProvider::new` all take `&Transport` (plus their own section-specific knobs — Anthropic's `ttl`/`fallbacks`, Jev's `model`) instead of five-odd positional scalars; `client_with_timeout` in `cox-provider/src/http.rs` is the one place that builds a `reqwest::Client` with a bounded connect timeout and `transport.timeout_s` as the read/idle timeout, reused by Anthropic (which used to duplicate it), Chat and Responses (which used to build an untimed `reqwest::Client::new()`); every backend's retry policy reads `transport.max_retries` instead of `Policy::default()`. `backend_for` in `crates/cox/src/session.rs` is one lookup: Anthropic and Jev keep their own arm (their extra knobs), and `openai`/`local`/every compatible section go through one `openai_shaped(owner, &Transport, models, context_window, api)` that resolves the key once and picks the Chat or Responses constructor by `api`. `local`'s `timeout_s` default is 600, not the 120 every remote section keeps — slow on-device prefill can otherwise cut a large prompt off before the first byte.
2. **One key resolver.** `http::resolve_key(section)` reads the section's `api_key_env`, then the keyring `cox/<section>`, for every section. A local server needs no key: a missing key there is "no auth header", not an error. That is what LM Studio needs (T30.15).

   Implemented (T30.21): `http::resolve_key(api_key_env, section)` in `cox-provider/src/http.rs` is the one resolver every section goes through. Anthropic and Jev, which always need a key, propagate its `Err` as `ProviderError::Auth`; `openai`, `local` and every compatible section build with an `Option<String>` key and call `.ok()`, so a missing key there is "no `Authorization` header", not a startup failure.
3. **One model catalog** in a new pure crate `cox-models` (see `crates.md`).
   - Each row: `id → context_window, max_output, efforts, capabilities (tools, adaptive_thinking, reasoning_effort_param), price`.
   - Built-in rows are embedded, the way `prices.toml` is today, and written only by T30.20's script from models.dev (A48). `[providers.<name>].models` entries and a user `prices.toml` override them by id.
   - `Caps` is derived from the catalog. The literals `200_000`, `128_000` and `400_000` and the prefix table are deleted.
   - A local server's loaded context (T30.16) is one more override source.
4. **One effort map.** The mapping `effort_for(api, Effort, &caps) -> Option<WireEffort>` lives in `cox-models`, next to the catalog.
   - Anthropic: `output_config.effort` plus adaptive thinking when the catalog says so.
   - Responses: `reasoning.effort`.
   - Chat: the `reasoning_effort` field when the row declares it. Whether the OpenAI Chat API and LM Studio accept it is checked against their API references in that card.
   - Jev: explicitly `None`.

   `clamp_effort` keeps enforcing the model's supported levels, now from the catalog. `Effort` gains `Medium` so models.dev's four levels map without loss.
5. **One sync check.** `cox doctor` reports a routable model with no catalog price, instead of relying only on the unit test.

**Kept, with reasons.**

- `ModelId` stays a plain string: gateway ids pass through verbatim, and nothing needs to parse `vendor/model`.
- `--provider`/`--model` keep retargeting only the `code` tier; `--tier` covers the others.
- Custom providers keep `ProviderId::Local`, so storage needs no migration.
- `cache_write_tokens = 0` on Chat and Responses is correct, because those APIs bill no cache writes.

**Migration order** (each step green; cards T30.21–T30.27 in `plan.md`, U*n* = T30.*(20+n)*):

| Card | Change | Files |
|---|---|---|
| U1 | One key resolver for every section | `http.rs`, `anthropic/mod.rs`, `session.rs` |
| U2 | `Transport` flattened into every section; schema regenerated | `config.rs`, schema, `config_load.rs` |
| U3 | Constructors take `&Transport`; Chat and Responses get configured retries | `session.rs`, `chat.rs`, `responses.rs` |
| U4 | `cox-models` crate: catalog types, embedded rows, merge with config and `prices.toml` | new crate, `usage.rs` |
| U5 | `Caps` and adaptive thinking from the catalog; delete the literals and the prefix table | `anthropic/*`, `jev.rs`, `session.rs` |
| U6 | `effort_for` in one place; `Effort::Medium` | `cox-models`, `request.rs`, `responses.rs` |
| U7 | `cox doctor` catalog/price sync row | `doctor.rs` |

T30.15 (LM Studio chat) waits for U1–U3 (T30.21–T30.23), so it lands as one more `Transport` section and not a sixth divergent one. T30.16 (loaded context) waits for U4–U5 (T30.24–T30.25), so the server's context feeds the catalog. T30.13 depends on none of this.
