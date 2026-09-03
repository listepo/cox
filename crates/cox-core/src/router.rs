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
            other => {
                return Err(RouteError::UnknownProvider {
                    tier,
                    name: other.to_string(),
                });
            }
        };
        // Local-only mode pins the local server's model: a Claude id would
        // be meaningless to Ollama/vLLM and may 404.
        let model = if provider == ProviderId::Local {
            ModelId(config.providers.local.model.clone())
        } else {
            overrides
                .models
                .get(&tier)
                .cloned()
                .unwrap_or(ModelId(tc.model.clone()))
        };
        Ok(Route {
            tier,
            provider,
            model,
            effort: tc.effort,
            thinking: tc.thinking,
            max_tokens: tc.max_tokens,
        })
    }
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
    use cox_protocol::types::Role;

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
