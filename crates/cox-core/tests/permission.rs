//! Permission engine table (T2.2): one row per plan.md §1.8 claim, plus the
//! property that adding a deny rule never weakens a decision.

// One row = one rule set + one call + one expectation; rstest needs them
// as separate arguments.
#![allow(clippy::too_many_arguments)]

use std::path::Path;

use cox_core::{Engine, Outcome};
use cox_protocol::config::PermissionsConfig;
use cox_protocol::errors::CoreError;
use cox_protocol::ids::CallId;
use cox_protocol::types::{
    ApprovalPolicy as P, DecidedBy, PermissionMode as M, Risk, ToolCall, Why,
};
use proptest::prelude::*;
use rstest::rstest;

const HOME: &str = "/home/u";
const CWD: &str = "/repo";

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn engine(allow: &[&str], ask: &[&str], deny: &[&str]) -> Engine {
    let cfg = PermissionsConfig {
        allow: strs(allow),
        ask: strs(ask),
        deny: strs(deny),
        ..PermissionsConfig::default()
    };
    Engine::compile(&cfg, Some(Path::new(HOME)), Path::new(CWD)).expect("rules compile")
}

fn call(name: &str, subject: &str, risk: Risk) -> ToolCall {
    ToolCall {
        id: CallId::new(),
        name: name.into(),
        input: serde_json::json!({}),
        risk,
        subject: subject.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Want {
    Deny,
    Ask,
    Allow,
}

fn want(o: &Outcome) -> Want {
    match o {
        Outcome::Allow { .. } => Want::Allow,
        Outcome::Deny { .. } => Want::Deny,
        Outcome::Ask(_) => Want::Ask,
    }
}

#[rstest]
#[case::deny_beats_allow(&["Bash"], &[], &["Bash(rm -rf /*)"], "bash", "rm -rf /*", Risk::Destructive, M::Default, P::OnRequest, &[], Want::Deny)]
#[case::deny_beats_bypass(&[], &[], &["Bash"], "bash", "ls", Risk::Exec, M::Bypass, P::OnRequest, &[], Want::Deny)]
#[case::bypass_allows_destructive(&[], &[], &[], "bash", "rm -rf x", Risk::Destructive, M::Bypass, P::OnRequest, &[], Want::Allow)]
#[case::plan_mode_denies_writes_without_prompt(&[], &[], &[], "edit", "/repo/a.rs", Risk::Write, M::Plan, P::OnRequest, &[], Want::Deny)]
#[case::plan_mode_denies_exec_even_with_allow_rule(&["Bash"], &[], &[], "bash", "ls", Risk::Exec, M::Plan, P::OnRequest, &[], Want::Deny)]
#[case::plan_mode_allows_read_only(&[], &[], &[], "read", "/repo/a.rs", Risk::ReadOnly, M::Plan, P::OnRequest, &[], Want::Allow)]
#[case::bash_prefix_pattern_matches_npm_run_test_colon_star(&["Bash(npm run test:*)"], &[], &[], "bash", "npm run test -- --watch", Risk::Exec, M::Default, P::OnRequest, &[], Want::Allow)]
#[case::bash_prefix_pattern_matches_bare_prefix(&["Bash(npm run test:*)"], &[], &[], "bash", "npm run test", Risk::Exec, M::Default, P::OnRequest, &[], Want::Allow)]
#[case::bash_prefix_pattern_needs_word_boundary(&["Bash(npm run test:*)"], &[], &[], "bash", "npm run tests", Risk::Exec, M::Default, P::OnRequest, &[], Want::Ask)]
#[case::bash_exact_rule_is_exact(&["Bash(git status)"], &[], &[], "bash", "git status --short", Risk::Exec, M::Default, P::OnRequest, &[], Want::Ask)]
#[case::ask_rule_beats_read_only_risk(&[], &["Read"], &[], "read", "/repo/a.rs", Risk::ReadOnly, M::Default, P::OnRequest, &[], Want::Ask)]
#[case::ask_rule_beats_session_grant(&[], &["Bash"], &[], "bash", "npm test", Risk::Exec, M::Default, P::OnRequest, &[("bash", "")], Want::Ask)]
#[case::allow_rule_beats_ask_rule(&["Bash"], &["Bash"], &[], "bash", "ls", Risk::Exec, M::Default, P::OnRequest, &[], Want::Allow)]
#[case::session_grant_allows_prefix(&[], &[], &[], "bash", "npm test", Risk::Exec, M::Default, P::OnRequest, &[("bash", "npm")], Want::Allow)]
#[case::session_grant_is_per_tool(&[], &[], &[], "edit", "npm", Risk::Write, M::Default, P::OnRequest, &[("bash", "npm")], Want::Ask)]
#[case::session_grant_uses_claude_alias(&[], &[], &[], "web_fetch", "https://a", Risk::Exec, M::Default, P::OnRequest, &[("WebFetch", "https://a")], Want::Allow)]
#[case::read_only_runs_without_prompt(&[], &[], &[], "read", "/repo/a.rs", Risk::ReadOnly, M::Default, P::OnRequest, &[], Want::Allow)]
#[case::write_asks_in_default_mode(&[], &[], &[], "edit", "/repo/a.rs", Risk::Write, M::Default, P::OnRequest, &[], Want::Ask)]
#[case::write_runs_in_auto_mode(&[], &[], &[], "edit", "/repo/a.rs", Risk::Write, M::Auto, P::OnRequest, &[], Want::Allow)]
#[case::exec_asks_in_auto_mode(&[], &[], &[], "bash", "ls", Risk::Exec, M::Auto, P::OnRequest, &[], Want::Ask)]
#[case::destructive_asks_in_auto_mode(&[], &[], &[], "apply_patch", "x", Risk::Destructive, M::Auto, P::OnRequest, &[], Want::Ask)]
#[case::untrusted_asks_for_writes_even_in_auto(&[], &[], &[], "edit", "/repo/a.rs", Risk::Write, M::Auto, P::Untrusted, &[], Want::Ask)]
#[case::untrusted_still_runs_read_only(&[], &[], &[], "read", "/repo/a.rs", Risk::ReadOnly, M::Default, P::Untrusted, &[], Want::Allow)]
#[case::on_failure_runs_exec(&[], &[], &[], "bash", "cargo test", Risk::Exec, M::Default, P::OnFailure, &[], Want::Allow)]
#[case::on_failure_still_asks_for_writes(&[], &[], &[], "edit", "/repo/a.rs", Risk::Write, M::Default, P::OnFailure, &[], Want::Ask)]
#[case::on_failure_still_asks_for_destructive(&[], &[], &[], "bash", "rm -rf x", Risk::Destructive, M::Default, P::OnFailure, &[], Want::Ask)]
#[case::never_policy_turns_ask_into_deny(&[], &[], &[], "edit", "/repo/a.rs", Risk::Write, M::Default, P::Never, &[], Want::Deny)]
#[case::never_policy_keeps_read_only(&[], &[], &[], "read", "/repo/a.rs", Risk::ReadOnly, M::Default, P::Never, &[], Want::Allow)]
#[case::never_policy_keeps_allow_rules(&["Bash"], &[], &[], "bash", "ls", Risk::Exec, M::Default, P::Never, &[], Want::Allow)]
#[case::mcp_wildcard_allows_server(&["mcp__gh__*"], &[], &[], "mcp__gh__issues", "", Risk::Exec, M::Default, P::OnRequest, &[], Want::Allow)]
#[case::mcp_wildcard_is_per_server(&["mcp__gh__*"], &[], &[], "mcp__slack__post", "", Risk::Exec, M::Default, P::OnRequest, &[], Want::Ask)]
#[case::web_fetch_domain_allows_subdomain(&["WebFetch(domain:example.com)"], &[], &[], "web_fetch", "https://docs.example.com/a", Risk::Exec, M::Default, P::OnRequest, &[], Want::Allow)]
#[case::web_fetch_domain_rejects_lookalike(&["WebFetch(domain:example.com)"], &[], &[], "web_fetch", "https://example.com.evil/a", Risk::Exec, M::Default, P::OnRequest, &[], Want::Ask)]
#[case::path_glob_is_relative_to_cwd(&[], &[], &["Edit(src/**)"], "edit", "/repo/src/a.rs", Risk::Write, M::Auto, P::OnRequest, &[], Want::Deny)]
#[case::path_glob_expands_tilde(&[], &[], &["Read(~/secrets/**)"], "read", "/home/u/secrets/k", Risk::ReadOnly, M::Default, P::OnRequest, &[], Want::Deny)]
#[case::claude_alias_multiedit_is_edit(&[], &[], &["MultiEdit(src/**)"], "edit", "/repo/src/a.rs", Risk::Write, M::Auto, P::OnRequest, &[], Want::Deny)]
#[case::rule_tool_names_are_case_insensitive(&["BASH"], &[], &[], "bash", "ls", Risk::Exec, M::Default, P::OnRequest, &[], Want::Allow)]
fn permission_table(
    #[case] allow: &[&str],
    #[case] ask: &[&str],
    #[case] deny: &[&str],
    #[case] tool: &str,
    #[case] subject: &str,
    #[case] risk: Risk,
    #[case] mode: M,
    #[case] policy: P,
    #[case] grants: &[(&str, &str)],
    #[case] expected: Want,
) {
    let grants: Vec<(String, String)> = grants
        .iter()
        .map(|(t, s)| (t.to_string(), s.to_string()))
        .collect();
    let outcome =
        engine(allow, ask, deny).decide(&call(tool, subject, risk), mode, policy, &grants);
    assert_eq!(want(&outcome), expected, "{outcome:?}");
}

#[test]
fn permission_read_ssh_denied_by_default() {
    let cfg = PermissionsConfig::default();
    let engine = Engine::compile(&cfg, Some(Path::new(HOME)), Path::new(CWD)).expect("defaults");
    let outcome = engine.decide(
        &call("read", "/home/u/.ssh/id_rsa", Risk::ReadOnly),
        M::Bypass,
        P::OnRequest,
        &[],
    );
    match outcome {
        Outcome::Deny { reason, by } => {
            assert_eq!(by, DecidedBy::Rule);
            assert!(reason.contains("Read(~/.ssh/**)"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn permission_outcomes_name_their_source() {
    let e = engine(&["Bash(ls:*)"], &["Edit"], &[]);
    assert_eq!(
        e.decide(
            &call("bash", "ls -la", Risk::Exec),
            M::Default,
            P::OnRequest,
            &[]
        ),
        Outcome::Allow {
            by: DecidedBy::Rule
        }
    );
    assert_eq!(
        e.decide(&call("edit", "x", Risk::Write), M::Auto, P::OnRequest, &[]),
        Outcome::Ask(Why::RuleAsk {
            rule: "Edit".into()
        })
    );
    assert_eq!(
        e.decide(
            &call("bash", "cat x", Risk::Exec),
            M::Default,
            P::OnRequest,
            &[("bash".into(), "cat".into())]
        ),
        Outcome::Allow {
            by: DecidedBy::Session
        }
    );
    assert!(matches!(
        e.decide(
            &call("bash", "cat x", Risk::Exec),
            M::Default,
            P::OnRequest,
            &[]
        ),
        Outcome::Ask(Why::Risk { risk: Risk::Exec })
    ));
    assert!(matches!(
        e.decide(&call("write", "x", Risk::Write), M::Auto, P::Untrusted, &[]),
        Outcome::Ask(Why::Policy {
            policy: P::Untrusted
        })
    ));
}

#[test]
fn permission_bad_rule_is_a_config_error_not_a_skipped_guard() {
    let cfg = PermissionsConfig {
        deny: strs(&["Bash("]),
        ..PermissionsConfig::default()
    };
    match Engine::compile(&cfg, None, Path::new(CWD)) {
        Err(CoreError::Config { key, message }) => {
            assert_eq!(key, "permissions.deny");
            assert!(message.contains("Bash("), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

fn arb_risk() -> impl Strategy<Value = Risk> {
    prop_oneof![
        Just(Risk::ReadOnly),
        Just(Risk::Write),
        Just(Risk::Exec),
        Just(Risk::Destructive)
    ]
}

fn arb_mode() -> impl Strategy<Value = M> {
    prop_oneof![
        Just(M::Default),
        Just(M::Plan),
        Just(M::Auto),
        Just(M::Bypass)
    ]
}

fn arb_policy() -> impl Strategy<Value = P> {
    prop_oneof![
        Just(P::Untrusted),
        Just(P::OnRequest),
        Just(P::OnFailure),
        Just(P::Never)
    ]
}

proptest! {
    #[test]
    fn permission_adding_deny_never_weakens(
        tool in prop_oneof![Just("bash"), Just("edit"), Just("read"), Just("mcp__gh__x")],
        subject in prop_oneof![Just("ls"), Just("npm test"), Just("/repo/src/a.rs"), Just("")],
        risk in arb_risk(),
        mode in arb_mode(),
        policy in arb_policy(),
        allow in proptest::bool::ANY,
        grant in proptest::bool::ANY,
        deny_rule in prop_oneof![Just("Bash"), Just("Edit"), Just("bash(npm:*)"), Just("mcp__gh__*"), Just("Read(src/**)")],
    ) {
        let allow_rules: &[&str] = if allow { &["Bash", "Edit"] } else { &[] };
        let grants: Vec<(String, String)> = if grant {
            vec![(tool.to_string(), String::new())]
        } else {
            vec![]
        };
        let base = engine(allow_rules, &[], &[]).decide(&call(tool, subject, risk), mode, policy, &grants);
        let denied = engine(allow_rules, &[], &[deny_rule]).decide(&call(tool, subject, risk), mode, policy, &grants);
        prop_assert!(want(&denied) <= want(&base), "{base:?} -> {denied:?}");
    }
}
