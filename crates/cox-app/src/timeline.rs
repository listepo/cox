//! The timeline fold (DT§4.3): `Event` in, keyed blocks updated, the
//! `TimelinePatch`es that describe the change out. Pure and deterministic,
//! so folding a rollout read back from disk yields the patches the live run
//! did — resume and live share one code path.

use std::fmt::Display;

use cox_protocol::types::{Event, ItemKind, Usage};
use cox_render::glyph::UNICODE;
use cox_render::markdown;

use crate::patch::{Block, BlockId, BlockKind, TimelinePatch, ToolState, tail};

/// The block list of one session and what folds events into it.
#[derive(Debug, Clone, Default)]
pub struct Timeline {
    blocks: Vec<Block>,
    /// The current turn's ordinal.
    turn: u32,
    /// Events applied so far; keys the blocks no event id names.
    seq: u64,
    /// The syntect theme code blocks are highlighted with.
    theme: String,
}

fn key(kind: &str, id: impl Display) -> BlockId {
    BlockId(format!("{kind}:{id}"))
}

impl Timeline {
    pub fn new(theme: &str) -> Self {
        Self {
            theme: theme.to_owned(),
            ..Self::default()
        }
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The whole list as one patch, to heal a consumer that missed some.
    pub fn reset(&self) -> TimelinePatch {
        TimelinePatch::Reset {
            blocks: self.blocks.clone(),
        }
    }

    /// Folds one event; returns the patches it caused, possibly none.
    pub fn apply(&mut self, event: &Event) -> Vec<TimelinePatch> {
        self.seq += 1;
        let seq = self.seq;
        match event {
            Event::TurnStarted {
                turn,
                seq,
                tier,
                model,
                ..
            } => {
                self.turn = *seq;
                let kind = BlockKind::TurnMeta {
                    model: model.clone(),
                    tier: *tier,
                    usage: None,
                    stop: None,
                };
                self.insert(key("turn", turn), kind)
            }
            Event::ItemStarted { item, kind } => {
                let kind = match kind {
                    ItemKind::UserMessage { text, attachments } => BlockKind::User {
                        text: text.clone(),
                        attachments: attachments.iter().map(|a| a.name.clone()).collect(),
                    },
                    ItemKind::AssistantMessage { text } => BlockKind::Assistant {
                        text: text.clone(),
                        doc: markdown::parse(text, &self.theme, &UNICODE),
                    },
                    ItemKind::Thinking { text, .. } => BlockKind::Thinking { text: text.clone() },
                    ItemKind::Notice { level, text } => BlockKind::Notice {
                        level: *level,
                        text: text.clone(),
                    },
                    // Calls and results arrive as their own events; a
                    // summary is shown by its `Compacted`.
                    ItemKind::ToolCall { .. }
                    | ItemKind::ToolResult { .. }
                    | ItemKind::Summary { .. } => return vec![],
                };
                self.insert(key("item", item), kind)
            }
            Event::TextDelta { item, text } => self.stream_doc(key("item", item), text),
            Event::ItemDone { item } => {
                // Closing a reply re-sends it whole: the source text reaches
                // the UI and any dropped `DocTail` is healed.
                let id = key("item", item);
                let reply = self.find(&id).map(|i| &self.blocks[i].kind);
                if matches!(reply, Some(BlockKind::Assistant { .. })) {
                    self.update(&id, |_| {})
                } else {
                    vec![]
                }
            }
            Event::ThinkingDelta { item, text } => self.append(key("item", item), text),
            Event::ToolCallRequested { call } => {
                let kind = BlockKind::Tool {
                    tool: call.name.clone(),
                    summary: one_line(&call.subject),
                    risk: call.risk,
                    state: ToolState::Running,
                    tail: String::new(),
                    archive: None,
                    diff: None,
                    duration_ms: 0,
                };
                self.insert(key("call", call.id), kind)
            }
            Event::ToolCallOutput { call_id, delta } => self.append(key("call", call_id), delta),
            Event::ToolCallDone { call_id, result } => {
                let mut out = self.update(&key("question", call_id), |k| {
                    if let BlockKind::Question { answer, .. } = k {
                        *answer = Some(result.visible.clone());
                    }
                });
                out.extend(self.update(&key("call", call_id), |k| {
                    if let BlockKind::Tool {
                        state,
                        tail: t,
                        archive,
                        diff,
                        duration_ms,
                        ..
                    } = k
                    {
                        *state = if result.ok {
                            ToolState::Done
                        } else {
                            ToolState::Failed
                        };
                        *t = tail(&result.visible).to_owned();
                        *archive = result.archive.clone();
                        *diff = result.diff.clone();
                        *duration_ms = result.duration_ms;
                    }
                }));
                out
            }
            Event::ApprovalRequired { call, why, source } => {
                let kind = BlockKind::Approval {
                    call: call.id,
                    tool: call.name.clone(),
                    summary: one_line(&call.subject),
                    why: why.clone(),
                    source: source.clone(),
                    decision: None,
                    by: None,
                };
                self.insert(key("approval", call.id), kind)
            }
            Event::ApprovalDecided {
                call_id,
                decision: d,
                by: b,
            } => self.update(&key("approval", call_id), |k| {
                if let BlockKind::Approval { decision, by, .. } = k {
                    *decision = Some(d.clone());
                    *by = Some(*b);
                }
            }),
            Event::QuestionAsked {
                call_id,
                question,
                options,
                ..
            } => {
                let kind = BlockKind::Question {
                    call: *call_id,
                    question: question.clone(),
                    options: options.clone(),
                    answer: None,
                };
                self.insert(key("question", call_id), kind)
            }
            Event::Usage { turn, usage: u } => self.update(&key("turn", turn), |k| {
                if let BlockKind::TurnMeta { usage, .. } = k {
                    *usage = Some(usage.map_or(*u, |sum| add(sum, u)));
                }
            }),
            Event::TurnDone { turn, stop: s } => self.update(&key("turn", turn), |k| {
                if let BlockKind::TurnMeta { stop, .. } = k {
                    *stop = Some(s.clone());
                }
            }),
            Event::Compacted {
                summary,
                before_tokens,
                after_tokens,
                reason,
                ..
            } => {
                let kind = BlockKind::Compaction {
                    before_tokens: *before_tokens,
                    after_tokens: *after_tokens,
                    reason: *reason,
                };
                self.insert(key("compaction", summary), kind)
            }
            Event::Checkpoint { files, .. } => {
                let files = files.iter().map(|f| f.path.clone()).collect();
                self.insert(key("checkpoint", seq), BlockKind::Checkpoint { files })
            }
            Event::Rewound {
                to_turn,
                conversation: true,
                ..
            } => {
                let (gone, kept): (Vec<Block>, Vec<Block>) = std::mem::take(&mut self.blocks)
                    .into_iter()
                    .partition(|b| b.turn > *to_turn);
                self.blocks = kept;
                gone.into_iter()
                    .map(|b| TimelinePatch::Remove { id: b.id })
                    .collect()
            }
            Event::TaskCreated { task, label, tier } => {
                let kind = BlockKind::Task {
                    task: *task,
                    label: label.clone(),
                    tier: *tier,
                    done: false,
                    cost_usd: 0.0,
                    exit_code: None,
                };
                self.insert(key("task", task), kind)
            }
            Event::TaskCompleted {
                task,
                cost_usd: c,
                exit_code: e,
                ..
            } => self.update(&key("task", task), |k| {
                if let BlockKind::Task {
                    done,
                    cost_usd,
                    exit_code,
                    ..
                } = k
                {
                    (*done, *cost_usd, *exit_code) = (true, *c, *e);
                }
            }),
            Event::Notice { level, text } => {
                let kind = BlockKind::Notice {
                    level: *level,
                    text: text.clone(),
                };
                self.insert(key("notice", seq), kind)
            }
            Event::Error { error, fatal } => {
                let kind = BlockKind::Error {
                    text: error.to_string(),
                    fatal: *fatal,
                };
                self.insert(key("error", seq), kind)
            }
            _ => vec![],
        }
    }

    fn find(&self, id: &BlockId) -> Option<usize> {
        self.blocks.iter().rposition(|b| &b.id == id)
    }

    fn upsert(&self, i: usize) -> Vec<TimelinePatch> {
        let after = i.checked_sub(1).map(|p| self.blocks[p].id.clone());
        vec![TimelinePatch::Upsert {
            block: Box::new(self.blocks[i].clone()),
            after,
        }]
    }

    fn insert(&mut self, id: BlockId, kind: BlockKind) -> Vec<TimelinePatch> {
        if self.find(&id).is_some() {
            return vec![];
        }
        let turn = self.turn;
        self.blocks.push(Block { id, turn, kind });
        self.upsert(self.blocks.len() - 1)
    }

    fn update(&mut self, id: &BlockId, f: impl FnOnce(&mut BlockKind)) -> Vec<TimelinePatch> {
        let Some(i) = self.find(id) else {
            return vec![];
        };
        f(&mut self.blocks[i].kind);
        self.upsert(i)
    }

    fn append(&mut self, id: BlockId, delta: &str) -> Vec<TimelinePatch> {
        let Some(i) = self.find(&id) else {
            return vec![];
        };
        match &mut self.blocks[i].kind {
            BlockKind::Thinking { text } => text.push_str(delta),
            BlockKind::Tool { tail: t, .. } => *t = tail(&(t.clone() + delta)).to_owned(),
            _ => return vec![],
        }
        vec![TimelinePatch::AppendText {
            id,
            text: delta.to_owned(),
        }]
    }

    /// Re-parses the reply and sends only the blocks from the first one
    /// that changed.
    fn stream_doc(&mut self, id: BlockId, delta: &str) -> Vec<TimelinePatch> {
        let Some(i) = self.find(&id) else {
            return vec![];
        };
        let BlockKind::Assistant { text, doc } = &mut self.blocks[i].kind else {
            return vec![];
        };
        text.push_str(delta);
        let next = markdown::parse(text, &self.theme, &UNICODE);
        let from = doc
            .blocks
            .iter()
            .zip(&next.blocks)
            .take_while(|(a, b)| a == b)
            .count();
        let blocks = next.blocks[from..].to_vec();
        *doc = next;
        vec![TimelinePatch::DocTail {
            id,
            from: from as u32,
            blocks,
        }]
    }
}

/// A multi-line subject (a heredoc command, a whole echoed text) shows its
/// first line; the full input is in the call.
fn one_line(subject: &str) -> String {
    subject.lines().next().unwrap_or_default().to_owned()
}

fn add(a: Usage, b: &Usage) -> Usage {
    Usage {
        input_tokens: a.input_tokens.saturating_add(b.input_tokens),
        output_tokens: a.output_tokens.saturating_add(b.output_tokens),
        cache_read_tokens: a.cache_read_tokens.saturating_add(b.cache_read_tokens),
        cache_write_tokens: a.cache_write_tokens.saturating_add(b.cache_write_tokens),
        estimated: a.estimated || b.estimated,
        cost_usd: a.cost_usd + b.cost_usd,
        latency_ms: a.latency_ms.saturating_add(b.latency_ms),
    }
}

#[cfg(test)]
mod tests {
    use cox_protocol::ids::{CallId, ItemId, TurnId};
    use cox_protocol::types::{Job, ModelId, Risk, Tier, ToolCall};

    use super::*;

    fn started(timeline: &mut Timeline, seq: u32) {
        timeline.apply(&Event::TurnStarted {
            turn: TurnId::new(),
            seq,
            job: Job::Main,
            tier: Tier::Code,
            model: ModelId("m".into()),
        });
    }

    #[test]
    fn streamed_markdown_resends_only_the_blocks_after_the_last_closed_one() {
        let mut timeline = Timeline::new("base16-ocean.dark");
        let item = ItemId::new();
        let kind = ItemKind::AssistantMessage {
            text: String::new(),
        };
        timeline.apply(&Event::ItemStarted { item, kind });
        let mut delta = |text: &str| {
            timeline.apply(&Event::TextDelta {
                item,
                text: text.into(),
            })
        };
        delta("# Title\n\nfirst para");
        let patches = delta("graph, still open");
        let [TimelinePatch::DocTail { from, blocks, .. }] = patches.as_slice() else {
            panic!("expected one DocTail, got {patches:?}");
        };
        assert_eq!((*from, blocks.len()), (1, 1), "the heading is frozen");
    }

    #[test]
    fn tool_tail_keeps_the_last_five_lines_across_chunks() {
        let mut timeline = Timeline::default();
        let call = ToolCall {
            id: CallId::new(),
            name: "bash".into(),
            input: serde_json::Value::Null,
            risk: Risk::Exec,
            subject: "seq 1 7\nsecond line".into(),
            segments: None,
        };
        let call_id = call.id;
        timeline.apply(&Event::ToolCallRequested { call });
        for delta in ["1\n2\n3", "\n4\n5\n", "6\n7\n"] {
            timeline.apply(&Event::ToolCallOutput {
                call_id,
                delta: delta.into(),
            });
        }
        let BlockKind::Tool { tail, summary, .. } = &timeline.blocks()[0].kind else {
            panic!("expected a tool block");
        };
        assert_eq!(
            (tail.as_str(), summary.as_str()),
            ("3\n4\n5\n6\n7\n", "seq 1 7")
        );
    }

    #[test]
    fn rewinding_the_conversation_removes_the_later_turns_blocks() {
        let mut timeline = Timeline::default();
        for seq in 1..=3 {
            started(&mut timeline, seq);
        }
        let patches = timeline.apply(&Event::Rewound {
            to_turn: 1,
            code: false,
            conversation: true,
            restored: vec![],
            skipped: vec![],
        });
        assert_eq!(patches.len(), 2);
        assert!(
            patches
                .iter()
                .all(|p| matches!(p, TimelinePatch::Remove { .. }))
        );
        assert_eq!(timeline.blocks().len(), 1);
    }
}
