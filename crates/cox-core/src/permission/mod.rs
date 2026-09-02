//! The single place a tool call is allowed, denied or escalated (AGENTS.md
//! trust boundaries; plan.md §1.8). Pure: rules compile once from config,
//! then `decide` is a function of the call, the mode, the policy and the
//! session grants — no I/O, so the 30-row table and the proptest need no
//! session around them. A tool never checks its own permission.

pub mod rules;

use std::path::Path;

use cox_protocol::config::PermissionsConfig;
use cox_protocol::errors::CoreError;
use cox_protocol::types::{ApprovalPolicy, DecidedBy, PermissionMode, Risk, ToolCall, Why};

use rules::{Rule, canonical_tool};

/// What the engine concluded for one call.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Run it.
    Allow {
        /// What allowed it.
        by: DecidedBy,
    },
    /// Refuse it; the model sees `reason`.
    Deny {
        /// Shown in the tool result.
        reason: String,
        /// What denied it.
        by: DecidedBy,
    },
    /// The surface must ask the user.
    Ask(Why),
}

/// Compiled `allow`/`ask`/`deny` rules.
#[derive(Debug, Clone, Default)]
pub struct Engine {
    deny: Vec<Rule>,
    allow: Vec<Rule>,
    ask: Vec<Rule>,
}

impl Engine {
    /// Compiles the three rule lists; a malformed rule is a config error,
    /// never a silently skipped guard.
    pub fn compile(
        cfg: &PermissionsConfig,
        home: Option<&Path>,
        cwd: &Path,
    ) -> Result<Self, CoreError> {
        let compile = |key: &str, raw: &[String]| {
            raw.iter()
                .map(|r| {
                    Rule::parse(r, home, cwd).map_err(|message| CoreError::Config {
                        key: format!("permissions.{key}"),
                        message: format!("{r:?}: {message}"),
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(Self {
            deny: compile("deny", &cfg.deny)?,
            allow: compile("allow", &cfg.allow)?,
            ask: compile("ask", &cfg.ask)?,
        })
    }

    /// Plan.md §1.8 steps 1–9, in order.
    pub fn decide(
        &self,
        call: &ToolCall,
        mode: PermissionMode,
        policy: ApprovalPolicy,
        grants: &[(String, String)],
    ) -> Outcome {
        let first = |rules: &[Rule]| {
            rules
                .iter()
                .find(|r| r.matches(&call.name, &call.subject))
                .map(|r| r.raw.clone())
        };
        if let Some(rule) = first(&self.deny) {
            return Outcome::Deny {
                reason: format!("denied by rule {rule}"),
                by: DecidedBy::Rule,
            };
        }
        if mode == PermissionMode::Bypass {
            return Outcome::Allow {
                by: DecidedBy::Policy,
            };
        }
        if mode == PermissionMode::Plan {
            return if call.risk == Risk::ReadOnly {
                Outcome::Allow {
                    by: DecidedBy::Policy,
                }
            } else {
                Outcome::Deny {
                    reason: "plan mode: only read-only tools run; describe the change instead"
                        .into(),
                    by: DecidedBy::Policy,
                }
            };
        }
        if first(&self.allow).is_some() {
            return Outcome::Allow {
                by: DecidedBy::Rule,
            };
        }
        let why = if let Some(rule) = first(&self.ask) {
            Some(Why::RuleAsk { rule })
        } else if grants.iter().any(|(tool, subject)| {
            rules::tool_matches(&canonical_tool(tool), &call.name)
                && call.subject.starts_with(subject.as_str())
        }) {
            return Outcome::Allow {
                by: DecidedBy::Session,
            };
        } else if policy == ApprovalPolicy::Untrusted && call.risk != Risk::ReadOnly {
            Some(Why::Policy { policy })
        } else {
            by_risk(call.risk, mode, policy)
        };
        match why {
            None => Outcome::Allow {
                by: DecidedBy::Policy,
            },
            Some(why) if policy == ApprovalPolicy::Never => Outcome::Deny {
                reason: format!("{} and the approval policy is `never`", why_text(&why)),
                by: DecidedBy::Policy,
            },
            Some(why) => Outcome::Ask(why),
        }
    }
}

/// Step 7: what the risk alone requires. `Exec` marked safe by the T3.7
/// classifier arrives here as `ReadOnly` (A5: risk is per call).
fn by_risk(risk: Risk, mode: PermissionMode, policy: ApprovalPolicy) -> Option<Why> {
    match risk {
        Risk::ReadOnly => None,
        Risk::Write if mode == PermissionMode::Auto => None,
        Risk::Exec if policy == ApprovalPolicy::OnFailure => None,
        _ => Some(Why::Risk { risk }),
    }
}

/// One line for a `Deny` reason or a notice.
pub fn why_text(why: &Why) -> String {
    match why {
        Why::RuleAsk { rule } => format!("rule {rule} requires approval"),
        Why::Risk { risk } => format!("{risk:?} calls require approval"),
        Why::SandboxDenied { detail } => format!("the sandbox denied it: {detail}"),
        Why::Policy { policy } => format!("approval policy {policy:?} requires approval"),
    }
}
