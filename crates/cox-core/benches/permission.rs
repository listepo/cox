//! Microbenchmarks for the pure permission engine
//! (`cox_core::permission::Engine::decide`, plan.md §1.8).
//!
//! The engine is table-driven and deterministic, so one compiled rule set
//! plus one call per decision path (allow/deny/ask, via rules, risk and
//! session grants) is a representative matrix. Each bench measures `decide`
//! only: the engine and the calls are built once, outside the timed closure.

use std::path::Path;
use std::sync::LazyLock;

use cox_core::Engine;
use cox_protocol::config::PermissionsConfig;
use cox_protocol::ids::CallId;
use cox_protocol::types::{ApprovalPolicy, PermissionMode, Risk, SandboxMode, ToolCall};

fn main() {
    divan::main();
}

/// One compiled engine plus one call per decision path. Rule subjects mirror
/// the T2.2 table (`tests/permission.rs`) so the bench stays representative
/// of the paths the tests pin.
struct Fixture {
    engine: Engine,
    /// `Bash(npm run test:*)` allow rule.
    allow_rule: ToolCall,
    /// No rule; `ReadOnly` allows by risk.
    allow_readonly: ToolCall,
    /// `Read(~/.ssh/**)` deny rule beats the `ReadOnly` default.
    deny_rule: ToolCall,
    /// `Bash(rm -rf /*)` deny rule.
    deny_bash: ToolCall,
    /// `Edit(src/**)` ask rule.
    ask_rule: ToolCall,
    /// No rule; `Write` asks by risk in `Default`.
    ask_write: ToolCall,
    /// No rule; `Exec` asks by risk.
    ask_exec: ToolCall,
    /// Session grant (`bash` + `npm` prefix) allows without a rule.
    grant: ToolCall,
    grants: Vec<(String, String)>,
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

static FIXTURE: LazyLock<Fixture> = LazyLock::new(|| {
    // Benches are dev targets like tests (AGENTS.md), so `expect` on a
    // fixture that must compile is the same convention `tests/` uses.
    let cfg = PermissionsConfig {
        allow: vec![
            "Bash(npm run test:*)".to_string(),
            "Read(src/**)".to_string(),
            "mcp__gh__*".to_string(),
        ],
        ask: vec![
            "Edit(src/**)".to_string(),
            "WebFetch(domain:example.com)".to_string(),
        ],
        deny: vec![
            "Read(~/.ssh/**)".to_string(),
            "Read(~/.aws/**)".to_string(),
            "Bash(rm -rf /*)".to_string(),
        ],
        ..PermissionsConfig::default()
    };
    let engine = Engine::compile(&cfg, Some(Path::new("/home/u")), Path::new("/repo"))
        .expect("bench rules compile");
    Fixture {
        engine,
        allow_rule: call("bash", "npm run test -- --watch", Risk::Exec),
        allow_readonly: call("read", "/repo/notes.md", Risk::ReadOnly),
        deny_rule: call("read", "/home/u/.ssh/id_rsa", Risk::ReadOnly),
        deny_bash: call("bash", "rm -rf /*", Risk::Destructive),
        ask_rule: call("edit", "/repo/src/a.rs", Risk::Write),
        ask_write: call("edit", "/repo/other.rs", Risk::Write),
        ask_exec: call("bash", "cargo test", Risk::Exec),
        grant: call("bash", "npm test", Risk::Exec),
        grants: vec![("bash".to_string(), "npm".to_string())],
    }
});

const MODE: PermissionMode = PermissionMode::Default;
const POLICY: ApprovalPolicy = ApprovalPolicy::OnRequest;
const SANDBOX: SandboxMode = SandboxMode::WorkspaceWrite;

#[divan::bench]
fn decide_allow_rule(bencher: divan::Bencher) {
    let fixture = &*FIXTURE;
    bencher.bench(|| {
        fixture
            .engine
            .decide(&fixture.allow_rule, MODE, POLICY, SANDBOX, &[])
    });
}

#[divan::bench]
fn decide_allow_readonly(bencher: divan::Bencher) {
    let fixture = &*FIXTURE;
    bencher.bench(|| {
        fixture
            .engine
            .decide(&fixture.allow_readonly, MODE, POLICY, SANDBOX, &[])
    });
}

#[divan::bench]
fn decide_deny_rule(bencher: divan::Bencher) {
    let fixture = &*FIXTURE;
    bencher.bench(|| {
        fixture
            .engine
            .decide(&fixture.deny_rule, MODE, POLICY, SANDBOX, &[])
    });
}

#[divan::bench]
fn decide_deny_bash(bencher: divan::Bencher) {
    let fixture = &*FIXTURE;
    bencher.bench(|| {
        fixture
            .engine
            .decide(&fixture.deny_bash, MODE, POLICY, SANDBOX, &[])
    });
}

#[divan::bench]
fn decide_ask_rule(bencher: divan::Bencher) {
    let fixture = &*FIXTURE;
    bencher.bench(|| {
        fixture
            .engine
            .decide(&fixture.ask_rule, MODE, POLICY, SANDBOX, &[])
    });
}

#[divan::bench]
fn decide_ask_write(bencher: divan::Bencher) {
    let fixture = &*FIXTURE;
    bencher.bench(|| {
        fixture
            .engine
            .decide(&fixture.ask_write, MODE, POLICY, SANDBOX, &[])
    });
}

#[divan::bench]
fn decide_ask_exec(bencher: divan::Bencher) {
    let fixture = &*FIXTURE;
    bencher.bench(|| {
        fixture
            .engine
            .decide(&fixture.ask_exec, MODE, POLICY, SANDBOX, &[])
    });
}

#[divan::bench]
fn decide_session_grant(bencher: divan::Bencher) {
    let fixture = &*FIXTURE;
    bencher.bench(|| {
        fixture
            .engine
            .decide(&fixture.grant, MODE, POLICY, SANDBOX, &fixture.grants)
    });
}

/// One iteration per path, so a single number covers the whole matrix.
#[divan::bench]
fn decide_mixed(bencher: divan::Bencher) {
    let fixture = &*FIXTURE;
    bencher.bench(|| {
        let allow = fixture
            .engine
            .decide(&fixture.allow_rule, MODE, POLICY, SANDBOX, &[]);
        let readonly = fixture
            .engine
            .decide(&fixture.allow_readonly, MODE, POLICY, SANDBOX, &[]);
        let deny = fixture
            .engine
            .decide(&fixture.deny_rule, MODE, POLICY, SANDBOX, &[]);
        let ask = fixture
            .engine
            .decide(&fixture.ask_rule, MODE, POLICY, SANDBOX, &[]);
        let exec = fixture
            .engine
            .decide(&fixture.ask_exec, MODE, POLICY, SANDBOX, &[]);
        let grant = fixture
            .engine
            .decide(&fixture.grant, MODE, POLICY, SANDBOX, &fixture.grants);
        (allow, readonly, deny, ask, exec, grant)
    });
}
