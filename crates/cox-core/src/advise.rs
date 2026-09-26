//! The session side of decision points (PL§4 "Decision points", T33.20):
//! asks the advisor `[plugins.decide]` names, applies the point's monotone
//! rule from `router`, and records every answer as `Event::Advised`.
//! Separate from `session.rs` because this is the one place a turn waits on
//! a plugin's answer, and from `router.rs` because routing stays pure.

use std::time::Duration;

use cox_protocol::errors::CoreError;
use cox_protocol::plugin::{DecidePoint, Question};
use cox_protocol::types::{Event, Job, Tier};
use cox_sanitize::redact::scrub;
use serde_json::json;

use crate::router::{self, Route};
use crate::session::Session;

/// How much of the prompt a `route` question carries (J5.2: ≤ 2k tokens).
const PROMPT_CHARS: usize = 8_000;

impl Session {
    /// This user turn's route: `route` advice applied to the static pick
    /// `route`, which stands on silence, lateness, low confidence or when
    /// the point is off. A followed tier is kept in `Inner::routed` so every
    /// provider call of the turn uses it (`route_for`).
    pub(crate) async fn route_turn(&self, route: Route, prompt: &str) -> Result<Route, CoreError> {
        let decide = &self.config.plugins.decide;
        let Some(plugin) = decide.route.as_deref() else {
            return Ok(route);
        };
        let Some(advisor) = self.advisor(plugin) else {
            return Ok(route);
        };
        let offer = router::route_offer(route.tier);
        if offer.iter().all(|tier| *tier == route.tier) {
            return Ok(route);
        }
        let question = Question {
            point: DecidePoint::Route,
            state: json!({
                "static": tier_name(route.tier),
                // The same redaction the rollout gets (J3: state is built
                // from scrubbed text), clipped to the question's size.
                "prompt": scrub(prompt).chars().take(PROMPT_CHARS).collect::<String>(),
            }),
            options: offer.iter().copied().map(tier_name).collect(),
        };
        let budget = Duration::from_millis(decide.route_ms);
        // The core holds the budget itself, so an advisor that overruns its
        // own deadline is late, never a slow turn.
        let Ok(Some(mut advice)) =
            tokio::time::timeout(budget, advisor.advise(question, budget)).await
        else {
            return Ok(route);
        };
        // `note` belongs to `approve_hint`; untrusted text with no use here
        // stays out of the rollout.
        advice.note = None;
        let (tier, mut applied) =
            router::apply_route(route.tier, &offer, &advice, decide.min_confidence);
        let mut chosen = route;
        if tier != chosen.tier {
            self.inner.lock().await.routed = Some(tier);
            match self.route_for(Job::Main, true).await {
                Ok(advised) => chosen = advised,
                // A tier that cannot route (bad provider name) is no reason
                // to fail a turn the static pick can run.
                Err(_) => {
                    self.inner.lock().await.routed = None;
                    applied = false;
                }
            }
        }
        self.emit(Event::Advised {
            point: DecidePoint::Route,
            plugin: plugin.to_string(),
            advice,
            applied,
        })
        .await?;
        Ok(chosen)
    }
}

/// A tier's wire name (`"cheap"`), the form `Question.options` carries.
fn tier_name(tier: Tier) -> String {
    serde_json::to_value(tier)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use cox_protocol::Store;
    use cox_protocol::plugin::{Advice, Answer};
    use cox_protocol::traits::Advisor;
    use cox_protocol::types::Submission;
    use cox_provider::scripted::Scripted;

    use super::*;
    use crate::rollout::History;
    use crate::session::MemoryStore;

    struct Fake {
        delay: Duration,
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
            tokio::time::sleep(self.delay).await;
            Some(self.advice.clone())
        }
    }

    fn cheap(confidence: f64) -> Advice {
        Advice {
            answer: Answer::Choice { order: vec![0] },
            confidence: Some(confidence),
            note: Some("\u{1b}[31mignored".into()),
        }
    }

    /// One user turn under an advisor; returns the rollout, the ledger's
    /// tiers and the questions the advisor saw.
    async fn turn(delay: Duration, advice: Advice) -> (Vec<Event>, Vec<Tier>, Vec<Question>) {
        let mut config = cox_protocol::Config::default();
        config.plugins.decide.route = Some("fake".into());
        config.plugins.decide.route_ms = 50;
        let provider =
            Arc::new(Scripted::from_toml("[[turn]]\ntext = \"done\"\n", "").expect("scenario"));
        let store = Arc::new(MemoryStore::new());
        let session = Session::new(
            config,
            provider,
            vec![],
            store.clone(),
            store.clone(),
            PathBuf::from("/tmp/cox-advise"),
        )
        .expect("session");
        let fake = Arc::new(Fake {
            delay,
            advice,
            asked: Mutex::new(Vec::new()),
        });
        session.set_advisors(vec![fake.clone()]);
        session
            .submit(Submission::UserTurn {
                text: "rename foo to bar".into(),
                attachments: vec![],
                confirm_think: false,
            })
            .await
            .expect("turn");
        assert_eq!(
            session.inner.lock().await.routed,
            None,
            "advice outlives its turn"
        );
        let events = store.rollout_read(&session.id()).expect("rollout");
        let tiers = store.usage_rows().iter().map(|r| r.tier).collect();
        let asked = fake.asked.lock().unwrap_or_else(|e| e.into_inner()).clone();
        (events, tiers, asked)
    }

    fn started_tier(events: &[Event]) -> Option<Tier> {
        events.iter().find_map(|e| match e {
            Event::TurnStarted { tier, .. } => Some(*tier),
            _ => None,
        })
    }

    fn advised(events: &[Event]) -> Vec<&Event> {
        events
            .iter()
            .filter(|e| matches!(e, Event::Advised { .. }))
            .collect()
    }

    #[tokio::test]
    async fn late_advice_falls_back_to_static_pick() {
        let (events, tiers, asked) = turn(Duration::from_millis(500), cheap(0.99)).await;
        assert_eq!(asked.len(), 1, "the point was asked");
        assert_eq!(started_tier(&events), Some(Tier::Code));
        assert_eq!(tiers, vec![Tier::Code]);
        assert!(advised(&events).is_empty(), "a late answer is no answer");
    }

    #[tokio::test]
    async fn low_confidence_advice_is_recorded_but_not_applied() {
        let (events, tiers, _) = turn(Duration::ZERO, cheap(0.3)).await;
        assert_eq!(started_tier(&events), Some(Tier::Code));
        assert_eq!(tiers, vec![Tier::Code]);
        assert!(matches!(
            advised(&events)[..],
            [Event::Advised { applied: false, .. }]
        ));
    }

    #[tokio::test]
    async fn advised_event_in_rollout_replays_identically() {
        let (events, tiers, asked) = turn(Duration::ZERO, cheap(0.9)).await;
        assert_eq!(asked[0].options, vec!["cheap", "code"]);
        assert_eq!(asked[0].state["static"], "code");
        // The whole turn, every provider call included, ran on the advice.
        assert_eq!(started_tier(&events), Some(Tier::Cheap));
        assert_eq!(tiers, vec![Tier::Cheap]);
        let [
            Event::Advised {
                point,
                plugin,
                advice,
                applied,
            },
        ] = advised(&events)[..]
        else {
            panic!("one Advised event: {events:?}");
        };
        assert_eq!(
            (point, plugin.as_str(), applied),
            (&DecidePoint::Route, "fake", &true)
        );
        assert_eq!(advice.note, None);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::ModelSwitched { .. }))
        );
        // The rollout line round-trips byte-for-byte, and replaying with or
        // without it rebuilds the same history (D12: replay without the
        // plugin gives the same log).
        for ev in &events {
            let line = serde_json::to_string(ev).expect("serialize");
            let back: Event = serde_json::from_str(&line).expect("deserialize");
            assert_eq!(serde_json::to_string(&back).expect("serialize"), line);
        }
        let without: Vec<Event> = events
            .iter()
            .filter(|e| !matches!(e, Event::Advised { .. }))
            .cloned()
            .collect();
        assert_eq!(
            History::from_rollout(&events, false),
            History::from_rollout(&without, false)
        );
    }
}
