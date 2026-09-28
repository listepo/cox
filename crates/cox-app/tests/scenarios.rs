//! The timeline fold over real event streams (T37.8, DT§8): each scripted
//! scenario runs a live cox-core session over the Scripted provider, its
//! patches are snapshotted, and the rollout read back through JSONL — as
//! `cox resume` reads it — folds to the very same patches.

#[path = "../../cox-core/tests/common/mod.rs"]
mod common;

use std::path::PathBuf;
use std::time::Duration;

use cox_app::{Block, BlockKind, Controller, Timeline, TimelinePatch, coalesce};
use cox_protocol::Config;
use cox_protocol::traits::Store;
use cox_protocol::types::{Decision, Event, Submission};
use serde_json::Value;

/// What the test does when the turn stops for the person.
#[derive(Clone)]
enum Act {
    Nothing,
    Approve(Decision),
    InterruptAtFirstCall,
}

/// Scenarios from `crates/cox-core/tests/scenarios`, with the config and
/// reaction their own core tests use.
fn cases() -> Vec<(&'static str, Config, Act)> {
    let mut big = Config::default();
    big.context.tool_output_visible_bytes = 120;
    big.context.tool_output_head_lines = 2;
    big.context.tool_output_tail_lines = 2;
    let mut one_turn = Config::default();
    one_turn.core.max_turns = 1;
    let plain = Config::default;
    vec![
        ("text_only", plain(), Act::Nothing),
        ("one_tool", plain(), Act::Nothing),
        ("three_parallel", plain(), Act::Nothing),
        ("big_tool_output", big, Act::Nothing),
        ("provider_error", plain(), Act::Nothing),
        ("max_turns", one_turn, Act::Nothing),
        ("interrupt", plain(), Act::InterruptAtFirstCall),
        ("ask_then_approve", plain(), Act::Approve(Decision::Allow)),
        (
            "ask_then_deny",
            plain(),
            Act::Approve(Decision::Deny {
                reason: "no".into(),
            }),
        ),
        (
            "allow_for_session",
            plain(),
            Act::Approve(Decision::AllowForSession),
        ),
    ]
}

/// One user turn over scenario `name`: the live events, then the rollout.
async fn run(name: &str, config: Config, act: Act) -> (Vec<Event>, Vec<Event>) {
    let (session, store, mut rx) = common::open(&scenario(name), config);
    let running = common::spawn_turn(&session, name);
    let mut live = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("event timeout")
            .expect("event stream open");
        match (&event, &act) {
            (Event::ApprovalRequired { call, .. }, Act::Approve(decision)) => {
                let approve = Submission::Approve {
                    call_id: call.id,
                    decision: decision.clone(),
                };
                session.submit(approve).await.expect("approve");
            }
            (Event::ToolCallRequested { .. }, Act::InterruptAtFirstCall) => {
                session
                    .submit(Submission::Interrupt)
                    .await
                    .expect("interrupt");
            }
            _ => {}
        }
        let done = matches!(event, Event::TurnDone { .. });
        live.push(event);
        if done {
            break;
        }
    }
    let _ = running.await.expect("join");
    let rollout = store.rollout_read(&session.id()).expect("rollout");
    (live, rollout)
}

fn scenario(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../cox-core/tests/scenarios")
        .join(format!("{name}.toml"));
    std::fs::read_to_string(&path).expect("scenario file")
}

fn fold(events: &[Event]) -> Vec<TimelinePatch> {
    let mut timeline = Timeline::new("base16-ocean.dark");
    events.iter().flat_map(|e| timeline.apply(e)).collect()
}

/// The rollout as `cox resume` gets it: one JSON line per event, parsed.
fn through_jsonl(events: &[Event]) -> Vec<Event> {
    let jsonl: String = events
        .iter()
        .map(|e| serde_json::to_string(e).expect("serialize") + "\n")
        .collect();
    jsonl
        .lines()
        .map(|line| serde_json::from_str(line).expect("parse rollout line"))
        .collect()
}

fn is_ulid(s: &str) -> bool {
    s.len() == 26
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"ILOU".contains(&b)))
}

/// Numbers ULIDs by first appearance (`#1`, `#2`, …) — so the snapshot still
/// shows which blocks share a key — and zeroes timings and token estimates.
fn normalize(value: &mut Value, ids: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            let words: Vec<String> = s
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|w| is_ulid(w))
                .map(str::to_owned)
                .collect();
            for word in words {
                let n = match ids.iter().position(|id| *id == word) {
                    Some(n) => n + 1,
                    None => {
                        ids.push(word.clone());
                        ids.len()
                    }
                };
                *s = s.replace(&word, &format!("#{n}"));
            }
        }
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                match k.as_str() {
                    "duration_ms" | "input_tokens" | "output_tokens" | "cache_read_tokens"
                    | "cache_write_tokens" | "latency_ms" => *v = Value::from(0),
                    "cost_usd" => *v = Value::from(0.0),
                    _ => normalize(v, ids),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| normalize(v, ids)),
        _ => {}
    }
}

#[tokio::test]
async fn replay_equals_live() {
    for (name, config, act) in cases() {
        let (live, rollout) = run(name, config, act).await;
        assert_eq!(
            fold(&live),
            fold(&through_jsonl(&rollout)),
            "{name}: the replayed rollout folds differently"
        );
    }
}

#[tokio::test]
async fn patches_match_snapshot_per_scenario() {
    for (name, config, act) in cases() {
        let (live, _) = run(name, config, act).await;
        let mut ids = Vec::new();
        let lines: Vec<String> = fold(&live)
            .iter()
            .map(|patch| {
                let mut value = serde_json::to_value(patch).expect("patch json");
                normalize(&mut value, &mut ids);
                value.to_string()
            })
            .collect();
        insta::assert_snapshot!(name, lines.join("\n"));
    }
}

/// Sixty tool rounds in one user turn: more events than the core's channel
/// (256) holds, so a drain that waited on the consumer would stall the turn.
fn flood() -> String {
    let mut toml = String::new();
    for i in 0..60 {
        toml += &format!(
            "[[turn]]\ntext = \"round {i}\"\ntool_calls = [{{ name = \"echo\", input = {{ text = \"e{i}\" }} }}]\n\n"
        );
    }
    toml + "[[turn]]\ntext = \"done\"\n"
}

fn turn_over(blocks: &[Block]) -> bool {
    blocks
        .iter()
        .any(|b| matches!(&b.kind, BlockKind::TurnMeta { stop: Some(_), .. }))
}

fn mirror(patches: Vec<TimelinePatch>) -> Vec<Block> {
    let mut blocks = Vec::new();
    patches
        .into_iter()
        .for_each(|p| coalesce::apply(&mut blocks, p));
    blocks
}

/// Paused time: the consumer's 2 s naps cost nothing, and the clock only
/// advances past one when every task is idle, so a core blocked on a full
/// channel would finish at 2 s or later.
#[tokio::test(start_paused = true)]
async fn slow_consumer_never_stalls_the_core() {
    let nap = Duration::from_secs(2);
    let mut runs: Vec<(String, String, Config)> = cases()
        .into_iter()
        .filter(|(_, _, act)| matches!(act, Act::Nothing))
        .map(|(name, config, _)| (name.to_owned(), scenario(name), config))
        .collect();
    runs.push(("flood".into(), flood(), Config::default()));
    for (name, toml, config) in runs {
        let (session, store, rx) = common::open(&toml, config);
        let controller = Controller::spawn(Timeline::new("base16-ocean.dark"), rx);
        let start = tokio::time::Instant::now();
        let turn = common::spawn_turn(&session, &name);
        let finished = tokio::spawn(async move {
            let _ = turn.await;
            start.elapsed()
        });
        let (mut blocks, mut widest) = (Vec::new(), 0);
        while !turn_over(&blocks) {
            let batch = tokio::time::timeout(Duration::from_secs(60), controller.next_patches())
                .await
                .expect("patches in time")
                .expect("stream open");
            widest = widest.max(batch.len());
            batch
                .into_iter()
                .for_each(|p| coalesce::apply(&mut blocks, p));
            tokio::time::sleep(nap).await;
        }
        let took = finished.await.expect("join");
        assert!(
            took < nap,
            "{name}: the turn waited on the consumer ({took:?})"
        );
        let events = store.rollout_read(&session.id()).expect("rollout");
        if name == "flood" {
            assert!(events.len() > 256, "flood emitted only {}", events.len());
        }
        assert!(
            widest <= blocks.len(),
            "{name}: a batch of {widest} patches for {} blocks",
            blocks.len()
        );
        assert_eq!(
            blocks,
            mirror(fold(&events)),
            "{name}: coalesced state differs from applying every patch"
        );
    }
}
