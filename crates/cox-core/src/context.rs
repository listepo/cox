//! Cache-stable request assembly (plan.md §1.9). Separate from `turn` so the
//! prefix order can be snapshot-tested without running tools. `Breakdown`
//! (T25.7) attributes a request's estimated tokens back to those segments.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use cox_protocol::ids::CallId;
use cox_protocol::traits::Tool;
use cox_protocol::types::{
    ArchiveRef, Content, Job, Message, ModelId, Request, SystemBlock, Tier, Usage,
};

/// Instruction-file stub until T7.1 reads the AGENTS.md chain.
const INSTRUCTIONS: &str = "Follow repository instruction files when present.";

const PROMPT: &str = include_str!("prompt.md");

/// Builds a `Request` with `system[0..=2]` byte-stable and three breakpoints.
pub fn assemble(
    history: &[Message],
    config: &cox_protocol::Config,
    tools: &[Arc<dyn Tool>],
    cwd: &Path,
    date: &str,
) -> Request {
    assemble_with(history, config, Tier::Code, tools, &[], cwd, date)
}

/// `assemble` with the deferred tools the model has found through
/// `tool_search` (D6d): those specs join the request in discovery order
/// after the stable core set, so the prefix changes once per discovery
/// and is stable again afterwards. With `context.deferred_tools = false`
/// nothing is deferred and every tool is always present.
pub fn assemble_with(
    history: &[Message],
    config: &cox_protocol::Config,
    tier: Tier,
    tools: &[Arc<dyn Tool>],
    discovered: &[String],
    cwd: &Path,
    date: &str,
) -> Request {
    let all: Vec<_> = tools.iter().map(|t| t.spec()).collect();
    let deferring = config.context.deferred_tools;
    let mut specs: Vec<_> = all
        .iter()
        .filter(|s| !deferring || !s.deferred)
        .cloned()
        .collect();
    specs.sort_by(|a, b| a.name.cmp(&b.name));
    if deferring {
        for name in discovered {
            if specs.iter().any(|s| &s.name == name) {
                continue;
            }
            if let Some(spec) = all.iter().find(|s| s.deferred && &s.name == name) {
                specs.push(spec.clone());
            }
        }
    }
    let tools_json = serde_json::to_string(&specs).unwrap_or_else(|_| "[]".into());

    let system = vec![
        SystemBlock {
            text: tools_json,
            cache: true,
        },
        SystemBlock {
            text: PROMPT.to_string(),
            cache: true,
        },
        SystemBlock {
            text: INSTRUCTIONS.to_string(),
            cache: true,
        },
        SystemBlock {
            text: format!(
                "date={date}\ncwd={}\npermission_mode={:?}\n",
                cwd.display(),
                config.permissions.mode
            ),
            cache: false,
        },
    ];
    let cache_breakpoints = breakpoints(system.len(), history.len());
    let tc = config.tiers.get(tier);
    Request {
        tier,
        job: Job::Main,
        model: ModelId(tc.model.clone()),
        system,
        tools: specs,
        messages: history.to_vec(),
        effort: tc.effort,
        max_tokens: tc.max_tokens,
        thinking: tc.thinking,
        cache_breakpoints,
        stop_sequences: vec![],
    }
}

fn breakpoints(system_len: usize, n_messages: usize) -> Vec<usize> {
    let mut bps = vec![2];
    if n_messages >= 2 {
        bps.push(system_len + n_messages - 2);
    }
    if n_messages >= 1 {
        let last = system_len + n_messages - 1;
        if bps.last().copied() != Some(last) {
            bps.push(last);
        }
    }
    bps.truncate(3);
    bps
}

/// Microcompaction (T8.2 §1.10): old tool results become `Pointer`s in the
/// request without a model call. Pure over a copy: the stored history keeps
/// the visible text (so the rollout and `cox expand` are untouched); only
/// the returned messages change, one block at a time, so turn boundaries
/// and cache breakpoints are unaffected.
///
/// A result is replaced when its turn is older than `after_turns` back from
/// the newest AND outside the last `keep_turns` turns (which are never
/// touched). Turns come from `turn_starts` (T8.1 marks); an empty slice
/// means "no turn info" and returns the input unchanged.
pub fn microcompact(
    messages: &[Message],
    turn_starts: &[usize],
    keep_turns: u32,
    after_turns: u32,
    archives: &HashMap<CallId, ArchiveRef>,
) -> Vec<Message> {
    if turn_starts.is_empty() || messages.is_empty() {
        return messages.to_vec();
    }
    let n = turn_starts.len();
    let keep_from = n.saturating_sub(keep_turns as usize);
    let turn_of = |m: usize| -> usize {
        match turn_starts.binary_search(&m) {
            Ok(t) => t,
            Err(0) => 0,
            Err(t) => t - 1,
        }
        .min(n - 1)
    };
    // Tool names live in the matching `ToolUse` block in history.
    let mut names: HashMap<CallId, &str> = HashMap::new();
    for msg in messages {
        for c in &msg.content {
            if let Content::ToolUse { id, name, .. } = c {
                names.insert(*id, name.as_str());
            }
        }
    }
    messages
        .iter()
        .enumerate()
        .map(|(m, msg)| {
            let t = turn_of(m);
            // ponytail: O(turns) scan per message via binary_search; fine at
            // session sizes, revisit with a cursor if history grows large.
            if t >= keep_from || (n - t) <= after_turns as usize {
                return msg.clone();
            }
            let content = msg
                .content
                .iter()
                .map(|c| match c {
                    Content::ToolResult { call_id, .. } => match archives.get(call_id) {
                        Some(arch) => Content::Pointer {
                            archive: arch.clone(),
                            summary: format!(
                                "{}: {} bytes archived; expand #{}",
                                names.get(call_id).copied().unwrap_or("tool"),
                                arch.bytes,
                                arch.id
                            ),
                        },
                        None => c.clone(),
                    },
                    _ => c.clone(),
                })
                .collect();
            Message {
                role: msg.role,
                content,
            }
        })
        .collect()
}

// The `/context` payload surface below is called from `session.rs`'s
// `Submission::Command` dispatch — the wiring split out of T25.7's 3-file
// budget (recorded in the card) — so until that lands nothing in the crate
// reaches it and `dead_code` is allowed one item at a time, never blanket.
#[allow(dead_code)]
/// The marker `compact.rs` prefixes its summary message with; otherwise a
/// summary is an indistinguishable plain user message (append-only history,
/// `ItemKind::Summary` replays as one) and could not fill `summary` below.
const SUMMARY_HEADER: &str = "[Compacted summary of ";

#[allow(dead_code)]
/// Where the next request's tokens go (T25.7 `/context`): the §1.9 segments
/// plus the whole and the cached share. `total` is the T1.8 estimator's
/// request total verbatim — cox-core may not depend on cox-provider, so the
/// provider-owning caller passes `cox_provider::tokens::estimate`'s number —
/// and the nine segment fields distribute it exactly.
pub struct Breakdown {
    pub tools: u32,
    pub system: u32,
    pub instructions: u32,
    pub skills: u32,
    pub memory: u32,
    pub volatile: u32,
    pub history_verbatim: u32,
    pub history_pointers: u32,
    pub summary: u32,
    pub total: u32,
    pub cached_estimate: u32,
}

#[allow(dead_code)]
impl Breakdown {
    /// The `structured` payload `/context`'s notice carries (T25.7 step 2).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "tools": self.tools, "system": self.system, "instructions": self.instructions,
            "skills": self.skills, "memory": self.memory, "volatile": self.volatile,
            "history_verbatim": self.history_verbatim, "history_pointers": self.history_pointers,
            "summary": self.summary, "total": self.total, "cached_estimate": self.cached_estimate,
        })
    }
}

/// Splits `total` (the T1.8 estimator's request total for `req`) across the
/// segments by rendered bytes; cumulative rounding keeps the nine shares
/// summing to `total` exactly, so the modal's bars never disagree with the
/// estimate. `last_usage` supplies `cached_estimate` from its
/// `cache_read_tokens` — what the last call actually served from cache.
#[allow(dead_code)]
pub fn breakdown(req: &Request, total: u32, last_usage: Option<&Usage>) -> Breakdown {
    // Byte weights per segment (index order is `Breakdown`'s); the
    // estimator's byte term is itself a heuristic, so attributing each
    // content by its rendered JSON length is close enough for the split.
    let mut w = [0u64; 9];
    for (i, block) in req.system.iter().enumerate() {
        let seg = match i {
            0 => 0, // tool specs
            1 => 1, // system prompt
            2 => 2, // instruction files (the skills index joins here in T7.1)
            _ => 5, // volatile (the memory index joins here in T10)
        };
        w[seg] += block.text.len() as u64 + 1;
    }
    for (i, msg) in req.messages.iter().enumerate() {
        // skills/memory stay empty until T7.1/T10 split them out of 2/3.
        let verbatim = match (&msg.content[..], i) {
            ([Content::Text { text }], 0) if text.starts_with(SUMMARY_HEADER) => 8,
            _ => 6,
        };
        for c in &msg.content {
            let seg = match c {
                Content::Pointer { .. } => 7,
                Content::Image { .. } => continue,
                _ => verbatim,
            };
            w[seg] += serde_json::to_string(c).map_or(0, |s| s.len() as u64);
        }
    }
    let sum: u64 = w.iter().sum();
    let mut shares = [0u32; 9];
    let (mut acc, mut prev) = (0u64, 0u64);
    for (i, weight) in w.iter().enumerate() {
        acc += weight;
        let cum = u64::from(total) * acc / sum.max(1);
        shares[i] = (cum - prev) as u32;
        prev = cum;
    }
    if sum == 0 {
        shares[5] = total; // a byte-free request still costs; volatile is the catch-all
    }
    Breakdown {
        tools: shares[0],
        system: shares[1],
        instructions: shares[2],
        skills: shares[3],
        memory: shares[4],
        volatile: shares[5],
        history_verbatim: shares[6],
        history_pointers: shares[7],
        summary: shares[8],
        total,
        cached_estimate: last_usage.map_or(0, |u| u.cache_read_tokens),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T25.7: `breakdown.total` equals the estimator's request total and the
    /// segment shares sum to that estimate exactly.
    #[test]
    fn breakdown_sums_to_estimate() {
        let history: Vec<Message> = serde_json::from_value(serde_json::json!([
            {"role": "user", "content": [{"type": "text", "text": "[Compacted summary of 2 earlier turn(s)]"}]},
            {"role": "user", "content": [{"type": "text", "text": "go on"}]},
            {"role": "user", "content": [{"type": "pointer",
                "archive": {"id": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "bytes": 9},
                "summary": "read: 9 bytes"}]},
            {"role": "assistant", "content": [{"type": "tool_use",
                "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "name": "read", "input": {"path": "lib.rs"}}]},
        ]))
        .expect("history");
        let config = cox_protocol::Config::default();
        let req = assemble(&history, &config, &[], Path::new("/w"), "2026-09-22");
        let estimated = cox_provider::tokens::estimate(&req).tokens;
        let usage: Usage = serde_json::from_value(serde_json::json!(
            {"input_tokens": 1, "output_tokens": 1, "cache_read_tokens": 640,
             "cache_write_tokens": 8, "estimated": true, "cost_usd": 0.0, "latency_ms": 1}
        ))
        .expect("usage");
        let b = breakdown(&req, estimated, Some(&usage));
        assert_eq!(b.total, estimated, "the estimator's request total");
        assert_eq!(
            b.tools
                + b.system
                + b.instructions
                + b.skills
                + b.memory
                + b.volatile
                + b.history_verbatim
                + b.history_pointers
                + b.summary,
            b.total,
            "the segments sum to the estimate"
        );
        assert!(b.summary > 0 && b.history_pointers > 0 && b.instructions > 0);
        assert_eq!(b.cached_estimate, usage.cache_read_tokens);
        assert_eq!(b.to_json()["total"], b.total);
    }
}
