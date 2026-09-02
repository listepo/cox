//! Cache-stable request assembly (plan.md §1.9). Separate from `turn` so the
//! prefix order can be snapshot-tested without running tools.

use std::path::Path;
use std::sync::Arc;

use cox_protocol::traits::Tool;
use cox_protocol::types::{Job, Message, ModelId, Request, SystemBlock, Tier};

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
    assemble_with(history, config, tools, &[], cwd, date)
}

/// `assemble` with the deferred tools the model has found through
/// `tool_search` (D6d): those specs join the request in discovery order
/// after the stable core set, so the prefix changes once per discovery
/// and is stable again afterwards. With `context.deferred_tools = false`
/// nothing is deferred and every tool is always present.
pub fn assemble_with(
    history: &[Message],
    config: &cox_protocol::Config,
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
    let tier = &config.tiers.code;
    Request {
        tier: Tier::Code,
        job: Job::Main,
        model: ModelId(tier.model.clone()),
        system,
        tools: specs,
        messages: history.to_vec(),
        effort: tier.effort,
        max_tokens: tier.max_tokens,
        thinking: tier.thinking,
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
