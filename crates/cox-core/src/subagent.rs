//! Subagents (plan.md T3.9): the `agent` tool runs one task in a child
//! `Session` on its own tier with a tool allowlist, a budget slice and a
//! result cap. It lives in `cox-core` rather than `cox-tools` because a
//! child session *is* the loop, not I/O, and `cox-tools` may not depend on
//! this crate; the presets are plain data here for the same reason.

use std::sync::Arc;

use async_trait::async_trait;
use cox_protocol::errors::{CoreError, ToolError};
use cox_protocol::ids::{ItemId, TaskId};
use cox_protocol::traits::{Tool, ToolCx};
use cox_protocol::types::{
    Concurrency, Content, Event, Job, Message, ModelId, ProviderEvent, Request, Risk, Role,
    Submission, SystemBlock, ToolOutput, ToolSpec,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::budget;
use crate::session::Session;

/// A subagent shape: which job it reports as, which tools it may use, how
/// long it may run and how big its answer may be.
#[derive(Debug, Clone, Copy)]
pub struct Preset {
    /// What the model passes as `preset`.
    pub name: &'static str,
    /// The job its provider calls are recorded under; picks the tier.
    pub job: Job,
    /// Tool names the child may use.
    pub tools: &'static [&'static str],
    /// Every tool must be `ReadOnly`, whatever the allowlist says.
    pub read_only: bool,
    /// Provider calls the child may make for one task.
    pub max_turns: u32,
    /// Results longer than this (≈ 4 bytes per token) are summarised.
    pub result_cap_tokens: usize,
}

/// Read-only file exploration on the cheap tier, short answer.
pub const EXPLORE: Preset = Preset {
    name: "explore",
    job: Job::Explore,
    tools: &["read", "grep", "glob", "outline", "expand"],
    read_only: true,
    max_turns: 30,
    result_cap_tokens: 1000,
};

/// Builds, tests and HTTP calls whose full output the parent does not need.
pub const SHELL: Preset = Preset {
    name: "shell",
    job: Job::Shell,
    tools: &["bash", "web_fetch"],
    read_only: false,
    max_turns: 30,
    result_cap_tokens: 2000,
};

const PRESETS: &[Preset] = &[EXPLORE, SHELL];

/// What a subagent may spend when the call does not say: a quarter of
/// what the parent has left, so four background explorers cannot drain it.
const DEFAULT_SLICE: f64 = 0.25;

/// The child's session cap in USD: never more than the parent has left.
pub fn slice(parent_cap: f64, parent_spent: f64, requested: Option<f64>) -> f64 {
    let remaining = (parent_cap - parent_spent).max(0.0);
    requested.map_or(remaining * DEFAULT_SLICE, |r| r.max(0.0).min(remaining))
}

/// `agent`: delegates a task to a child session and returns its answer.
pub struct AgentTool {
    parent: Session,
}

impl AgentTool {
    pub(crate) fn new(parent: Session) -> Self {
        Self { parent }
    }

    fn preset(input: &Value) -> Result<Preset, ToolError> {
        let name = input
            .get("preset")
            .and_then(Value::as_str)
            .unwrap_or(EXPLORE.name);
        PRESETS
            .iter()
            .copied()
            .find(|p| p.name == name)
            .ok_or_else(|| ToolError::Denied {
                why: format!("unknown agent preset {name:?}; use explore or shell"),
            })
    }

    /// The parent's tools this call may hand to the child: the preset's
    /// allowlist (or the call's `tools`, narrowed to it for `explore`),
    /// never `agent` itself.
    fn tools_for(&self, preset: Preset, input: &Value) -> Vec<Arc<dyn Tool>> {
        let wanted: Vec<String> = input
            .get("tools")
            .and_then(Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_else(|| preset.tools.iter().map(|s| s.to_string()).collect());
        self.parent
            .tools
            .iter()
            .filter(|t| {
                let spec = t.spec();
                spec.name != "agent"
                    && wanted.contains(&spec.name)
                    && (!preset.read_only || spec.risk == Risk::ReadOnly)
            })
            .cloned()
            .collect()
    }
}

#[async_trait]
impl Tool for AgentTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "agent".to_string(),
            description: "Delegate a self-contained task to a subagent that runs on the \
                cheap tier with its own tool set and budget, and returns only its answer. \
                Presets: `explore` (read-only file tools, answer ≤ 1k tokens) for \"find \
                where X is handled and report file:line\", `shell` (bash, web_fetch) for \
                builds, test runs and HTTP calls whose full output you do not need. Pass \
                `task` with everything the subagent needs to know; it does not see this \
                conversation. Optional: `tools` to narrow the tool list, `budget_usd`."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task": {"type": "string"},
                    "preset": {"type": "string", "enum": ["explore", "shell"]},
                    "tools": {"type": "array", "items": {"type": "string"}},
                    "budget_usd": {"type": "number", "minimum": 0},
                    "background": {"type": "boolean"}
                },
                "required": ["task"]
            }),
            deferred: true,
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Parallel,
        }
    }

    fn subject(&self, input: &Value) -> String {
        Self::preset(input).map_or_else(|_| "?".into(), |p| p.name.to_string())
    }

    /// The riskiest tool the child may use (plan.md §1.11: "inherits max
    /// of its tools").
    fn risk(&self, input: &Value) -> Risk {
        let Ok(preset) = Self::preset(input) else {
            return Risk::Exec;
        };
        self.tools_for(preset, input)
            .iter()
            .map(|t| t.spec().risk)
            .max_by_key(|r| rank(*r))
            .unwrap_or(Risk::ReadOnly)
    }

    async fn call(&self, input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        let task_text = input
            .get("task")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| ToolError::Denied {
                why: "missing or empty \"task\"".into(),
            })?
            .to_string();
        let preset = Self::preset(&input)?;
        let tools = self.tools_for(preset, &input);
        let tier = self.parent.config.jobs.tier_for(preset.job);
        let mut config = self.parent.config.clone();
        config.budget.session_usd = slice(
            config.budget.session_usd,
            self.parent.spent().await,
            input.get("budget_usd").and_then(Value::as_f64),
        );
        config.core.max_turns = preset.max_turns;
        let child = self
            .parent
            .spawn_child(config, tools, preset.job, tier)
            .map_err(core_error)?;
        let Some(mut events) = child.events() else {
            return Err(ToolError::Io);
        };
        let task = TaskId::new();
        let label = format!("{}: {}", preset.name, first_line(&task_text));
        self.parent
            .emit(Event::TaskCreated { task, label, tier })
            .await
            .map_err(core_error)?;

        let runner = child.clone();
        let text = task_text.clone();
        let turn = tokio::spawn(async move {
            runner
                .submit(Submission::UserTurn {
                    text,
                    attachments: vec![],
                    confirm_think: false,
                })
                .await
        });
        let mut cost_usd = 0.0;
        let mut turns = 0u32;
        let mut interrupted = false;
        // `TurnDone` is the child's last event (turn.rs: nothing follows it).
        let outcome = loop {
            tokio::select! {
                _ = cx.cancel.cancelled(), if !interrupted => {
                    interrupted = true;
                    child.interrupt();
                }
                ev = events.recv() => match ev {
                    Some(Event::Usage { usage, .. }) => {
                        cost_usd += usage.cost_usd;
                        turns += 1;
                    }
                    Some(Event::ToolCallRequested { call }) => {
                        let _ = cx.output.send(format!("[{}] {}\n", preset.name, call.name)).await;
                    }
                    Some(Event::TurnDone { .. }) => break Ok(()),
                    Some(_) => {}
                    None => break Err(ToolError::Io),
                },
            }
        };
        if let Ok(Err(e)) = turn.await {
            return Err(core_error(e));
        }
        outcome?;
        if budget::counts(tier, self.parent.config.budget.cheap_counts) {
            self.parent.add_spend(cost_usd).await;
        }

        let mut result = child
            .history()
            .await
            .into_iter()
            .rev()
            .find(|m| m.role == Role::Assistant)
            .and_then(|m| {
                m.content.into_iter().find_map(|c| match c {
                    Content::Text { text } => Some(text),
                    _ => None,
                })
            })
            .unwrap_or_else(|| "(the subagent produced no answer)".to_string());
        let mut summarised = false;
        if result.len() / 4 > preset.result_cap_tokens {
            if let Some(short) = summarize(&self.parent, &result, preset.result_cap_tokens).await {
                result = short;
                summarised = true;
            } else {
                result.truncate(preset.result_cap_tokens * 4);
                result.push_str("\n[cut at the result cap]");
            }
        }
        self.parent
            .emit(Event::TaskCompleted {
                task,
                result_item: ItemId::new(),
                cost_usd,
            })
            .await
            .map_err(core_error)?;
        Ok(ToolOutput {
            text: result,
            is_error: false,
            diff: None,
            structured: Some(json!({
                "task": task,
                "preset": preset.name,
                "turns": turns,
                "cost_usd": cost_usd,
                "summarised": summarised,
            })),
        })
    }
}

/// One `Job::Summarize` call on its tier, recorded in the ledger like any
/// other request; `None` when the provider fails, so the caller falls
/// back to a cut.
async fn summarize(parent: &Session, text: &str, cap_tokens: usize) -> Option<String> {
    let tier = parent.config.jobs.tier_for(Job::Summarize);
    let tc = parent.config.tiers.get(tier);
    let model = ModelId(tc.model.clone());
    let req = Request {
        tier,
        job: Job::Summarize,
        model: model.clone(),
        system: vec![SystemBlock {
            text: format!(
                "Summarise the subagent result below in at most {cap_tokens} tokens. Keep \
                 file paths, line numbers, identifiers and exact error text; drop narration."
            ),
            cache: false,
        }],
        tools: vec![],
        messages: vec![Message {
            role: Role::User,
            content: vec![Content::Text {
                text: text.to_string(),
            }],
        }],
        effort: tc.effort,
        max_tokens: tc.max_tokens,
        thinking: tc.thinking,
        cache_breakpoints: vec![],
        stop_sequences: vec![],
    };
    let (tx, mut rx) = mpsc::channel(64);
    let provider = parent.provider.clone();
    let cancel = parent.cancel_token();
    let join = tokio::spawn(async move { provider.stream(req, tx, cancel).await });
    let mut out = String::new();
    while let Some(ev) = rx.recv().await {
        if let ProviderEvent::TextDelta { text } = ev {
            out.push_str(&text);
        }
    }
    let usage = join.await.ok()?.ok()?;
    parent
        .store
        .usage_insert(&cox_protocol::UsageRow {
            session_id: parent.id,
            turn: 0,
            job: Job::Summarize,
            tier,
            provider: parent.provider.id(),
            model,
            usage,
        })
        .ok()?;
    if budget::counts(tier, parent.config.budget.cheap_counts) {
        parent.add_spend(usage.cost_usd).await;
    }
    (!out.trim().is_empty()).then_some(out)
}

fn rank(r: Risk) -> u8 {
    match r {
        Risk::ReadOnly => 0,
        Risk::Write => 1,
        Risk::Exec => 2,
        Risk::Destructive => 3,
    }
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    line.chars().take(60).collect()
}

fn core_error(e: CoreError) -> ToolError {
    ToolError::Denied {
        why: format!("subagent failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subagent_budget_is_a_slice_of_parent() {
        assert_eq!(slice(4.0, 0.0, None), 1.0, "a quarter by default");
        assert_eq!(slice(4.0, 3.5, None), 0.125);
        assert_eq!(slice(4.0, 1.0, Some(10.0)), 3.0, "never more than remains");
        assert_eq!(slice(4.0, 1.0, Some(0.5)), 0.5);
        assert_eq!(
            slice(1.0, 2.0, Some(1.0)),
            0.0,
            "nothing left, nothing granted"
        );
    }

    #[test]
    fn subagent_presets_are_explore_and_shell() {
        let explore = AgentTool::preset(&json!({}));
        assert_eq!(
            explore.map(|p| (p.name, p.read_only)),
            Ok(("explore", true))
        );
        let shell = AgentTool::preset(&json!({"preset": "shell"}));
        assert_eq!(shell.map(|p| (p.name, p.read_only)), Ok(("shell", false)));
        assert!(AgentTool::preset(&json!({"preset": "nope"})).is_err());
    }
}
