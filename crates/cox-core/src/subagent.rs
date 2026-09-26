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
//!
//! T34.5: a subagent keeps answering follow-ups (SM§2, §3, §5). The parent
//! routes every message — a child never holds a sibling's handle — and
//! owns the causal hop count; delivery is always a whole new turn.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use cox_protocol::errors::{CoreError, ToolError};
use cox_protocol::ids::{ItemId, SessionId, TaskId};
use cox_protocol::traits::{Tool, ToolCx, Worktree};
use cox_protocol::types::{
    Concurrency, Content, DecidedBy, Decision, Event, HookEvent, HookOutcome, Job, Level, Message,
    ModelId, ProviderEvent, Request, Risk, Role, Source, Submission, SystemBlock, Tier, ToolCall,
    ToolOutput, ToolSpec, Why,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::budget;
use crate::hooks;
use crate::rollout::History;
use crate::session::Session;
use crate::tasks::{Queued, cost_detail, message_line};

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
        let spec = Spec {
            config,
            tools,
            job: preset.job,
            tier,
            worktree,
            name: format!(
                "{}-{}",
                preset.name,
                self.spawned.fetch_add(1, Ordering::Relaxed) + 1
            ),
            preset_name: preset.name.clone(),
            result_cap_tokens: preset.result_cap_tokens,
            label: format!("{}: {}", preset.name, first_line(&task_text)),
        };
        let child = spawn(&self.parent, &spec, None).map_err(core_error)?;
        let Some(events) = child.events() else {
            return Err(ToolError::Io);
        };
        let label = spec.label.clone();
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
        self.parent.track_child(task).await;
        let mut io = RunIo {
            task,
            spec,
            events,
            cancel: cx.cancel.clone(),
            progress: cx.output.clone(),
        };
        if input
            .get("background")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            tokio::spawn(drive(self.parent.clone(), task, child, task_text, io));
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

        let session = child.id();
        let outcome = run_task(&self.parent, child, task_text, &mut io).await;
        // A failed task still finished: drop the registry entry and close
        // the Created/Completed pair so `/tasks` never shows a ghost.
        let cost_usd = outcome.as_ref().map(|o| o.cost_usd).unwrap_or(0.0);
        self.parent.complete_task(task).await;
        let completed = self
            .parent
            .emit(Event::TaskCompleted {
                task,
                result_item: ItemId::new(),
                cost_usd,
                exit_code: None,
                archive: None,
            })
            .await;
        let _ = hooks::fire(
            &self.parent,
            HookEvent::SubagentStop,
            json!({"task": task.to_string(), "label": label}),
        )
        .await;
        io.spec.charge(cost_usd);
        let parked = self.parent.park_child(
            task,
            Dormant {
                session,
                spec: io.spec,
            },
        );
        if let Some((dormant, next)) = parked.await {
            tokio::spawn(wake(self.parent.clone(), task, dormant, next));
        }
        completed.map_err(core_error)?;
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
        agent: Some(io.spec.name.clone()),
        preset: Some(io.spec.preset_name.clone()),
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

/// Everything a child needs to run again after it finished (SM§2): the
/// same job, tier, tools, parent and what is left of its budget slice, and
/// how its runs are labelled and capped.
#[derive(Clone)]
pub(crate) struct Spec {
    config: cox_protocol::Config,
    tools: Vec<Arc<dyn Tool>>,
    job: Job,
    tier: Tier,
    /// The child's worktree, named in the answer so the parent can merge it.
    worktree: Option<Worktree>,
    /// `<preset>-<n>`: how its approvals are labelled on the parent's surface.
    name: String,
    preset_name: String,
    result_cap_tokens: usize,
    label: String,
}

impl Spec {
    /// A resumed run gets what the finished ones left of the slice.
    fn charge(&mut self, cost_usd: f64) {
        let left = self.config.budget.session_usd - cost_usd;
        self.config.budget.session_usd = left.max(0.0);
    }
}

/// A finished child: its stored session, resumed from the rollout.
pub(crate) struct Dormant {
    session: SessionId,
    spec: Spec,
}

/// The child session for `spec`, fresh or restored from `resume`.
fn spawn(
    parent: &Session,
    spec: &Spec,
    resume: Option<(SessionId, History)>,
) -> Result<Session, CoreError> {
    let cwd = spec.worktree.as_ref().map(|wt| wt.path.clone());
    let (config, tools) = (spec.config.clone(), spec.tools.clone());
    let child = parent.spawn_child(config, tools, spec.job, spec.tier, cwd, resume)?;
    if let Some(wt) = &spec.worktree {
        child.set_writable_roots(vec![wt.path.clone()]);
    }
    Ok(child)
}

/// Runs a background child until nothing is left for it to answer: each
/// run reports like any background task, then a message that arrived
/// meanwhile resumes it rather than waiting for another `wake` (SM§2).
async fn drive(parent: Session, task: TaskId, mut child: Session, text: String, mut io: RunIo) {
    let mut text = text;
    loop {
        let session = child.id();
        let (answer, cost_usd) = match run_task(&parent, child, text, &mut io).await {
            Ok(o) => (o.answer, o.cost_usd),
            Err(e) => (format!("task failed: {e}"), 0.0),
        };
        let label = io.spec.label.clone();
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
            .publish_task_result(task, &label, &answer, &cost_detail(cost_usd))
            .await;
        let stop = json!({"task": task.to_string(), "label": label});
        let _ = hooks::fire(&parent, HookEvent::SubagentStop, stop).await;
        io.spec.charge(cost_usd);
        let spec = io.spec.clone();
        let Some((dormant, next)) = parent.park_child(task, Dormant { session, spec }).await else {
            return;
        };
        text = message_line(next.from, &next.text);
        child = match restart(&parent, task, &dormant, &mut io).await {
            Ok(child) => child,
            Err(e) => return lost(&parent, task, e).await,
        };
    }
}

/// A message woke a finished child. Boxed so `Session::deliver`, which
/// spawns it, does not need its own future's `Send` proof in a cycle.
pub(crate) fn wake(
    parent: Session,
    task: TaskId,
    dormant: Box<Dormant>,
    next: Queued,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async move {
        let (progress, _) = mpsc::channel(1);
        let (_, events) = mpsc::channel(1);
        let cancel = parent.cancel_token();
        let spec = dormant.spec.clone();
        let mut io = RunIo {
            task,
            spec,
            events,
            cancel,
            progress,
        };
        match restart(&parent, task, &dormant, &mut io).await {
            Ok(child) => drive(parent, task, child, message_line(next.from, &next.text), io).await,
            Err(e) => lost(&parent, task, e).await,
        }
    })
}

/// Restores a finished child from its rollout with its own job, tier,
/// parent and budget slice, and announces it as running again.
async fn restart(
    parent: &Session,
    task: TaskId,
    dormant: &Dormant,
    io: &mut RunIo,
) -> Result<Session, CoreError> {
    let events = parent
        .store
        .rollout_read(&dormant.session)
        .map_err(|error| CoreError::Store { error })?;
    let history = History::from_events(&events);
    let child = spawn(parent, &dormant.spec, Some((dormant.session, history)))?;
    io.events = child.events().unwrap_or_else(|| mpsc::channel(1).1);
    io.cancel = parent.cancel_token();
    let (label, tier) = (dormant.spec.label.clone(), dormant.spec.tier);
    parent
        .emit(Event::TaskCreated {
            task,
            label: label.clone(),
            tier,
        })
        .await?;
    parent
        .register_task(task, label, tier, crate::tasks::TaskKind::Agent)
        .await;
    Ok(child)
}

/// A child that could not be restored stops being addressable, loudly.
async fn lost(parent: &Session, task: TaskId, e: CoreError) {
    parent.forget_child(task).await;
    let text = format!("subagent task {task} could not resume: {e}");
    let _ = parent.notice(Level::Warn, text).await;
}

/// SM§5: hops a message may travel before the parent drops it, so an
/// A→B→A ping-pong stops. `MAX_MESSAGES_PER_TASK` is T34.6's, with the tool.
pub(crate) const MAX_HOPS: u32 = 4;

/// A child's `Event::TaskMessage`, routed by the parent only (SM§3): to its
/// own task id means "to the parent", anything else is a sibling reached
/// through the parent's own `Submission::TaskMessage`. `from` and `hop`
/// are the parent's, never what the child claimed.
pub(crate) async fn relay(
    parent: &Session,
    from: TaskId,
    to: TaskId,
    text: String,
) -> Result<(), CoreError> {
    let hop = parent.child_hop(from).await + 1;
    if hop > MAX_HOPS {
        let text = format!("message from task {from} to {to} dropped: hop limit {MAX_HOPS}");
        return parent.notice(Level::Warn, text).await;
    }
    if to == from {
        return parent.message_parent(from, hop, text).await;
    }
    let from = Some(from);
    let sub = Submission::TaskMessage {
        task: to,
        from,
        hop,
        text,
    };
    parent.submit(sub).await
}

/// How one child run is driven and observed.
struct RunIo {
    /// The child's own task id: its queue and hop in the parent's registry.
    task: TaskId,
    spec: Spec,
    events: mpsc::Receiver<Event>,
    cancel: CancellationToken,
    progress: mpsc::Sender<String>,
}

/// The child's next turn, on its own task so the relay loop keeps running.
fn submit_turn(child: &Session, text: String) -> JoinHandle<Result<(), CoreError>> {
    let runner = child.clone();
    tokio::spawn(async move {
        runner
            .submit(Submission::UserTurn {
                text,
                attachments: vec![],
                confirm_think: false,
            })
            .await
    })
}

/// Drives the child's turns and distills its answer (shared by the
/// foreground call and the background task): accumulates cost, streams
/// progress lines, charges the parent, caps the answer. A message queued
/// during a turn becomes the next turn once `TurnDone` closes this one.
async fn run_task(
    parent: &Session,
    child: Session,
    task_text: String,
    io: &mut RunIo,
) -> Result<TaskOutcome, ToolError> {
    let mut turn = submit_turn(&child, task_text);
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
                        .send(format!("[{}] {}\n", io.spec.preset_name, call.name))
                        .await;
                }
                Some(Event::TurnDone { .. }) => {
                    let Some(next) = parent.next_queued(io.task).await else {
                        break Ok(());
                    };
                    if let Ok(Err(e)) = (&mut turn).await {
                        break Err(core_error(e));
                    }
                    turn = submit_turn(&child, message_line(next.from, &next.text));
                }
                Some(Event::ApprovalRequired { call, why, .. }) => {
                    relay_approval(parent, &child, call, why, io).await;
                }
                // Closes the prompt the relay opened on the parent's surface;
                // a rule's verdict never opened one.
                Some(ev @ Event::ApprovalDecided { by: DecidedBy::User, .. }) => {
                    let _ = parent.emit(ev).await;
                }
                Some(Event::TaskMessage { task: to, text, .. }) => {
                    let _ = relay(parent, io.task, to, text).await;
                }
                Some(_) => {}
                None => break Err(ToolError::Io),
            },
        }
    };
    if let Ok(Err(e)) = turn.await {
        return Err(core_error(e));
    }
    outcome?;
    if budget::counts(io.spec.tier, parent.config.budget.cheap_counts) {
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
    let cap = io.spec.result_cap_tokens;
    if result.len() / 4 > cap {
        if let Some(short) = summarize(parent, &result, cap).await {
            result = short;
            summarised = true;
        } else {
            result.truncate(cap * 4);
            result.push_str("\n[cut at the result cap]");
        }
    }
    // After the cap, so the trailer the parent merges from is never cut.
    if let Some(wt) = &io.spec.worktree {
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

    use std::sync::{Mutex as StdMutex, OnceLock};
    use std::time::Duration;

    use cox_protocol::errors::ProviderError;
    use cox_protocol::traits::Provider;
    use cox_protocol::types::{Caps, ProviderId, Usage};
    use cox_provider::scripted::Scripted;

    use crate::tasks::Child;

    /// The script, plus each request's tier and text parts, so a test can
    /// see exactly what history a child turn was sent with.
    struct Recording {
        script: Scripted,
        seen: StdMutex<Vec<(Tier, Vec<String>)>>,
    }

    #[async_trait]
    impl Provider for Recording {
        fn id(&self) -> ProviderId {
            self.script.id()
        }
        fn capabilities(&self) -> Caps {
            self.script.capabilities()
        }
        async fn stream(
            &self,
            req: Request,
            sink: mpsc::Sender<ProviderEvent>,
            cancel: CancellationToken,
        ) -> Result<Usage, ProviderError> {
            let texts = req.messages.iter().flat_map(|m| &m.content);
            let texts = texts.filter_map(|c| match c {
                Content::Text { text } => Some(text.clone()),
                _ => None,
            });
            let row = (req.tier, texts.collect());
            self.seen.lock().expect("lock").push(row);
            self.script.stream(req, sink, cancel).await
        }
        async fn count_tokens(&self, req: &Request) -> Result<u32, ProviderError> {
            self.script.count_tokens(req).await
        }
    }

    impl Recording {
        /// The child's requests: `explore` runs on the cheap tier, the
        /// parent on `code`.
        fn child(&self) -> Vec<Vec<String>> {
            let seen = self.seen.lock().expect("lock");
            let rows = seen.iter().filter(|(tier, _)| *tier == Tier::Cheap);
            rows.map(|(_, texts)| texts.clone()).collect()
        }
    }

    /// Mid-turn, submits a message to the parent's one child.
    struct Poke(Arc<OnceLock<Session>>);

    #[async_trait]
    impl Tool for Poke {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "poke".into(),
                description: "messages the running child".into(),
                input_schema: json!({"type": "object"}),
                deferred: false,
                risk: Risk::ReadOnly,
                concurrency: Concurrency::Parallel,
            }
        }
        fn subject(&self, _input: &Value) -> String {
            "poke".into()
        }
        async fn call(&self, _input: Value, _cx: &ToolCx) -> Result<ToolOutput, ToolError> {
            let parent = self.0.get().ok_or(ToolError::Io)?;
            let task = *parent
                .inner
                .lock()
                .await
                .children
                .keys()
                .next()
                .ok_or(ToolError::Io)?;
            let sub = Submission::TaskMessage {
                task,
                from: None,
                hop: 0,
                text: "ping".into(),
            };
            parent.submit(sub).await.map_err(|_| ToolError::Io)?;
            Ok(ToolOutput {
                text: "poked".into(),
                is_error: false,
                diff: None,
                structured: None,
            })
        }
    }

    fn parent_with(toml: &str) -> (Session, Arc<Recording>, mpsc::Receiver<Event>) {
        let script = Scripted::from_toml(toml, "").expect("scenario");
        let seen = StdMutex::new(Vec::new());
        let provider = Arc::new(Recording { script, seen });
        let store = Arc::new(crate::MemoryStore::new());
        let slot = Arc::new(OnceLock::new());
        let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(Poke(slot.clone()))];
        let cwd = PathBuf::from("/tmp/cox-subagent-unit");
        let mut config = cox_protocol::Config::default();
        config.core.workspace_roots = vec![cwd.clone()];
        let session = Session::new(config, provider.clone(), tools, store.clone(), store, cwd)
            .expect("session");
        let _ = slot.set(session.clone());
        let rx = session.events().expect("events");
        (session, provider, rx)
    }

    async fn until(rx: &mut mpsc::Receiver<Event>, done: impl Fn(&Event) -> bool) -> Vec<Event> {
        let mut out = Vec::new();
        loop {
            let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
            let ev = ev.expect("event timeout").expect("stream closed");
            let stop = done(&ev);
            out.push(ev);
            if stop {
                return out;
            }
        }
    }

    /// One parent turn over `parent`'s script, drained to its `TurnDone`.
    async fn parent_turn(parent: &Session, rx: &mut mpsc::Receiver<Event>) -> Vec<Event> {
        let runner = parent.clone();
        let running = tokio::spawn(async move {
            let text = "go".to_string();
            let (attachments, confirm_think) = (vec![], false);
            let sub = Submission::UserTurn {
                text,
                attachments,
                confirm_think,
            };
            runner.submit(sub).await
        });
        let events = until(rx, |e| matches!(e, Event::TurnDone { .. })).await;
        running.await.expect("join").expect("turn");
        events
    }

    #[tokio::test]
    async fn task_message_reaches_a_still_running_subagent_after_its_current_turn() {
        let toml = r#"
[[turn]]
text = "delegating"
tool_calls = [{ name = "agent", input = { task = "work", tools = ["poke"] } }]
[[turn]]
text = "poking"
tool_calls = [{ name = "poke", input = {} }]
[[turn]]
text = "first answer"
[[turn]]
text = "got ping"
[[turn]]
text = "done"
"#;
        let (parent, seen, mut rx) = parent_with(toml);
        let events = parent_turn(&parent, &mut rx).await;

        let results: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::ToolCallDone { result, .. } => Some(result.visible.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(results, ["got ping"], "the child answered the message");
        assert!(events.iter().any(|e| matches!(
            e,
            Event::TaskMessage { from: None, hop: 0, text, .. } if text == "ping"
        )));
        let child = seen.child();
        assert_eq!(child.len(), 3, "poke, first answer, then the message turn");
        let ping = "[message from parent] ping";
        assert!(!child[1].iter().any(|t| t == ping), "never mid-turn");
        let last = &child[2];
        let at = last.iter().position(|t| t == ping).expect("delivered");
        assert_eq!(last[at - 1], "first answer", "after the whole turn");
    }

    /// Also the card's `resumed_subagent_keeps_its_parent_id_and_budget_slice`:
    /// the resumed request runs on the child's own tier, and the parked
    /// slice never grows.
    #[tokio::test]
    async fn task_message_to_a_finished_subagent_resumes_it_with_history_intact() {
        let toml = r#"
[[turn]]
text = "delegating"
tool_calls = [{ name = "agent", input = { task = "remember 42" } }]
[[turn]]
text = "noted"
[[turn]]
text = "done"
[[turn]]
text = "it was 42"
"#;
        let (parent, seen, mut rx) = parent_with(toml);
        let events = parent_turn(&parent, &mut rx).await;
        let task = events
            .iter()
            .find_map(|e| match e {
                Event::TaskCreated { task, .. } => Some(*task),
                _ => None,
            })
            .expect("task");
        let slice = match parent.inner.lock().await.children.get(&task) {
            Some(Child::Finished(d)) => d.spec.config.budget.session_usd,
            _ => panic!("a finished child is kept, not dropped"),
        };

        let sub = Submission::TaskMessage {
            task,
            from: None,
            hop: 0,
            text: "what number?".into(),
        };
        parent.submit(sub).await.expect("deliver");
        let woke = until(
            &mut rx,
            |e| matches!(e, Event::Notice { text, .. } if text.contains("finished")),
        )
        .await;
        assert!(woke.iter().any(|e| matches!(
            e,
            Event::TaskCreated { task: t, tier: Tier::Cheap, .. } if *t == task
        )));

        let child = seen.child();
        assert_eq!(
            child.len(),
            2,
            "the first run and the resumed one, both cheap"
        );
        assert_eq!(
            child[1],
            ["remember 42", "noted", "[message from parent] what number?"],
            "the resumed turn sees its whole history"
        );
        let history = parent.history().await;
        let pointer = history
            .iter()
            .flat_map(|m| &m.content)
            .any(|c| matches!(c, Content::Text { text } if text.contains("it was 42")));
        assert!(pointer, "the answer reaches the parent as a pointer line");
        match parent.inner.lock().await.children.get(&task) {
            Some(Child::Finished(d)) => {
                assert!(d.spec.config.budget.session_usd <= slice, "the same slice");
            }
            _ => panic!("parked again after the resumed run"),
        }
    }

    fn child_spec() -> Spec {
        Spec {
            config: cox_protocol::Config::default(),
            tools: vec![],
            job: Job::Explore,
            tier: Tier::Cheap,
            worktree: None,
            name: "explore-1".into(),
            preset_name: "explore".into(),
            result_cap_tokens: 1000,
            label: "explore: a".into(),
        }
    }

    #[tokio::test]
    async fn sibling_message_is_relayed_through_the_parent() {
        let (parent, _, mut rx) = parent_with("[[turn]]\ntext = \"sent\"\n");
        let (a, b) = (TaskId::new(), TaskId::new());
        parent.track_child(a).await;
        parent.track_child(b).await;
        let spec = child_spec();
        let child = spawn(&parent, &spec, None).expect("child");
        let events = child.events().expect("events");
        // The child claims a sender and a hop; the parent overrides both.
        for (to, text) in [(b, "hi b"), (a, "hi parent")] {
            let forged = Event::TaskMessage {
                task: to,
                from: None,
                hop: 99,
                text: text.into(),
            };
            child.emit(forged).await.expect("emit");
        }
        let (progress, _) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let task = a;
        let mut io = RunIo {
            task,
            spec,
            events,
            cancel,
            progress,
        };
        let out = run_task(&parent, child, "a".into(), &mut io).await;
        assert_eq!(out.map(|o| o.answer).ok().as_deref(), Some("sent"));

        let relayed = Queued {
            from: Some(a),
            hop: 1,
            text: "hi b".into(),
        };
        assert_eq!(parent.next_queued(b).await, Some(relayed));
        let parent_line = format!("[message from task {a}] hi parent");
        let history = parent.history().await;
        let line = history
            .iter()
            .flat_map(|m| &m.content)
            .any(|c| matches!(c, Content::Text { text } if *text == parent_line));
        assert!(line, "to its own task id means to the parent");
        let seen = until(
            &mut rx,
            |e| matches!(e, Event::TaskMessage { task, .. } if *task == a),
        )
        .await;
        assert!(seen.iter().any(|e| matches!(
            e,
            Event::TaskMessage { task, from: Some(f), hop: 1, .. } if *task == b && *f == a
        )));
    }

    #[tokio::test]
    async fn hop_limit_stops_a_ping_pong() {
        let (parent, _, _rx) = parent_with("");
        let (a, b, c) = (TaskId::new(), TaskId::new(), TaskId::new());
        for t in [a, b, c] {
            parent.track_child(t).await;
        }
        let (mut from, mut to) = (a, b);
        let mut delivered = 0;
        for _ in 0..10 {
            relay(&parent, from, to, "ping".into())
                .await
                .expect("relay");
            let Some(next) = parent.next_queued(to).await else {
                break;
            };
            delivered += 1;
            assert_eq!(next.hop, delivered, "each relay is one hop further");
            std::mem::swap(&mut from, &mut to);
        }
        assert_eq!(delivered, MAX_HOPS, "A→B→A stops after MAX_HOPS relays");
        // An independent message, sent during a turn its task started, is hop 1.
        relay(&parent, c, b, "hello".into()).await.expect("relay");
        assert_eq!(parent.next_queued(b).await.map(|q| q.hop), Some(1));
    }
}
