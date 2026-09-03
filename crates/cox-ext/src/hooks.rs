//! The hook runner (plan.md T7.4, D4/D14): runs `[[hooks.<Event>]]`
//! commands over Claude Code's JSON protocol — payload on stdin, verdict on
//! stdout, exit 2 = block. It lives here, not in `cox-core`, because it
//! spawns processes; the core only sees a `HookOutcome` and fails open.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use cox_protocol::config::{HookConfig, HooksConfig};
use cox_protocol::traits::Hook;
use cox_protocol::types::{HookEvent, HookOutcome};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub struct ShellHooks {
    events: HashMap<String, Vec<HookConfig>>,
    cwd: PathBuf,
}

impl ShellHooks {
    pub fn new(config: &HooksConfig, cwd: PathBuf) -> Self {
        Self {
            events: config.events.clone(),
            cwd,
        }
    }
}

#[async_trait]
impl Hook for ShellHooks {
    /// Hooks for `event` run in config order; the first block or failure
    /// ends the chain, a rewritten input feeds the hooks after it.
    async fn run(&self, event: HookEvent, mut payload: Value, timeout: Duration) -> HookOutcome {
        let Some(hooks) = self.events.get(event.name()) else {
            return HookOutcome::Continue;
        };
        let tool = payload
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut outcome = HookOutcome::Continue;
        for hook in hooks
            .iter()
            .filter(|h| matches(h.matcher.as_deref(), &tool))
        {
            let limit = hook
                .timeout_s
                .map_or(timeout, |s| Duration::from_secs(u64::from(s)));
            match run_one(&hook.command, &payload, limit, &self.cwd).await {
                HookOutcome::Continue => {}
                HookOutcome::Modify { input } => {
                    payload["tool_input"] = input.clone();
                    outcome = HookOutcome::Modify { input };
                }
                stop => return stop,
            }
        }
        outcome
    }
}

/// Claude's matcher: absent, empty or `*` matches everything; otherwise
/// `|`-separated names, each exact or a `prefix*` glob.
/// ponytail: no regex matchers; add `regex` when a real config needs one.
fn matches(matcher: Option<&str>, tool: &str) -> bool {
    match matcher.map(str::trim) {
        None | Some("") | Some("*") => true,
        Some(m) => m.split('|').map(str::trim).any(|pat| {
            pat.strip_suffix('*')
                .map_or(pat == tool, |prefix| tool.starts_with(prefix))
        }),
    }
}

async fn run_one(command: &str, payload: &Value, limit: Duration, cwd: &PathBuf) -> HookOutcome {
    let failed = |error: String| HookOutcome::Failed { error };
    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return failed(format!("spawn failed: {e}")),
    };
    let pid = child.id();
    if let Some(mut stdin) = child.stdin.take() {
        // A hook that never reads stdin is fine; a closed pipe is not an error.
        let _ = stdin.write_all(payload.to_string().as_bytes()).await;
    }
    let output = match tokio::time::timeout(limit, child.wait_with_output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return failed(format!("wait failed: {e}")),
        Err(_) => {
            // `kill_on_drop` took the shell; the group takes its children.
            if let Some(pid) = pid {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            return failed(format!("timed out after {}s", limit.as_secs()));
        }
    };
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    match output.status.code() {
        Some(0) => verdict(&String::from_utf8_lossy(&output.stdout)),
        Some(2) => HookOutcome::Block {
            reason: if stderr.is_empty() {
                "blocked by hook".into()
            } else {
                stderr
            },
        },
        Some(code) => failed(format!("exit {code}: {stderr}")),
        None => failed(format!("killed by signal: {stderr}")),
    }
}

/// Exit 0 stdout: JSON with `continue:false`, `decision:"block"`,
/// `hookSpecificOutput.permissionDecision:"deny"` or `updatedInput`;
/// anything else (including plain text) means continue.
fn verdict(stdout: &str) -> HookOutcome {
    let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) else {
        return HookOutcome::Continue;
    };
    let reason = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| v.pointer(k).and_then(Value::as_str))
            .unwrap_or("blocked by hook")
            .to_string()
    };
    let specific = v.get("hookSpecificOutput").cloned().unwrap_or(Value::Null);
    if v.get("continue") == Some(&Value::Bool(false)) {
        return HookOutcome::Block {
            reason: reason(&["/stopReason", "/reason"]),
        };
    }
    if v.get("decision").and_then(Value::as_str) == Some("block")
        || specific.get("permissionDecision").and_then(Value::as_str) == Some("deny")
    {
        return HookOutcome::Block {
            reason: reason(&["/reason", "/hookSpecificOutput/permissionDecisionReason"]),
        };
    }
    if let Some(input) = specific
        .get("updatedInput")
        .or_else(|| v.get("updatedInput"))
    {
        return HookOutcome::Modify {
            input: input.clone(),
        };
    }
    HookOutcome::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_matcher_is_exact_or_prefix_glob() {
        assert!(matches(None, "bash"));
        assert!(matches(Some("*"), "bash"));
        assert!(matches(Some("bash|edit"), "edit"));
        assert!(matches(Some("mcp__*"), "mcp__x__y"));
        assert!(!matches(Some("bash"), "bashful"));
    }

    #[test]
    fn hooks_verdict_reads_claude_shapes() {
        assert_eq!(verdict("all good"), HookOutcome::Continue);
        assert_eq!(
            verdict(r#"{"decision":"block","reason":"no"}"#),
            HookOutcome::Block {
                reason: "no".into()
            }
        );
        assert_eq!(
            verdict(
                r#"{"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"nope"}}"#
            ),
            HookOutcome::Block {
                reason: "nope".into()
            }
        );
        assert_eq!(
            verdict(r#"{"continue":false,"stopReason":"halt"}"#),
            HookOutcome::Block {
                reason: "halt".into()
            }
        );
        assert_eq!(
            verdict(r#"{"hookSpecificOutput":{"updatedInput":{"command":"ls"}}}"#),
            HookOutcome::Modify {
                input: serde_json::json!({"command":"ls"})
            }
        );
    }
}
