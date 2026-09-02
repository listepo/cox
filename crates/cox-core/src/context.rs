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
    let mut stable: Vec<_> = tools
        .iter()
        .map(|t| t.spec())
        .filter(|s| !s.deferred)
        .collect();
    stable.sort_by(|a, b| a.name.cmp(&b.name));
    let discovered: Vec<_> = tools
        .iter()
        .map(|t| t.spec())
        .filter(|s| s.deferred)
        .collect();
    let mut specs = stable;
    specs.extend(discovered);
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
        tools: tools.iter().map(|t| t.spec()).collect(),
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
