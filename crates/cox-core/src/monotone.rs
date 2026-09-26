//! The decision points after `route` (PL§4 "Decision points", T33.21):
//! `risk`, `approve_hint`, `compact` and `rank`. Each asks the advisor
//! `[plugins.decide]` names and applies the point's monotone rule here, in
//! the core, so no answer moves a decision the unsafe way: risk only up, a
//! caution only, compaction only earlier, rank only over cox's own
//! candidates. The permission engine still makes every tool decision; `risk`
//! only changes the call it judges. Separate from `advise.rs` so these
//! guards and routing change independently.

use std::time::Duration;

use cox_protocol::errors::CoreError;
use cox_protocol::plugin::{Advice, Answer, DecidePoint, Question};
use cox_protocol::types::{Event, Level, Risk, ToolCall, Why};
use cox_sanitize::redact::scrub;
use cox_sanitize::sanitize;
use serde_json::json;

use crate::permission::Outcome;
use crate::session::Session;

/// How much of a call's input a question carries (J5.1: never tool output).
const INPUT_CHARS: usize = 2_000;
/// How much of a caution note reaches the user.
const NOTE_CHARS: usize = 200;

impl Session {
    /// One answer from `plugin` for `point` within `ms`, or `None` when the
    /// point is off, the plugin is not live, or it is silent or late.
    async fn ask_point(
        &self,
        plugin: Option<&str>,
        ms: u64,
        question: Question,
    ) -> Option<(String, Advice)> {
        let plugin = plugin?;
        let advisor = self.advisor(plugin)?;
        let budget = Duration::from_millis(ms);
        // The core holds the budget itself, as for `route`.
        let advice = tokio::time::timeout(budget, advisor.advise(question, budget))
            .await
            .ok()??;
        Some((plugin.to_string(), advice))
    }

    async fn advised(
        &self,
        point: DecidePoint,
        plugin: String,
        mut advice: Advice,
        applied: bool,
    ) -> Result<(), CoreError> {
        // Only an applied `approve_hint` note is shown; any other note is
        // untrusted text with no use and stays out of the rollout.
        if point != DecidePoint::ApproveHint || !applied {
            advice.note = None;
        }
        self.emit(Event::Advised {
            point,
            plugin,
            advice,
            applied,
        })
        .await
    }

    /// `call` with `risk` advice applied, before `Engine::decide` judges it.
    /// Asked only when raising the call to `Destructive` would turn the
    /// engine's `Allow` into `Ask` or `Deny` (J5.1), so a read-only call, a
    /// call already asked, or `Bypass` never waits on a plugin.
    pub(crate) async fn advise_risk(&self, mut call: ToolCall) -> Result<ToolCall, CoreError> {
        let decide = &self.config.plugins.decide;
        if decide.risk.is_none() || matches!(call.risk, Risk::ReadOnly | Risk::Destructive) {
            return Ok(call);
        }
        let raised = ToolCall {
            risk: Risk::Destructive,
            ..call.clone()
        };
        let allowed = |o: Outcome| matches!(o, Outcome::Allow { .. });
        if !allowed(self.decide(&call).await) || allowed(self.decide(&raised).await) {
            return Ok(call);
        }
        let question = Question {
            point: DecidePoint::Risk,
            state: json!({
                "tool": call.name,
                "subject": clip(&call.subject, INPUT_CHARS),
                "input": clip(&call.input.to_string(), INPUT_CHARS),
                "classifier_risk": call.risk,
            }),
            options: vec![],
        };
        let Some((plugin, advice)) = self
            .ask_point(decide.risk.as_deref(), decide.risk_ms, question)
            .await
        else {
            return Ok(call);
        };
        let (risk, applied) = raise_risk(call.risk, &advice, decide.min_confidence);
        call.risk = risk;
        self.advised(DecidePoint::Risk, plugin, advice, applied)
            .await?;
        Ok(call)
    }

    /// `approve_hint` for a call about to be shown for approval: a caution
    /// notice the core frames itself, or nothing. Warning-only (§14
    /// decision 10), so no answer can read as "looks safe".
    pub(crate) async fn advise_approval(
        &self,
        call: &ToolCall,
        why: &Why,
    ) -> Result<(), CoreError> {
        let decide = &self.config.plugins.decide;
        let question = Question {
            point: DecidePoint::ApproveHint,
            state: json!({
                "tool": call.name,
                "subject": clip(&call.subject, INPUT_CHARS),
                "input": clip(&call.input.to_string(), INPUT_CHARS),
                "risk": call.risk,
                "why": why,
            }),
            options: vec![],
        };
        let Some((plugin, mut advice)) = self
            .ask_point(
                decide.approve_hint.as_deref(),
                decide.approve_hint_ms,
                question,
            )
            .await
        else {
            return Ok(());
        };
        let applied = yes(&advice, decide.min_confidence);
        advice.note = advice
            .note
            .as_deref()
            .map(|n| sanitize(n).chars().take(NOTE_CHARS).collect::<String>())
            .filter(|n| !n.trim().is_empty());
        if applied {
            let reason = advice.note.as_deref().unwrap_or("check this call closely");
            self.emit(Event::Notice {
                level: Level::Warn,
                text: format!("caution from plugin {plugin}: {reason}"),
            })
            .await?;
        }
        self.advised(DecidePoint::ApproveHint, plugin, advice, applied)
            .await
    }

    /// Whether to compact at a turn's start. A `mandatory` compaction runs
    /// without asking, so no answer can skip it; otherwise `compact` advice
    /// may only bring it earlier, and is asked only when there is history
    /// beyond `keep_turns` to compact.
    pub(crate) async fn compact_now(
        &self,
        mandatory: bool,
        context_tokens: u32,
        max_context: u32,
    ) -> Result<bool, CoreError> {
        if mandatory {
            return Ok(true);
        }
        let decide = &self.config.plugins.decide;
        let turns = self.inner.lock().await.turn_marks.len();
        if decide.compact.is_none() || turns <= self.config.context.keep_turns as usize {
            return Ok(false);
        }
        let question = Question {
            point: DecidePoint::Compact,
            state: json!({
                "context_tokens": context_tokens,
                "max_context": max_context,
                "compact_at": self.config.context.compact_at,
                "turns": turns,
            }),
            options: vec![],
        };
        let Some((plugin, advice)) = self
            .ask_point(decide.compact.as_deref(), decide.compact_ms, question)
            .await
        else {
            return Ok(false);
        };
        let now = yes(&advice, decide.min_confidence);
        self.advised(DecidePoint::Compact, plugin, advice, now)
            .await?;
        Ok(now)
    }

    /// `found` (what `tool_search` returned for `query`) after `rank`
    /// advice: reordered or filtered, never added to.
    pub(crate) async fn advise_rank(&self, query: &str, found: Vec<String>) -> Vec<String> {
        let decide = &self.config.plugins.decide;
        let question = Question {
            point: DecidePoint::Rank,
            state: json!({ "query": clip(query, INPUT_CHARS) }),
            options: found.clone(),
        };
        let Some((plugin, advice)) = self
            .ask_point(decide.rank.as_deref(), decide.rank_ms, question)
            .await
        else {
            return found;
        };
        let (ranked, applied) = rank(&found, &advice, decide.min_confidence);
        // A rollout write failure is the emitter's to report; the call's
        // result stands either way.
        let _ = self
            .advised(DecidePoint::Rank, plugin, advice, applied)
            .await;
        ranked
    }
}

/// The same redaction the rollout gets, clipped to a question's size.
fn clip(text: &str, chars: usize) -> String {
    scrub(text).chars().take(chars).collect()
}

fn confident(advice: &Advice, min_confidence: f64) -> bool {
    advice.confidence.is_some_and(|c| c >= min_confidence)
}

/// A yes/no point's verdict: yes only when `p_yes` reaches `min_confidence`
/// and a stated confidence does too (J11: a yes/no may give none).
fn yes(advice: &Advice, min_confidence: f64) -> bool {
    let sure = advice.confidence.is_none_or(|c| c >= min_confidence);
    matches!(advice.answer, Answer::Noul { p_yes } if p_yes >= min_confidence) && sure
}

/// `Risk` on the J5.1 severity scale: 0 read-only … 3 destructive.
fn level(risk: Risk) -> u8 {
    match risk {
        Risk::ReadOnly => 0,
        Risk::Write => 1,
        Risk::Exec => 2,
        Risk::Destructive => 3,
    }
}

/// `max(builtin, advised)`: a confident severity `Score` rounded onto the
/// scale; anything else keeps `builtin`. Returns the risk and whether the
/// advice raised it.
fn raise_risk(builtin: Risk, advice: &Advice, min_confidence: f64) -> (Risk, bool) {
    let advised = match advice.answer {
        Answer::Score { value } if confident(advice, min_confidence) && value.is_finite() => {
            [Risk::ReadOnly, Risk::Write, Risk::Exec, Risk::Destructive]
                .into_iter()
                .rev()
                .find(|r| f64::from(level(*r)) - 0.5 <= value)
        }
        _ => None,
    };
    match advised {
        Some(risk) if level(risk) > level(builtin) => (risk, true),
        _ => (builtin, false),
    }
}

/// `candidates` in a confident `Choice`'s order, keeping only the indices
/// it names once each; anything else keeps `candidates` as they are.
fn rank(candidates: &[String], advice: &Advice, min_confidence: f64) -> (Vec<String>, bool) {
    let Answer::Choice { order } = &advice.answer else {
        return (candidates.to_vec(), false);
    };
    if !confident(advice, min_confidence) {
        return (candidates.to_vec(), false);
    }
    let mut out: Vec<String> = Vec::new();
    for i in order {
        let Some(name) = usize::try_from(*i).ok().and_then(|i| candidates.get(i)) else {
            continue;
        };
        if !out.contains(name) {
            out.push(name.clone());
        }
    }
    (out, true)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use cox_protocol::ids::{CallId, ItemId};
    use cox_protocol::traits::Advisor;
    use cox_protocol::types::PermissionMode;
    use cox_provider::scripted::Scripted;
    use tokio::sync::mpsc;

    use super::*;
    use crate::compact::TurnMark;
    use crate::session::MemoryStore;

    struct Fake {
        advice: Advice,
        asked: Mutex<Vec<Question>>,
    }

    #[async_trait]
    impl Advisor for Fake {
        fn id(&self) -> &str {
            "fake"
        }

        async fn advise(&self, question: Question, _budget: Duration) -> Option<Advice> {
            self.asked
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(question);
            Some(self.advice.clone())
        }
    }

    impl Fake {
        fn asked(&self) -> usize {
            self.asked.lock().unwrap_or_else(|e| e.into_inner()).len()
        }
    }

    fn advice(answer: Answer, confidence: Option<f64>, note: &str) -> Advice {
        Advice {
            answer,
            confidence,
            note: Some(note.into()),
        }
    }

    /// A session whose every decision point is answered by one `Fake`.
    fn open(mode: PermissionMode, advice: Advice) -> (Session, Arc<Fake>, mpsc::Receiver<Event>) {
        let mut config = cox_protocol::Config::default();
        config.permissions.mode = mode;
        let decide = &mut config.plugins.decide;
        for point in [
            &mut decide.risk,
            &mut decide.approve_hint,
            &mut decide.compact,
            &mut decide.rank,
        ] {
            *point = Some("fake".into());
        }
        let provider = Arc::new(Scripted::from_toml("", "").expect("scenario"));
        let store = Arc::new(MemoryStore::new());
        let session = Session::new(
            config,
            provider,
            vec![],
            store.clone(),
            store,
            PathBuf::from("/tmp/cox-monotone"),
        )
        .expect("session");
        let fake = Arc::new(Fake {
            advice,
            asked: Mutex::new(Vec::new()),
        });
        session.set_advisors(vec![fake.clone()]);
        let rx = session.events().expect("events once");
        (session, fake, rx)
    }

    fn call(risk: Risk) -> ToolCall {
        ToolCall {
            id: CallId::new(),
            name: "bash".into(),
            input: json!({ "command": "git push --force origin main" }),
            subject: "git push --force origin main".into(),
            risk,
        }
    }

    fn drain(rx: &mut mpsc::Receiver<Event>) -> Vec<Event> {
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    fn advised(events: &[Event]) -> Vec<(bool, Option<String>)> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Advised {
                    applied, advice, ..
                } => Some((*applied, advice.note.clone())),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn risk_advice_cannot_lower_risk() {
        let score = |value, confidence| advice(Answer::Score { value }, confidence, "");
        assert_eq!(
            raise_risk(Risk::Exec, &score(0.0, Some(1.0)), 0.6),
            (Risk::Exec, false)
        );
        assert_eq!(
            raise_risk(Risk::Exec, &score(-9.0, Some(1.0)), 0.6),
            (Risk::Exec, false)
        );
        assert_eq!(
            raise_risk(Risk::Write, &score(3.0, Some(0.5)), 0.6),
            (Risk::Write, false)
        );
        assert_eq!(
            raise_risk(Risk::Write, &score(3.0, None), 0.6),
            (Risk::Write, false)
        );
        let noul = advice(Answer::Noul { p_yes: 0.0 }, Some(1.0), "");
        assert_eq!(raise_risk(Risk::Exec, &noul, 0.6), (Risk::Exec, false));
        assert_eq!(
            raise_risk(Risk::Write, &score(2.5, Some(0.9)), 0.6),
            (Risk::Destructive, true)
        );

        // Through the session: a "harmless" answer leaves the call as the
        // classifier left it, and a raise is judged by the engine.
        let (session, fake, mut rx) = open(PermissionMode::Auto, score(0.0, Some(1.0)));
        let kept = session.advise_risk(call(Risk::Write)).await.expect("risk");
        assert_eq!((kept.risk, fake.asked()), (Risk::Write, 1));
        assert!(matches!(session.decide(&kept).await, Outcome::Allow { .. }));
        assert_eq!(advised(&drain(&mut rx)), vec![(false, None)]);

        let (session, _, mut rx) = open(PermissionMode::Auto, score(3.0, Some(0.9)));
        let raised = session.advise_risk(call(Risk::Write)).await.expect("risk");
        assert_eq!(raised.risk, Risk::Destructive);
        assert!(matches!(session.decide(&raised).await, Outcome::Ask(_)));
        assert_eq!(advised(&drain(&mut rx)), vec![(true, None)]);
    }

    #[tokio::test]
    async fn risk_not_asked_when_outcome_would_not_change() {
        let raise = advice(Answer::Score { value: 3.0 }, Some(1.0), "");
        // Already asked (Write outside `Auto`), always allowed (`Bypass`),
        // read-only, or already destructive: the answer could change nothing.
        for (mode, risk) in [
            (PermissionMode::Default, Risk::Write),
            (PermissionMode::Bypass, Risk::Write),
            (PermissionMode::Auto, Risk::ReadOnly),
            (PermissionMode::Auto, Risk::Destructive),
        ] {
            let (session, fake, mut rx) = open(mode, raise.clone());
            let out = session.advise_risk(call(risk)).await.expect("risk");
            assert_eq!((out.risk, fake.asked()), (risk, 0), "{mode:?} {risk:?}");
            assert!(advised(&drain(&mut rx)).is_empty());
        }
    }

    #[tokio::test]
    async fn approve_hint_cannot_say_looks_safe() {
        let why = Why::Risk { risk: Risk::Exec };
        for safe in [
            advice(Answer::Noul { p_yes: 0.02 }, Some(0.99), "looks safe"),
            advice(Answer::Score { value: 0.0 }, Some(0.99), "looks safe"),
            advice(Answer::Choice { order: vec![0] }, Some(0.99), "looks safe"),
            advice(Answer::Noul { p_yes: 0.9 }, Some(0.1), "looks safe"),
        ] {
            let (session, _, mut rx) = open(PermissionMode::Default, safe);
            session
                .advise_approval(&call(Risk::Exec), &why)
                .await
                .expect("hint");
            let events = drain(&mut rx);
            assert!(!events.iter().any(|e| matches!(e, Event::Notice { .. })));
            // The unused note never reaches the rollout either.
            assert_eq!(advised(&events), vec![(false, None)]);
        }
        let caution = advice(
            Answer::Noul { p_yes: 0.9 },
            None,
            "pushes \u{1b}[31mto main",
        );
        let (session, _, mut rx) = open(PermissionMode::Default, caution);
        session
            .advise_approval(&call(Risk::Exec), &why)
            .await
            .expect("hint");
        let events = drain(&mut rx);
        let notices: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                Event::Notice {
                    level: Level::Warn,
                    text,
                } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(notices, vec!["caution from plugin fake: pushes to main"]);
    }

    #[tokio::test]
    async fn compact_advice_cannot_skip_mandatory_compaction() {
        let never = advice(Answer::Noul { p_yes: 0.0 }, Some(1.0), "");
        let (session, fake, _rx) = open(PermissionMode::Default, never);
        let mark = TurnMark {
            item: ItemId::new(),
            start: 0,
            seq: 1,
        };
        session.inner.lock().await.turn_marks = vec![mark; 3];
        assert!(session.compact_now(true, 900, 1000).await.expect("compact"));
        assert_eq!(fake.asked(), 0, "a due compaction is not up for a vote");
        assert!(
            !session
                .compact_now(false, 100, 1000)
                .await
                .expect("compact")
        );
        assert_eq!(fake.asked(), 1);

        // Earlier is allowed, but only when there is something to compact.
        let now = advice(Answer::Noul { p_yes: 0.9 }, Some(0.9), "");
        let (session, fake, _rx) = open(PermissionMode::Default, now);
        assert!(
            !session
                .compact_now(false, 100, 1000)
                .await
                .expect("compact")
        );
        assert_eq!(fake.asked(), 0, "nothing beyond keep_turns");
        session.inner.lock().await.turn_marks = vec![mark; 3];
        assert!(
            session
                .compact_now(false, 100, 1000)
                .await
                .expect("compact")
        );
    }

    #[tokio::test]
    async fn rank_advice_cannot_add_tools() {
        let found: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        let choice =
            |order: Vec<u32>, confidence| advice(Answer::Choice { order }, Some(confidence), "");
        assert_eq!(
            rank(&found, &choice(vec![2, 7, 0, 2, u32::MAX], 0.9), 0.6),
            (vec!["c".to_string(), "a".to_string()], true)
        );
        assert_eq!(rank(&found, &choice(vec![], 0.9), 0.6), (vec![], true));
        assert_eq!(
            rank(&found, &choice(vec![1], 0.1), 0.6),
            (found.clone(), false)
        );

        let (session, fake, mut rx) = open(PermissionMode::Default, choice(vec![9, 1], 0.9));
        let ranked = session.advise_rank("github issue", found.clone()).await;
        assert_eq!(ranked, vec!["b".to_string()]);
        let asked = fake.asked.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(asked[0].options, found);
        assert_eq!(advised(&drain(&mut rx)), vec![(true, None)]);
    }
}
