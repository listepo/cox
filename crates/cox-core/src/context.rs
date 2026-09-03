//! Cache-stable request assembly (plan.md §1.9). Separate from `turn` so the
//! prefix order can be snapshot-tested without running tools.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use cox_protocol::ids::CallId;
use cox_protocol::traits::Tool;
use cox_protocol::types::{ArchiveRef, Content, Job, Message, ModelId, Request, SystemBlock, Tier};

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

#[cfg(test)]
mod tests {
    use super::breakpoints;

    #[test]
    fn context_three_breakpoints_max_indices() {
        let bps = breakpoints(4, 3);
        assert_eq!(bps.len(), 3);
        assert_eq!(bps[0], 2);
        assert!(bps[1] < bps[2]);
    }
}
