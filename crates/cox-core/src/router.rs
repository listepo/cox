//! Job → tier → model routing (D5, T9.1). One pure function owns every
//! routing choice: which tier a job runs on, which model string it sends,
//! and whether the `think` tier's confirmation gate blocks the turn. The
//! loop never guesses a model itself; a failing cheap call is retried on
//! cheap because `pick` is stateless.

use std::collections::HashMap;

use cox_protocol::Config;
use cox_protocol::types::{Content, Effort, Job, Message, ModelId, ProviderId, Thinking, Tier};

/// Fable 5.1 list prices shown by the think gate (Anthropic first-party,
/// re-verify in T1.7's `prices.toml` when it changes).
pub const THINK_PRICE: &str = "$10/$50 per MTok";

/// Where one provider call goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Resolved tier.
    pub tier: Tier,
    /// Backend the tier names.
    pub provider: ProviderId,
    /// Model id sent on the wire.
    pub model: ModelId,
    /// Effort from the tier block.
    pub effort: Effort,
    /// Thinking mode from the tier block.
    pub thinking: Thinking,
    /// Max output tokens from the tier block.
    pub max_tokens: u32,
}

/// Session-scoped routing overrides (`/model`, `--tier <tier>=<model>`).
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    /// Per-tier model replacements; absent means the tier's configured model.
    pub models: HashMap<Tier, ModelId>,
    /// `/model <tier>`: main turns run on this tier instead of `jobs.main`.
    pub main_tier: Option<Tier>,
}

/// Why `pick` refused to route.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteError {
    /// The `think` tier needs `confirm_think` (invariant #9).
    NeedsConfirm {
        /// Tier that required confirmation.
        tier: Tier,
        /// Model that would have been called.
        model: ModelId,
    },
    /// A tier names no known `[providers.*]` table.
    UnknownProvider {
        /// Tier carrying the bad name.
        tier: Tier,
        /// The unknown name.
        name: String,
    },
}

impl RouteError {
    /// What the model and the user see for this refusal.
    pub fn notice(&self) -> String {
        match self {
            Self::NeedsConfirm { model, .. } => format!(
                "think tier ({}) requires confirmation: resubmit with confirm_think \
                 ({THINK_PRICE})",
                model.0
            ),
            Self::UnknownProvider { tier, name } => {
                format!("tier {tier:?} names unknown provider `{name}`")
            }
        }
    }
}

/// The router: stateless job → route resolution (plan.md T9.1).
pub struct Router;

impl Router {
    /// Resolves `job` to a [`Route`]. Main turns run on the session tier
    /// (or the `/model` tier); every other job follows the `[jobs]` table.
    /// Pure: the same inputs always give the same route, so a retry never
    /// escalates a tier on its own.
    pub fn pick(
        config: &Config,
        job: Job,
        session_tier: Tier,
        overrides: &Overrides,
        confirm_think: bool,
    ) -> Result<Route, RouteError> {
        let tier = match job {
            Job::Main => overrides.main_tier.unwrap_or(session_tier),
            _ => config.jobs.tier_for(job),
        };
        let tc = config.tiers.get(tier);
        if tier == Tier::Think && tc.confirm && !confirm_think {
            return Err(RouteError::NeedsConfirm {
                tier,
                model: ModelId(tc.model.clone()),
            });
        }
        let provider = match tc.provider.as_str() {
            "anthropic" => ProviderId::Anthropic,
            "openai" => ProviderId::OpenAi,
            "local" => ProviderId::Local,
            // Type-2 (compatible) providers are OpenAI-Chat-shaped by
            // construction, so they ride the `Local` family id — the
            // ledger's "OpenAI-compatible" bucket, where the model string
            // disambiguates the row (precedent: Scripted/Replay already do).
            other if config.providers.custom.contains_key(other) => ProviderId::Local,
            other => {
                return Err(RouteError::UnknownProvider {
                    tier,
                    name: other.to_string(),
                });
            }
        };
        // The model the wire carries: a session override (`/model`,
        // `--tier TIER=MODEL`) wins; otherwise a Local-family tier pins its
        // section's default model — a Claude id would 404 against Ollama,
        // and a bare `tiers.code.provider = "deepseek"` flip must work
        // without also editing every tier model. Native tiers carry their
        // own configured model. An empty section default falls back to the
        // tier model rather than sending an empty id.
        let pinned = match tc.provider.as_str() {
            "local" => Some(config.providers.local.model.clone()),
            other => config.providers.custom.get(other).map(|c| c.model.clone()),
        };
        let model = overrides.models.get(&tier).cloned().unwrap_or_else(|| {
            pinned
                .filter(|m| !m.is_empty())
                .map(ModelId)
                .unwrap_or_else(|| ModelId(tc.model.clone()))
        });
        Ok(Route {
            tier,
            provider,
            model: model.clone(),
            effort: clamp_effort(config, &tc.provider, &model, tc.effort),
            thinking: tc.thinking,
            max_tokens: tc.max_tokens,
        })
    }
}

/// Clamps the tier's effort to what the routed model supports: the greatest
/// supported level at or below the request, else the lowest supported one.
/// Models absent from the section list — or an empty list — pass through
/// untouched, so a gateway can serve models cox never catalogued.
fn clamp_effort(config: &Config, provider: &str, model: &ModelId, want: Effort) -> Effort {
    let supported: Vec<Effort> = config
        .providers
        .models_for(provider)
        .iter()
        .find(|m| m.id == model.0)
        .map(|m| m.efforts.clone())
        .unwrap_or_default();
    if supported.is_empty() || supported.contains(&want) {
        return want;
    }
    supported
        .iter()
        .filter(|e| **e <= want)
        .max()
        .or_else(|| supported.iter().min())
        .copied()
        .unwrap_or(want)
}

/// Drops `Thinking` blocks after a model switch: a signature binds its block
/// to the model and prefix that produced it, so replaying it under another
/// model is a guaranteed provider error. Messages left empty are dropped;
/// everything else is byte-identical.
pub fn strip_thinking(messages: &[Message]) -> Vec<Message> {
    messages
        .iter()
        .filter_map(|msg| {
            let kept: Vec<Content> = msg
                .content
                .iter()
                .filter(|c| !matches!(c, Content::Thinking { .. }))
                .cloned()
                .collect();
            (!kept.is_empty()).then_some(Message {
                role: msg.role,
                content: kept,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cox_protocol::config::CompatibleProviderConfig;
    use cox_protocol::types::Role;

    fn custom_config() -> Config {
        let mut cfg = Config::default();
        cfg.tiers.code.provider = "deepseek".into();
        cfg.providers.custom.insert(
            "deepseek".into(),
            CompatibleProviderConfig {
                base_url: "https://api.deepseek.com".into(),
                api_key_env: "DEEPSEEK_API_KEY".into(),
                api: "chat".into(),
                model: "deepseek-v4-pro".into(),
                context_window: 1_000_000,
                models: vec![
                    cox_protocol::config::ProviderModel {
                        id: "deepseek-v4-flash".into(),
                        context_window: 1_000_000,
                        efforts: vec![Effort::Low, Effort::High, Effort::Xhigh],
                    },
                    cox_protocol::config::ProviderModel {
                        id: "deepseek-v4-pro".into(),
                        context_window: 1_000_000,
                        efforts: vec![Effort::High, Effort::Xhigh],
                    },
                ],
            },
        );
        cfg
    }

    #[test]
    fn router_custom_provider_pins_section_model_and_clamps_effort() {
        let cfg = custom_config();
        // A bare provider flip routes without touching tier models: the
        // section default pins the wire id (a Claude id would 404).
        let route = Router::pick(&cfg, Job::Main, Tier::Code, &Overrides::default(), true)
            .expect("custom routes");
        assert_eq!(route.provider, ProviderId::Local);
        assert_eq!(route.model.0, "deepseek-v4-pro");
        // Code tier asks High, pro supports it: unchanged.
        assert_eq!(route.effort, Effort::High);
        // An override still wins over the pin (gateway escape hatch).
        let mut overrides = Overrides::default();
        overrides
            .models
            .insert(Tier::Code, ModelId("deepseek-v4-flash".into()));
        let route = Router::pick(&cfg, Job::Main, Tier::Code, &overrides, true).expect("routes");
        assert_eq!(route.model.0, "deepseek-v4-flash");
        assert_eq!(route.effort, Effort::High);
    }

    #[test]
    fn router_clamp_effort_never_upgrades_past_the_request() {
        let cfg = custom_config();
        // Xhigh on flash (supports all three): passes through.
        assert_eq!(
            clamp_effort(
                &cfg,
                "deepseek",
                &ModelId("deepseek-v4-flash".into()),
                Effort::Xhigh
            ),
            Effort::Xhigh
        );
        // Low on pro (supports high/xhigh): raised to the floor, the only
        // direction that keeps the call valid.
        assert_eq!(
            clamp_effort(
                &cfg,
                "deepseek",
                &ModelId("deepseek-v4-pro".into()),
                Effort::Low
            ),
            Effort::High
        );
        // Unlisted models and unknown providers pass through untouched.
        assert_eq!(
            clamp_effort(
                &cfg,
                "deepseek",
                &ModelId("deepseek-future-1".into()),
                Effort::Xhigh
            ),
            Effort::Xhigh
        );
        assert_eq!(
            clamp_effort(&cfg, "local", &ModelId("qwen3-coder".into()), Effort::High),
            Effort::High
        );
    }

    #[test]
    fn router_strip_thinking_keeps_everything_else_verbatim() {
        let thinking = Content::Thinking {
            text: "hmm".into(),
            signature: Some("sig".into()),
        };
        let text = Content::Text { text: "hi".into() };
        let messages = vec![
            Message {
                role: Role::User,
                content: vec![text.clone()],
            },
            Message {
                role: Role::Assistant,
                content: vec![thinking, text.clone()],
            },
            Message {
                role: Role::Assistant,
                content: vec![Content::Thinking {
                    text: "only".into(),
                    signature: None,
                }],
            },
        ];
        let out = strip_thinking(&messages);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].content, vec![text.clone()]);
        assert_eq!(out[1].content, vec![text]);
    }
}
