//! Subagents (plan.md T3.9): the `agent` tool runs one task in a child
//! `Session` on its own tier with a tool allowlist, a budget slice and a
//! result cap. It lives in `cox-core` rather than `cox-tools` because a
//! child session *is* the loop, not I/O, and `cox-tools` may not depend on
//! this crate; the presets are plain data here for the same reason.
//!
//! T34.1: `preset` also resolves a discovered `AgentDef` (`.cox/agents`,
//! `.claude/agents`) by name, so a custom definition dispatches exactly
//! like `explore`/`shell`. `AgentDef`/`tier_for` live in
//! `cox_protocol::agent`, not `cox-ext` (which reads the filesystem and
//! this crate may not depend on): the surface
//! (`crates/cox/src/session.rs`) runs discovery once at session build and
//! hands the result to `Session::set_agent_defs`, keeping this crate
//! I/O-free. A custom preset's usage rows are tagged `Job::Agent`; its own
//! `tier`/`model` decides the actual tier, not the job.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use cox_protocol::errors::{CoreError, ToolError};
use cox_protocol::ids::{ItemId, TaskId};
use cox_protocol::traits::{Tool, ToolCx, Worktree};
use cox_protocol::types::{
    Concurrency, Content, DecidedBy, Decision, Event, HookEvent, HookOutcome, Job, Message,
    ModelId, ProviderEvent, Request, Risk, Role, Source, Submission, SystemBlock, Tier, ToolCall,
    ToolOutput, ToolSpec, Why,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::budget;
use crate::hooks;
use crate::session::Session;
use crate::tasks::cost_detail;

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

/// A generic, non-read-only default for a discovered `AgentDef`: the
/// same shape as [`SHELL`], since a custom preset's own `tools:` (not
/// `read_only`) is what narrows it.
const CUSTOM_MAX_TURNS: u32 = SHELL.max_turns;
const CUSTOM_RESULT_CAP_TOKENS: usize = SHELL.result_cap_tokens;

/// A dispatch target flattened from either a built-in [`Preset`] or a
/// discovered `AgentDef`, so `tools_for`/`call` match on it once instead
/// of on the source everywhere they need a field.
#[derive(Debug)]
struct Resolved {
    name: String,
    job: Job,
    /// `None` means every parent tool (an `AgentDef` with an empty
    /// `tools:`); built-in presets always name theirs explicitly.
    tools: Option<Vec<String>>,
    read_only: bool,
    max_turns: u32,
    result_cap_tokens: usize,
    /// The tier this dispatch would run at with no `tier` override: the
    /// job's configured tier for a built-in preset, or the def's own
    /// `model`/`tier_for`, falling back to the parent's tier when the
    /// model is `inherit` or absent (`cox_protocol::agent::tier_for`).
    natural_tier: Tier,
}

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
    /// Children spawned so far; numbers their names (`explore-2`, T27.2).
    spawned: AtomicU32,
}

impl AgentTool {
    pub(crate) fn new(parent: Session) -> Self {
        Self {
            parent,
            spawned: AtomicU32::new(0),
        }
    }

    /// Discovered names not already shadowed by a built-in preset — used
    /// both in the tool description and in the "unknown preset" error, so
    /// the two never disagree about what is dispatchable.
    fn custom_names(&self) -> Vec<String> {
        self.parent
            .agent_defs()
            .iter()
            .map(|d| d.name.clone())
            .filter(|n| !PRESETS.iter().any(|p| p.name == n))
            .collect()
    }

    /// Built-in `PRESETS` first (unchanged behaviour for `explore`/`shell`,
    /// even if a same-named file is discovered), then a discovered
    /// `AgentDef` by exact name; a miss lists both.
    fn resolve(&self, input: &Value) -> Result<Resolved, ToolError> {
        let name = input
            .get("preset")
            .and_then(Value::as_str)
            .unwrap_or(EXPLORE.name);
        if let Some(p) = PRESETS.iter().copied().find(|p| p.name == name) {
            return Ok(Resolved {
                name: p.name.to_string(),
                job: p.job,
                tools: Some(p.tools.iter().map(|s| s.to_string()).collect()),
                read_only: p.read_only,
                max_turns: p.max_turns,
                result_cap_tokens: p.result_cap_tokens,
                natural_tier: self.parent.config.jobs.tier_for(p.job),
            });
        }
        if let Some(def) = self.parent.agent_defs().iter().find(|d| d.name == name) {
            return Ok(Resolved {
                name: def.name.clone(),
                job: Job::Agent,
                tools: (!def.tools.is_empty()).then(|| def.tools.clone()),
                read_only: false,
                max_turns: CUSTOM_MAX_TURNS,
                result_cap_tokens: CUSTOM_RESULT_CAP_TOKENS,
                natural_tier: cox_protocol::agent::tier_for(def.model.as_deref())
                    .unwrap_or(self.parent.tier),
            });
        }
        let mut names: Vec<String> = PRESETS.iter().map(|p| p.name.to_string()).collect();
        names.extend(self.custom_names());
        Err(ToolError::Denied {
            why: format!(
                "unknown agent preset {name:?}; available presets: {}",
                names.join(", ")
            ),
        })
    }

    /// The `tier` a run at `resolved.natural_tier` may be asked to switch
    /// to: only down (D5 "never up"), and only to a name the router knows.
    fn resolve_tier(&self, resolved: &Resolved, input: &Value) -> Result<Tier, ToolError> {
        let Some(raw) = input.get("tier").and_then(Value::as_str) else {
            return Ok(resolved.natural_tier);
        };
        let requested = parse_tier(raw).ok_or_else(|| ToolError::Denied {
            why: format!("unknown tier {raw:?}; use cheap, code or think"),
        })?;
        let clamped = if tier_rank(requested) <= tier_rank(resolved.natural_tier) {
            requested
        } else {
            resolved.natural_tier
        };
        Ok(clamped)
    }

    /// The parent's tools this call may hand to the child: the resolved
    /// allowlist (or the call's `tools`, narrowed to it for a read-only
    /// preset), never `agent` itself.
    fn tools_for(&self, resolved: &Resolved, input: &Value) -> Vec<Arc<dyn Tool>> {
        let wanted: Option<Vec<String>> = input
            .get("tools")
            .and_then(Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .or_else(|| resolved.tools.clone());
        self.parent
            .tools
            .iter()
            .filter(|t| {
                let spec = t.spec();
                spec.name != "agent"
                    && wanted.as_ref().is_none_or(|w| w.contains(&spec.name))
                    && (!resolved.read_only || spec.risk == Risk::ReadOnly)
            })
            .cloned()
            .collect()
    }
}

/// `cheap` < `code` < `think`, for the "never up" clamp (D5); `Tier` has
/// no `Ord` of its own because nothing else needs to compare tiers.
fn tier_rank(t: Tier) -> u8 {
    match t {
        Tier::Cheap => 0,
        Tier::Code => 1,
        Tier::Think => 2,
    }
}

fn parse_tier(s: &str) -> Option<Tier> {
    match s {
        "cheap" => Some(Tier::Cheap),
        "code" => Some(Tier::Code),
        "think" => Some(Tier::Think),
        _ => None,
    }
}

#[async_trait]
impl Tool for AgentTool {
    fn spec(&self) -> ToolSpec {
        let mut description = "Delegate a self-contained task to a subagent that runs on the \
                cheap tier with its own tool set and budget, and returns only its answer. \
                Presets: `explore` (read-only file tools, answer ≤ 1k tokens) for \"find \
                where X is handled and report file:line\", `shell` (bash, web_fetch) for \
                builds, test runs and HTTP calls whose full output you do not need. Pass \
                `task` with everything the subagent needs to know; it does not see this \
                conversation. Optional: `tools` to narrow the tool list, `tier` (`cheap`, \
                `code` or `think`) to run the task on a cheaper tier than its preset's \
                default — never a more expensive one, `budget_usd`, \
                `isolation: \"worktree\"` to run the task in its own git worktree and \
                branch (named after the task id) so its edits never touch this checkout; \
                the answer then ends with the worktree path and branch."
            .to_string();
        let custom = self.custom_names();
        if !custom.is_empty() {
            description.push_str(&format!(
                " Custom presets from `.cox/agents`/`.claude/agents`: {}.",
                custom.join(", ")
            ));
        }
        ToolSpec {
            name: "agent".to_string(),
            description,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task": {"type": "string"},
                    "preset": {"type": "string"},
                    "tier": {"type": "string", "enum": ["cheap", "code", "think"]},
                    "tools": {"type": "array", "items": {"type": "string"}},
                    "budget_usd": {"type": "number", "minimum": 0},
                    "background": {"type": "boolean"},
                    "isolation": {"type": "string", "enum": ["none", "worktree"]}
                },
                "required": ["task"]
            }),
            deferred: true,
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Parallel,
        }
    }

    fn subject(&self, input: &Value) -> String {
        self.resolve(input).map_or_else(|_| "?".into(), |r| r.name)
    }

    /// The riskiest tool the child may use (plan.md §1.11: "inherits max
    /// of its tools").
    fn risk(&self, input: &Value) -> Risk {
        let Ok(resolved) = self.resolve(input) else {
            return Risk::Exec;
        };
        self.tools_for(&resolved, input)
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
        let preset = self.resolve(&input)?;
        let tools = self.tools_for(&preset, &input);
        let tier = self.resolve_tier(&preset, &input)?;
        let mut config = self.parent.config.clone();
        config.budget.session_usd = slice(
            config.budget.session_usd,
            self.parent.spent().await,
            input.get("budget_usd").and_then(Value::as_f64),
        );
        config.core.max_turns = preset.max_turns;
        let task = TaskId::new();
        // T27.3: the child works in `_worktrees/<repo>-<task>` on branch
        // `<task>`, with the main checkout as a second root so it can still
        // read what the parent sees; the worktree outlives the task for
        // the user to merge.
        let worktree = match input.get("isolation").and_then(Value::as_str) {
            Some("worktree") => {
                let worktrees = self.parent.worktrees().ok_or_else(|| ToolError::Denied {
                    why: "worktree isolation is not available on this surface".into(),
                })?;
                let owner = format!("cox / {}", self.parent.id);
                let wt = worktrees
                    .add(&self.parent.cwd, &task.to_string(), &owner)
                    .await
                    .map_err(|e| ToolError::Denied {
                        why: format!("worktree: {e}"),
                    })?;
                config.core.workspace_roots = vec![wt.path.clone(), wt.main.clone()];
                Some(wt)
            }
            Some("none") | None => None,
            Some(other) => {
                return Err(ToolError::Denied {
                    why: format!("unknown isolation {other:?}; use none or worktree"),
                });
            }
        };
        let child = self
            .parent
            .spawn_child(
                config,
                tools,
                preset.job,
                tier,
                worktree.as_ref().map(|wt| wt.path.clone()),
            )
            .map_err(core_error)?;
        if let Some(wt) = &worktree {
            child.set_writable_roots(vec![wt.path.clone()]);
        }
        let Some(events) = child.events() else {
            return Err(ToolError::Io);
        };
        let label = format!("{}: {}", preset.name, first_line(&task_text));
        let name = format!(
            "{}-{}",
            preset.name,
            self.spawned.fetch_add(1, Ordering::Relaxed) + 1
        );
        // SubagentStart gates both paths; a Block means the task never existed.
        if let HookOutcome::Block { reason } = hooks::fire(
            &self.parent,
            HookEvent::SubagentStart,
            json!({"task": task.to_string(), "label": label, "preset": preset.name}),
        )
        .await
        {
            return Err(ToolError::Denied {
                why: format!("subagent blocked by hook: {reason}"),
            });
        }
        self.parent
            .emit(Event::TaskCreated {
                task,
                label: label.clone(),
                tier,
            })
            .await
            .map_err(core_error)?;
        self.parent
            .register_task(task, label.clone(), tier, crate::tasks::TaskKind::Agent)
            .await;
        if input
            .get("background")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let parent = self.parent.clone();
            let (cancel, progress) = (cx.cancel.clone(), cx.output.clone());
            let bg_label = label.clone();
            // `preset` itself is not `Copy` (T34.1), so only what the child
            // run needs is cloned into the `async move` closure; `preset`
            // stays intact for the `structured` reply built after `spawn`.
            let preset_name = preset.name.clone();
            let result_cap_tokens = preset.result_cap_tokens;
            tokio::spawn(async move {
                let io = RunIo {
                    name,
                    preset_name,
                    result_cap_tokens,
                    tier,
                    events,
                    cancel,
                    progress,
                    worktree,
                };
                let outcome = run_task(&parent, child, task_text, io).await;
                let (answer, cost_usd) = match outcome {
                    Ok(o) => (o.answer, o.cost_usd),
                    Err(e) => (format!("task failed: {e}"), 0.0),
                };
                parent.complete_task(task).await;
                let _ = parent
                    .emit(Event::TaskCompleted {
                        task,
                        result_item: ItemId::new(),
                        cost_usd,
                        exit_code: None,
                        archive: None,
                    })
                    .await;
                let _ = parent
                    .publish_task_result(task, &bg_label, &answer, &cost_detail(cost_usd))
                    .await;
                let _ = hooks::fire(
                    &parent,
                    HookEvent::SubagentStop,
                    json!({"task": task.to_string(), "label": bg_label}),
                )
                .await;
            });
            return Ok(ToolOutput {
                text: format!(
                    "background task {task} started: {label}\n\
                     its result will arrive as a notice, not in this turn"
                ),
                is_error: false,
                diff: None,
                structured: Some(json!({
                    "task": task,
                    "preset": preset.name,
                    "background": true,
                })),
            });
        }

        let outcome = run_task(
            &self.parent,
            child,
            task_text,
            RunIo {
                name,
                preset_name: preset.name.clone(),
                result_cap_tokens: preset.result_cap_tokens,
                tier,
                events,
                cancel: cx.cancel.clone(),
                progress: cx.output.clone(),
                worktree,
            },
        )
        .await;
        // A failed task still finished: drop the registry entry and close
        // the Created/Completed pair so `/tasks` never shows a ghost.
        let cost_usd = outcome.as_ref().map(|o| o.cost_usd).unwrap_or(0.0);
        self.parent.complete_task(task).await;
        self.parent
            .emit(Event::TaskCompleted {
                task,
                result_item: ItemId::new(),
                cost_usd,
                exit_code: None,
                archive: None,
            })
            .await
            .map_err(core_error)?;
        let _ = hooks::fire(
            &self.parent,
            HookEvent::SubagentStop,
            json!({"task": task.to_string(), "label": label}),
        )
        .await;
        let outcome = outcome?;
        Ok(ToolOutput {
            text: outcome.answer,
            is_error: false,
            diff: None,
            structured: Some(json!({
                "task": task,
                "preset": preset.name,
                "turns": outcome.turns,
                "cost_usd": outcome.cost_usd,
                "summarised": outcome.summarised,
            })),
        })
    }
}

/// T27.2: the child's own event stream has no surface, so its prompt is
/// raised on the parent's, labelled with the agent, and the parent's
/// `Submission::Approve` for that call is handed back to the child. If the
/// child is cancelled first it answers itself `Deny`, and the relayed
/// `ApprovalDecided` closes the prompt.
async fn relay_approval(parent: &Session, child: &Session, call: ToolCall, why: Why, io: &RunIo) {
    let id = call.id;
    let decision = parent.relay_decision(id).await;
    let source = Source {
        session: child.id(),
        agent: Some(io.name.clone()),
        preset: Some(io.preset_name.clone()),
    };
    let asked = parent.emit(Event::ApprovalRequired {
        call,
        why,
        source: Some(source),
    });
    if asked.await.is_err() {
        return;
    }
    let child = child.clone();
    tokio::spawn(async move {
        let decision = decision.await.unwrap_or(Decision::Deny {
            reason: "session closed".into(),
        });
        let _ = child
            .submit(Submission::Approve {
                call_id: id,
                decision,
            })
            .await;
    });
}

/// What one child run produced, foreground or background.
struct TaskOutcome {
    answer: String,
    cost_usd: f64,
    turns: u32,
    summarised: bool,
}

/// How one child run is driven and observed.
struct RunIo {
    /// `<preset>-<n>`: how its approvals are labelled on the parent's surface.
    name: String,
    /// The dispatched preset/def's own name (T34.1: `Resolved` is not
    /// `Copy`, unlike the old `Preset`, so this is cloned out of it once
    /// rather than moved, which would strand the caller's own copy).
    preset_name: String,
    result_cap_tokens: usize,
    tier: Tier,
    events: mpsc::Receiver<Event>,
    cancel: CancellationToken,
    progress: mpsc::Sender<String>,
    /// The child's worktree, named in the answer so the parent can merge it.
    worktree: Option<Worktree>,
}

/// Drives the child's turn and distills its answer (shared by the
/// foreground call and the background task): accumulates cost, streams
/// progress lines, charges the parent, caps the answer.
async fn run_task(
    parent: &Session,
    child: Session,
    task_text: String,
    mut io: RunIo,
) -> Result<TaskOutcome, ToolError> {
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
            _ = io.cancel.cancelled(), if !interrupted => {
                interrupted = true;
                child.interrupt();
            }
            ev = io.events.recv() => match ev {
                Some(Event::Usage { usage, .. }) => {
                    cost_usd += usage.cost_usd;
                    turns += 1;
                }
                Some(Event::ToolCallRequested { call }) => {
                    let _ = io
                        .progress
                        .send(format!("[{}] {}\n", io.preset_name, call.name))
                        .await;
                }
                Some(Event::TurnDone { .. }) => break Ok(()),
                Some(Event::ApprovalRequired { call, why, .. }) => {
                    relay_approval(parent, &child, call, why, &io).await;
                }
                // Closes the prompt the relay opened on the parent's surface;
                // a rule's verdict never opened one.
                Some(ev @ Event::ApprovalDecided { by: DecidedBy::User, .. }) => {
                    let _ = parent.emit(ev).await;
                }
                // T34.5 turns a child's own `TaskMessage` (`to: "parent"` or a
                // sibling, SM§3) into the parent's `Submission::TaskMessage`
                // instead of dropping it here.
                Some(Event::TaskMessage { .. }) => {}
                Some(_) => {}
                None => break Err(ToolError::Io),
            },
        }
    };
    if let Ok(Err(e)) = turn.await {
        return Err(core_error(e));
    }
    outcome?;
    if budget::counts(io.tier, parent.config.budget.cheap_counts) {
        parent.add_spend(cost_usd).await;
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
    if result.len() / 4 > io.result_cap_tokens {
        if let Some(short) = summarize(parent, &result, io.result_cap_tokens).await {
            result = short;
            summarised = true;
        } else {
            result.truncate(io.result_cap_tokens * 4);
            result.push_str("\n[cut at the result cap]");
        }
    }
    // After the cap, so the trailer the parent merges from is never cut.
    if let Some(wt) = &io.worktree {
        result.push_str(&format!(
            "\n[worktree {}, branch {}]",
            wt.path.display(),
            wt.branch
        ));
    }
    Ok(TaskOutcome {
        answer: result,
        cost_usd,
        turns,
        summarised,
    })
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
            effort: Some(tc.effort),
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

pub(crate) fn first_line(text: &str) -> String {
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
    use std::path::PathBuf;

    use cox_protocol::agent::AgentDef;

    use super::*;

    /// An `AgentTool` over a throwaway session, for `resolve`'s own claims
    /// (unit-level, no turn ever runs). T34.1 made `resolve`/`preset`
    /// resolution an instance method — it now reads discovered defs off
    /// the parent session — so this replaces the old bare-function call.
    fn test_agent_tool(defs: Vec<AgentDef>) -> AgentTool {
        let store = Arc::new(crate::MemoryStore::new());
        let provider =
            Arc::new(cox_provider::scripted::Scripted::from_toml("", "").expect("scripted"));
        let session = Session::new(
            cox_protocol::Config::default(),
            provider,
            vec![],
            store.clone(),
            store,
            PathBuf::from("/tmp/cox-subagent-unit"),
        )
        .expect("session");
        session.set_agent_defs(defs);
        AgentTool::new(session)
    }

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
        let tool = test_agent_tool(vec![]);
        let explore = tool.resolve(&json!({}));
        assert_eq!(
            explore.map(|p| (p.name, p.read_only)),
            Ok(("explore".to_string(), true))
        );
        let shell = tool.resolve(&json!({"preset": "shell"}));
        assert_eq!(
            shell.map(|p| (p.name, p.read_only)),
            Ok(("shell".to_string(), false))
        );
        assert!(tool.resolve(&json!({"preset": "nope"})).is_err());
    }

    /// T34.1: an unrecognised name is denied, and the error names both the
    /// built-in presets and whatever `.cox/agents`/`.claude/agents`
    /// discovered. Unit-level (like the test above), not a full turn: a
    /// failed `resolve` makes `risk()` fall back to `Exec` (pre-existing,
    /// unchanged behaviour — an unresolvable call could be anything), which
    /// would otherwise need an approval answer before `call()`'s own
    /// friendlier text ever surfaces as a tool result.
    #[test]
    fn agent_unknown_preset_lists_builtin_and_discovered_names_in_error() {
        let tool = test_agent_tool(vec![AgentDef {
            name: "reviewer".into(),
            description: "reviews a diff".into(),
            tools: vec!["read".into()],
            model: Some("haiku".into()),
            path: PathBuf::from("<test>/.cox/agents/reviewer.md"),
            body: "You review changes for correctness.".into(),
        }]);
        let err = tool
            .resolve(&json!({"preset": "nope"}))
            .expect_err("unknown preset");
        let ToolError::Denied { why } = err else {
            panic!("expected Denied, got {err:?}");
        };
        for name in ["explore", "shell", "reviewer"] {
            assert!(why.contains(name), "{name:?} missing from {why:?}");
        }
    }
}
