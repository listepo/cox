//! Compaction (plan.md §1.10, T8.1): replaces every turn but the last
//! `keep_turns` with one summary from the `compact` job. Append-only (D6f):
//! the rollout keeps every original event and `Compacted.dropped` says which
//! turns a rebuild skips. Separate from `session.rs` because it is the only
//! place history is ever rewritten in memory.

use cox_protocol::errors::CoreError;
use cox_protocol::ids::ItemId;
use cox_protocol::types::{
    Content, Event, HookEvent, HookOutcome, ItemKind, Job, Level, Message, ProviderEvent, Request,
    Role, SystemBlock,
};
use serde_json::json;
use tokio::sync::mpsc;

use crate::budget;
use crate::hooks;
use crate::session::{Session, State};

const PROMPT: &str = include_str!("prompts/compact.md");
/// §1.10 step 3: the summary itself is capped.
const MAX_SUMMARY_TOKENS: u32 = 2048;

/// Why compaction ran; reaches `PreCompact` hooks as `trigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// `context_tokens_last_call ≥ compact_at × max_context`.
    Auto,
    /// `/compact` or `Submission::Compact`.
    Manual,
    /// The provider rejected the request as too long.
    ContextTooLong,
}

impl Trigger {
    fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::ContextTooLong => "context_too_long",
        }
    }
}

/// Where a turn starts in the in-memory history, and the user item that
/// started it (the id a rollout rebuild drops the whole turn by).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TurnMark {
    pub item: ItemId,
    pub start: usize,
}

/// The history index the kept turns start at, and the turns before it.
pub(crate) fn split(marks: &[TurnMark], keep_turns: u32) -> Option<(usize, Vec<ItemId>)> {
    let keep = keep_turns as usize;
    if marks.len() <= keep {
        return None;
    }
    let first_kept = marks.len() - keep;
    let cut = marks[first_kept].start;
    let dropped = marks[..first_kept].iter().map(|m| m.item).collect();
    Some((cut, dropped))
}

pub(crate) fn needs_compaction(
    last_context_tokens: u32,
    max_context: u32,
    compact_at: f64,
) -> bool {
    max_context > 0 && f64::from(last_context_tokens) >= compact_at * f64::from(max_context)
}

/// ⌈bytes/4⌉ of the serialised messages: the same heuristic `truncate` and
/// the subagent cap use, so `before`/`after` compare across features.
pub(crate) fn estimate_tokens(messages: &[Message]) -> u32 {
    let bytes = serde_json::to_vec(messages).map(|v| v.len()).unwrap_or(0);
    u32::try_from(bytes.div_ceil(4)).unwrap_or(u32::MAX)
}

/// The summariser's input: one line per block, archived results as pointers.
pub(crate) fn transcript(messages: &[Message]) -> String {
    let mut out = String::new();
    for m in messages {
        let role = match m.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        for c in &m.content {
            match c {
                Content::Text { text } => out.push_str(&format!("{role}: {text}\n")),
                Content::ToolUse { name, input, .. } => {
                    out.push_str(&format!("tool_use {name} {input}\n"));
                }
                Content::ToolResult {
                    content, is_error, ..
                } => {
                    let tag = if *is_error {
                        "tool_error"
                    } else {
                        "tool_result"
                    };
                    out.push_str(&format!("{tag}: {content}\n"));
                }
                Content::Pointer { summary, archive } => {
                    out.push_str(&format!(
                        "tool_result (archived {}): {summary}\n",
                        archive.id
                    ));
                }
                Content::Thinking { .. } | Content::Image { .. } => {}
            }
        }
    }
    out
}

impl Session {
    /// §1.10 steps 1–5. `Ok(true)` when history changed; every failure is a
    /// notice and `Ok(false)`, since a session that cannot compact still runs.
    pub(crate) async fn compact(
        &self,
        trigger: Trigger,
        focus: Option<String>,
    ) -> Result<bool, CoreError> {
        let payload = json!({ "trigger": trigger.name(), "focus": focus });
        if let HookOutcome::Block { reason } =
            hooks::fire(self, HookEvent::PreCompact, payload.clone()).await
        {
            return self
                .compaction_notice(&format!("skipped by hook: {reason}"))
                .await;
        }
        let (history, marks) = {
            let inner = self.inner.lock().await;
            (inner.history.clone(), inner.turn_marks.clone())
        };
        let Some((cut, dropped)) = split(&marks, self.config.context.keep_turns) else {
            return self
                .compaction_notice(&format!(
                    "nothing to compact: {} turn(s) fit in keep_turns={}",
                    marks.len(),
                    self.config.context.keep_turns
                ))
                .await;
        };
        self.set_state(State::Compacting).await;
        let summary = self.summarise(&history[..cut], focus.as_deref()).await;
        self.set_state(State::Idle).await;
        let Some(summary) = summary else {
            return self.compaction_notice("summariser returned nothing").await;
        };
        let item = ItemId::new();
        let text = format!(
            "[Compacted summary of {} earlier turn(s)]\n\n{summary}",
            dropped.len()
        );
        self.emit(Event::ItemStarted {
            item,
            kind: ItemKind::Summary { text: text.clone() },
        })
        .await?;
        self.emit(Event::ItemDone { item }).await?;
        let (before, after) = {
            let mut inner = self.inner.lock().await;
            // A turn that ran meanwhile changed what `cut` means; give up
            // rather than splice the wrong messages.
            if inner.history.len() != history.len() {
                drop(inner);
                return self
                    .compaction_notice("history changed while summarising")
                    .await;
            }
            let before = estimate_tokens(&inner.history);
            let mut kept = inner.history.split_off(cut);
            let mut next = vec![Message {
                role: Role::User,
                content: vec![Content::Text { text }],
            }];
            next.append(&mut kept);
            inner.history = next;
            let mut kept_marks: Vec<TurnMark> = inner
                .turn_marks
                .split_off(marks.len() - marks.len().min(self.config.context.keep_turns as usize));
            for m in &mut kept_marks {
                m.start = m.start - cut + 1;
            }
            inner.turn_marks = vec![TurnMark { item, start: 0 }];
            inner.turn_marks.append(&mut kept_marks);
            let after = estimate_tokens(&inner.history);
            inner.last_context_tokens = after;
            (before, after)
        };
        self.emit(Event::Compacted {
            summary: item,
            dropped,
            before_tokens: before,
            after_tokens: after,
        })
        .await?;
        let _ = hooks::fire(self, HookEvent::PostCompact, payload).await;
        Ok(true)
    }

    async fn compaction_notice(&self, why: &str) -> Result<bool, CoreError> {
        self.emit(Event::Notice {
            level: Level::Warn,
            text: format!("compaction {why}"),
        })
        .await?;
        Ok(false)
    }

    /// One request on the `compact` job, recorded in the ledger like any
    /// other (D6g); `None` when the provider fails or answers nothing.
    async fn summarise(&self, messages: &[Message], focus: Option<&str>) -> Option<String> {
        // T9.1: the summary routes like any other job, so a `/model` switch
        // of the cheap tier applies here too.
        let route = self.route_for(Job::Compact, true).await.ok()?;
        let model = route.model.clone();
        let mut system = PROMPT.to_string();
        if let Some(focus) = focus {
            system.push_str(&format!("\nFocus on: {focus}\n"));
        }
        let req = Request {
            tier: route.tier,
            job: Job::Compact,
            model: model.clone(),
            system: vec![SystemBlock {
                text: system,
                cache: false,
            }],
            tools: vec![],
            messages: vec![Message {
                role: Role::User,
                content: vec![Content::Text {
                    text: transcript(messages),
                }],
            }],
            effort: route.effort,
            max_tokens: MAX_SUMMARY_TOKENS.min(route.max_tokens),
            thinking: route.thinking,
            cache_breakpoints: vec![],
            stop_sequences: vec![],
        };
        let (tx, mut rx) = mpsc::channel(64);
        let provider = self.provider.clone();
        let cancel = self.cancel_token();
        let join = tokio::spawn(async move { provider.stream(req, tx, cancel).await });
        let mut out = String::new();
        while let Some(ev) = rx.recv().await {
            if let ProviderEvent::TextDelta { text } = ev {
                out.push_str(&text);
            }
        }
        let usage = join.await.ok()?.ok()?;
        self.store
            .usage_insert(&cox_protocol::UsageRow {
                session_id: self.id,
                turn: 0,
                job: Job::Compact,
                tier: route.tier,
                provider: self.provider.id(),
                model,
                effort: Some(route.effort),
                usage,
            })
            .ok()?;
        if budget::counts(route.tier, self.config.budget.cheap_counts) {
            self.add_spend(usage.cost_usd).await;
        }
        let out = out.trim().to_string();
        (!out.is_empty()).then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(n: usize) -> Vec<TurnMark> {
        (0..n)
            .map(|i| TurnMark {
                item: ItemId::new(),
                start: i * 2,
            })
            .collect()
    }

    #[test]
    fn compact_split_keeps_the_last_turns_and_names_the_rest() {
        assert!(split(&marks(2), 2).is_none());
        let m = marks(5);
        let (cut, dropped) = split(&m, 2).expect("three to drop");
        assert_eq!(cut, 6);
        assert_eq!(dropped, [m[0].item, m[1].item, m[2].item]);
    }

    #[test]
    fn compact_trigger_is_a_fraction_of_max_context() {
        assert!(needs_compaction(750, 1000, 0.75));
        assert!(!needs_compaction(749, 1000, 0.75));
        assert!(!needs_compaction(1, 0, 0.75));
    }
}
