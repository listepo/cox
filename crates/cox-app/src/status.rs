//! The fold behind `TimelinePatch::Status` (T37.24.7): the composer's mode,
//! model and effort chips, and the queue count. Seeded from the config the
//! session opened with — the core records its opening mode to the rollout
//! only (T50.4), because every surface already has it from that config —
//! then kept by `StateChanged`, `TurnStarted` and `ModelSwitched`; the
//! model's display name comes from the model catalog (A111). Separate
//! from `Timeline`, whose fold is the block list, and from `Controller`,
//! which only queues what this says changed.

use std::collections::HashMap;

use cox_core::permission::next_mode;
use cox_protocol::Config;
use cox_protocol::types::{Effort, Event, Job, ModelId, PermissionMode, Tier};

use crate::patch::Status;

/// Model id → display name for every catalog row that has one (A111). The
/// built-in rows are under `config`'s, so an entry that names nothing keeps
/// models.dev's name; a catalog that fails to load names nothing, and the
/// app shows the id.
pub(crate) fn model_names(config: &Config) -> HashMap<String, String> {
    cox_models::Catalog::load(config, &[], None)
        .map(|catalog| {
            catalog
                .rows()
                .filter_map(|r| Some((r.id.clone(), r.display_name.clone()?)))
                .collect()
        })
        .unwrap_or_default()
}

/// Brand words a vendor puts before every model it names, dropped because
/// the chip already sits beside that vendor's session. Only where the rest
/// still names the model on its own: `GPT-5.1` has no separate prefix, and
/// `Grok 4.3` or `DeepSeek V4 Pro` without theirs would not.
const VENDOR_PREFIXES: [&str; 1] = ["Claude "];

/// The alias marker models.dev appends to a name; the chip has no room for
/// it and the version already says which model runs.
const LATEST_SUFFIX: &str = " (latest)";

/// A catalog name as the desktop's chip, pop-ups and menu show it (A111,
/// A116, A129): without its vendor prefix or a trailing ` (latest)`,
/// `Claude Haiku 4.5 (latest)` → `Haiku 4.5`; a marker that is the whole
/// name stays. `None` for an empty name, and the client shows the id. Its
/// own field beside the full name, which the TUI and ACP keep showing.
pub(crate) fn short_name(name: &str) -> Option<String> {
    let whole = |rest: &&str| !rest.is_empty();
    let name = name
        .strip_suffix(LATEST_SUFFIX)
        .filter(whole)
        .unwrap_or(name);
    let name = VENDOR_PREFIXES
        .iter()
        .find_map(|p| name.strip_prefix(p).filter(whole))
        .unwrap_or(name);
    (!name.is_empty()).then(|| name.to_owned())
}

/// The status and what it needs to put back when an override is cleared.
#[derive(Debug, Clone, Default)]
pub struct StatusFold {
    status: Status,
    /// The `code` tier's configured effort, which `SetEffort { None }`
    /// restores.
    tier_effort: Option<Effort>,
    /// Model id → display name, for every catalog row that has one.
    names: HashMap<String, String>,
}

impl StatusFold {
    /// The status a session opened with `config` starts in.
    pub fn open(config: &Config) -> Self {
        let code = &config.tiers.code;
        let names = model_names(config);
        let mut fold = Self {
            status: Status::default(),
            tier_effort: Some(code.effort),
            names,
        };
        fold.name(ModelId(code.model.clone()));
        fold.set(config.permissions.mode, None);
        fold
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Changes the queue count.
    pub fn queue(&mut self, change: impl FnOnce(&mut u32)) {
        change(&mut self.status.queued);
    }

    /// Folds `event` in; says whether the status changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        let before = self.status.clone();
        match event {
            Event::StateChanged { mode, effort } => self.set(*mode, *effort),
            // A subagent's or compaction's turn is not the one the chip names.
            Event::TurnStarted {
                job: Job::Main,
                model,
                ..
            } => self.name(model.clone()),
            Event::ModelSwitched {
                tier: Tier::Code,
                to,
                ..
            } => self.name(to.clone()),
            _ => {}
        }
        self.status != before
    }

    fn name(&mut self, model: ModelId) {
        self.status.model_name = self.names.get(&model.0).cloned();
        self.status.short_name = self.status.model_name.as_deref().and_then(short_name);
        self.status.model = Some(model);
    }

    fn set(&mut self, mode: PermissionMode, effort: Option<Effort>) {
        self.status.mode = Some(mode);
        self.status.next_mode = Some(next_mode(mode));
        self.status.effort = effort.or(self.tier_effort);
    }
}

#[cfg(test)]
mod tests {
    use cox_protocol::ids::TurnId;

    use super::*;

    fn opened() -> StatusFold {
        let mut config = Config::default();
        config.tiers.code.model = "claude-sonnet-5".into();
        config.tiers.code.effort = Effort::High;
        config.permissions.mode = PermissionMode::Plan;
        StatusFold::open(&config)
    }

    fn turn(job: Job, model: &str) -> Event {
        Event::TurnStarted {
            turn: TurnId::new(),
            seq: 1,
            job,
            tier: Tier::Code,
            model: ModelId(model.into()),
        }
    }

    #[test]
    fn a_session_opens_with_the_configured_mode_model_and_effort() {
        let status = opened().status().clone();
        assert_eq!(status.mode, Some(PermissionMode::Plan));
        assert_eq!(status.next_mode, Some(PermissionMode::Auto));
        assert_eq!(status.model, Some(ModelId("claude-sonnet-5".into())));
        assert_eq!(status.effort, Some(Effort::High));
    }

    #[test]
    fn state_changed_sets_the_mode_and_a_cleared_effort_falls_back_to_the_tier() {
        let mut fold = opened();
        let changed = |mode, effort| Event::StateChanged { mode, effort };
        assert!(fold.apply(&changed(PermissionMode::Auto, Some(Effort::Low))));
        assert_eq!(fold.status().mode, Some(PermissionMode::Auto));
        assert_eq!(fold.status().next_mode, Some(PermissionMode::Default));
        assert_eq!(fold.status().effort, Some(Effort::Low));
        assert!(fold.apply(&changed(PermissionMode::Auto, None)));
        assert_eq!(fold.status().effort, Some(Effort::High));
        assert!(
            !fold.apply(&changed(PermissionMode::Auto, None)),
            "no change"
        );
    }

    #[test]
    fn only_a_main_turn_or_a_code_tier_switch_names_the_model() {
        let mut fold = opened();
        assert!(!fold.apply(&turn(Job::Compact, "claude-haiku-5")));
        assert!(fold.apply(&turn(Job::Main, "claude-opus-5")));
        assert_eq!(fold.status().model, Some(ModelId("claude-opus-5".into())));
        let switch = |tier| Event::ModelSwitched {
            tier,
            from: ModelId("claude-opus-5".into()),
            to: ModelId("gpt-6".into()),
        };
        assert!(!fold.apply(&switch(Tier::Cheap)));
        assert!(fold.apply(&switch(Tier::Code)));
        assert_eq!(fold.status().model, Some(ModelId("gpt-6".into())));
    }

    #[test]
    fn the_status_names_the_model_from_the_catalog_and_an_unknown_one_by_nothing() {
        let mut fold = opened();
        assert_eq!(fold.status().model_name.as_deref(), Some("Claude Sonnet 5"));
        assert!(fold.apply(&turn(Job::Main, "claude-opus-5")));
        assert_eq!(fold.status().model_name.as_deref(), Some("Claude Opus 5"));
        assert!(fold.apply(&turn(Job::Main, "my-local-model")));
        assert_eq!(fold.status().model_name, None);
    }

    #[test]
    fn a_short_name_drops_the_vendor_and_latest() {
        let cases = [
            ("Claude Sonnet 5", Some("Sonnet 5")),
            ("GPT-5.1", Some("GPT-5.1")),
            ("DeepSeek V4 Pro", Some("DeepSeek V4 Pro")),
            ("Claude ", Some("Claude ")),
            ("Claude Haiku 4.5 (latest)", Some("Haiku 4.5")),
            ("GPT (latest) Mini", Some("GPT (latest) Mini")),
            (" (latest)", Some(" (latest)")),
            ("", None),
        ];
        for (name, short) in cases {
            assert_eq!(short_name(name).as_deref(), short, "{name:?}");
        }
    }

    #[test]
    fn the_status_keeps_the_full_name_beside_the_short_one() {
        let mut fold = opened();
        let status = fold.status();
        assert_eq!(status.model_name.as_deref(), Some("Claude Sonnet 5"));
        assert_eq!(status.short_name.as_deref(), Some("Sonnet 5"));
        assert!(fold.apply(&turn(Job::Main, "my-local-model")));
        assert_eq!(
            fold.status().short_name,
            None,
            "no name, the client shows the id"
        );
    }
}
