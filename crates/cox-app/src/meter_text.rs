//! The token meter and its popover as text (T37.25, DS§7–8, mockup 30):
//! every figure the desktop views show, formatted here so the UI does no
//! arithmetic on them. Separate from `usage.rs`, which folds events into
//! numbers; this only turns a folded `UsageView`, plus the few rates the fold
//! keeps aside, into strings.

use serde::{Deserialize, Serialize};

use crate::summary::{plural, seconds};
use crate::usage::{Tally, UsageView};

/// What the meter and the popover show, figure by figure.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeterText {
    /// Session ↑ sent, `218.5k`.
    pub sent: String,
    /// Session ↓ received, `9.8k`.
    pub received: String,
    /// The turn's tok/s, `71`; empty before the first rate.
    pub rate: String,
    /// The meter read aloud (DS§8): `218 thousand tokens sent, 9.8 thousand
    /// received, 71 tokens per second`.
    pub spoken: String,
    /// `This turn · 4 requests`, `Last turn · 1 request`; empty before a turn.
    pub heading: String,
    /// `streaming` while the turn runs, then `done`.
    pub phase: String,
    /// Beside the big rate: `tok/s now`, or `tok/s last call` once a call's
    /// exact figure replaced the estimate.
    pub rate_unit: String,
    /// `avg 64 tok/s · first token 800 ms · peak 77 tok/s`, what is known.
    pub rate_detail: String,
    /// The turn and session columns (`–` for the turn before one runs).
    pub rows: Vec<MeterRow>,
    /// `Context · 76.4k`, the last call's context.
    pub context: String,
    /// `Cache hit 94% this turn · counts from the provider's usage, …`.
    pub footnote: String,
}

/// One line of the popover's grid.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeterRow {
    pub label: String,
    pub turn: String,
    pub session: String,
    /// A breakdown of the row above it, drawn indented and quieter.
    pub detail: bool,
}

/// Figures the fold keeps beside `UsageView` for the popover alone.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Rates {
    /// The turn's received tokens over its calls' streaming time.
    pub avg: Option<f64>,
    pub peak: Option<f64>,
    pub session_thinking: u32,
}

impl MeterText {
    pub(crate) fn of(view: &UsageView, rates: Rates) -> Self {
        let s = &view.session;
        let turn = view.turn.as_ref();
        let rate = turn.and_then(|t| t.tok_per_s).map(per_second);
        let mut spoken = format!(
            "{} tokens sent, {} received",
            spoken(s.sent),
            spoken(s.received)
        );
        if let Some(r) = &rate {
            spoken += &format!(", {r} tokens per second");
        }
        let detail = [
            rates.avg.map(|r| format!("avg {} tok/s", per_second(r))),
            turn.and_then(|t| t.ttft_ms)
                .map(|ms| format!("first token {}", seconds(ms))),
            rates.peak.map(|r| format!("peak {} tok/s", per_second(r))),
        ];
        let t = turn.map(|t| (t.tally, t.thinking_tokens));
        let col = |f: fn(&Tally) -> String| t.map_or_else(|| "–".into(), |(t, _)| f(&t));
        let row = |label: &str, f: fn(&Tally) -> String, detail| MeterRow {
            label: label.into(),
            turn: col(f),
            session: f(s),
            detail,
        };
        let rows = vec![
            row("↑ Sent", |t| tokens(t.sent), false),
            row("cache read", |t| tokens(t.cache_read), true),
            row("cache write", |t| tokens(t.cache_write), true),
            row("uncached", |t| tokens(t.uncached), true),
            row("↓ Received", |t| tokens(t.received), false),
            MeterRow {
                label: "of it thinking".into(),
                turn: t.map_or_else(|| "–".into(), |(_, n)| tokens(n)),
                session: tokens(rates.session_thinking),
                detail: true,
            },
            row("Cost", |t| format!("${:.2}", t.cost_usd), false),
        ];
        let source = if s.estimated {
            "some counts are cox's estimate"
        } else {
            "counts from the provider's usage, one ledger row per request"
        };
        let footnote = match t.filter(|(t, _)| t.sent > 0) {
            Some((t, _)) => {
                let hit = f64::from(t.cache_read) / f64::from(t.sent) * 100.0;
                format!("Cache hit {hit:.0}% this turn · {source}")
            }
            None => format!("C{}", &source[1..]),
        };
        Self {
            sent: tokens(s.sent),
            received: tokens(s.received),
            rate: rate.unwrap_or_default(),
            spoken,
            heading: turn.map_or_else(String::new, |t| {
                let when = if t.done { "Last" } else { "This" };
                let calls = plural(u64::from(t.tally.calls), "request", "requests");
                format!("{when} turn · {calls}")
            }),
            phase: turn
                .map_or("", |t| if t.done { "done" } else { "streaming" })
                .into(),
            rate_unit: if turn.is_some_and(|t| t.exact) {
                "tok/s last call"
            } else {
                "tok/s now"
            }
            .into(),
            rate_detail: detail.into_iter().flatten().collect::<Vec<_>>().join(" · "),
            rows,
            context: format!("Context · {}", tokens(view.context_tokens)),
            footnote,
        }
    }
}

/// `950`, `9.8k`, `218.5k`, `1.2M`.
fn tokens(n: u32) -> String {
    match n {
        0..1000 => n.to_string(),
        1000..1_000_000 => format!("{:.1}k", f64::from(n) / 1e3),
        _ => format!("{:.1}M", f64::from(n) / 1e6),
    }
}

/// `tokens` as VoiceOver should say it: `950`, `9.8 thousand`,
/// `218 thousand`, `1.2 million`.
fn spoken(n: u32) -> String {
    match n {
        0..1000 => n.to_string(),
        1000..10_000 => format!("{:.1} thousand", f64::from(n) / 1e3),
        10_000..1_000_000 => format!("{} thousand", n / 1000),
        _ => format!("{:.1} million", f64::from(n) / 1e6),
    }
}

/// A rate in whole tokens per second, `71`.
fn per_second(rate: f64) -> String {
    format!("{rate:.0}")
}

#[cfg(test)]
mod tests {
    use cox_protocol::ids::TurnId;

    use super::*;
    use crate::usage::TurnUsage;

    fn tally(sent: u32, received: u32) -> Tally {
        Tally {
            sent,
            received,
            cache_read: sent / 10 * 9,
            cache_write: 0,
            uncached: sent / 10,
            cost_usd: 0.4249,
            calls: 4,
            estimated: false,
        }
    }

    #[test]
    fn numbers_read_as_the_mockup_and_ds_8_say() {
        let cases = [(950, "950", "950"), (9_800, "9.8k", "9.8 thousand")];
        for (n, shown, said) in cases {
            assert_eq!((tokens(n), spoken(n)), (shown.into(), said.into()));
        }
        assert_eq!(
            (tokens(218_500), spoken(218_500)),
            ("218.5k".into(), "218 thousand".into())
        );
        assert_eq!(tokens(1_200_000), "1.2M");
    }

    #[test]
    fn a_streaming_turn_formats_every_figure_the_popover_shows() {
        let view = UsageView {
            session: tally(218_500, 9_800),
            turn: Some(TurnUsage {
                turn: TurnId::new(),
                tally: tally(41_600, 1_900),
                thinking_tokens: 600,
                ttft_ms: Some(800),
                tok_per_s: Some(71.4),
                exact: false,
                sparkline: vec![],
                done: false,
            }),
            context_tokens: 76_400,
            text: MeterText::default(),
        };
        let rates = Rates {
            avg: Some(64.2),
            peak: Some(77.0),
            session_thinking: 3_100,
        };
        let text = MeterText::of(&view, rates);
        assert_eq!(
            text.spoken,
            "218 thousand tokens sent, 9.8 thousand received, 71 tokens per second"
        );
        assert_eq!(
            (text.sent, text.received, text.rate),
            ("218.5k".into(), "9.8k".into(), "71".into())
        );
        assert_eq!(
            (text.heading.as_str(), text.phase.as_str()),
            ("This turn · 4 requests", "streaming")
        );
        assert_eq!(
            text.rate_detail,
            "avg 64 tok/s · first token 800 ms · peak 77 tok/s"
        );
        let thinking = &text.rows[5];
        assert_eq!(
            (thinking.turn.as_str(), thinking.session.as_str()),
            ("600", "3.1k")
        );
        assert_eq!(
            (text.rows[6].turn.as_str(), text.rows[6].session.as_str()),
            ("$0.42", "$0.42")
        );
        assert_eq!(text.context, "Context · 76.4k");
        assert!(
            text.footnote
                .starts_with("Cache hit 90% this turn · counts"),
            "{}",
            text.footnote
        );
    }

    #[test]
    fn before_any_turn_the_turn_column_is_a_dash_and_no_rate_is_spoken() {
        let text = MeterText::of(&UsageView::default(), Rates::default());
        assert_eq!(text.spoken, "0 tokens sent, 0 received");
        assert_eq!((text.rate.as_str(), text.heading.as_str()), ("", ""));
        assert!(text.rows.iter().all(|r| r.turn == "–"));
        assert!(text.footnote.starts_with("Counts from"));
    }
}
