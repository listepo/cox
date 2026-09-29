//! The sidebar's sections (DT§5.1, DS§6.4 `Sidebar`, T58.4.4): "Needs
//! you" from the inbox, "Running" lifted out of the projects, then each
//! project with its other sessions, each row with its status, words and
//! cost. Here, not in a client, because these decide what the list shows
//! and both desktop clients would otherwise decide it twice; a client only
//! joins a row's subtitle parts and localizes its `age` part. Separate from
//! the workspace queries, which it reads, because this is the rule over
//! them, not a read of `cox.db`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::app::{App, AppError};
use crate::inbox::{Activity, InboxItem, InboxStatus};
use crate::workspace::{Project, SessionEntry};

/// The projects the list reads, and the sessions it reads of each.
const PROJECT_LIMIT: i64 = 20;
const SESSION_LIMIT: i64 = 20;

/// A row's status glyph, named as the clients' status dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowStatus {
    Running,
    Waiting,
    Idle,
    Error,
}

/// One part of a row's subtitle; a client joins them with ` · `.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SubtitlePart {
    Text {
        text: String,
    },
    /// How long ago the session last wrote, which each client words in its
    /// own locale (`2h ago`); RFC 3339. The filter never matches it (A129).
    Age {
        updated_at: String,
    },
}

/// One row: an inbox item's or a session's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidebarRow {
    pub id: String,
    /// The session a click opens.
    pub session: String,
    pub status: RowStatus,
    pub title: String,
    pub subtitle: Vec<SubtitlePart>,
    /// `$0.42`; `None` before it cost anything.
    pub cost: Option<String>,
    /// An expired inbox item: shown, not answerable from here.
    pub read_only: bool,
}

/// A status section or a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SectionKind {
    /// "Needs you" with its count, or "Running" without one.
    Section {
        count: Option<u32>,
    },
    Project {
        expanded: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidebarSection {
    pub id: String,
    pub title: String,
    pub kind: SectionKind,
    pub rows: Vec<SidebarRow>,
}

/// A project with its sessions and what each is doing here.
pub type ProjectSessions = (Project, Vec<(SessionEntry, Activity)>);

/// "Needs you" and "Running" while they hold a row, then every project that
/// does; with a filter, a project with no match is left out and a folded
/// one opens to show what matched. `folded` holds project roots.
pub fn sections(
    inbox: &[InboxItem],
    projects: &[ProjectSessions],
    filter: &str,
    folded: &[String],
) -> Vec<SidebarSection> {
    let query = filter.to_lowercase();
    let matches = |row: &SidebarRow| query.is_empty() || matches(row, &query);
    let needs: Vec<SidebarRow> = inbox.iter().map(inbox_row).filter(matches).collect();
    let mut running = Vec::new();
    let mut groups = Vec::new();
    for (project, sessions) in projects {
        let mut rows = Vec::new();
        for (entry, activity) in sessions {
            let row = session_row(entry, *activity, project);
            if !matches(&row) {
                continue;
            }
            if row.status == RowStatus::Running {
                running.push(row);
            } else {
                rows.push(row);
            }
        }
        if rows.is_empty() && !query.is_empty() {
            continue;
        }
        let is_folded = folded.iter().any(|f| Path::new(f) == project.root);
        groups.push(SidebarSection {
            id: project.root.display().to_string(),
            title: project.name.clone(),
            kind: SectionKind::Project {
                expanded: !is_folded || !query.is_empty(),
            },
            rows,
        });
    }
    let mut out = Vec::new();
    if !needs.is_empty() {
        out.push(SidebarSection {
            id: "needs-you".into(),
            title: "Needs you".into(),
            kind: SectionKind::Section {
                count: Some(u32::try_from(needs.len()).unwrap_or(u32::MAX)),
            },
            rows: needs,
        });
    }
    if !running.is_empty() {
        out.push(SidebarSection {
            id: "running".into(),
            title: "Running".into(),
            kind: SectionKind::Section { count: None },
            rows: running,
        });
    }
    out.extend(groups);
    out
}

/// The title or a text part holds `query` (already lowercased), ignoring
/// case; the `age` part, which each client words differently, never
/// counts. No diacritic folding: no crate in the workspace provides it
/// (A129).
fn matches(row: &SidebarRow, query: &str) -> bool {
    let holds = |text: &str| text.to_lowercase().contains(query);
    holds(&row.title)
        || row.subtitle.iter().any(|part| match part {
            SubtitlePart::Text { text } => holds(text),
            SubtitlePart::Age { .. } => false,
        })
}

fn text(text: &str) -> SubtitlePart {
    SubtitlePart::Text { text: text.into() }
}

fn inbox_row(item: &InboxItem) -> SidebarRow {
    SidebarRow {
        id: format!("{}#{}", item.session, item.seq),
        session: item.session.to_string(),
        status: match item.status {
            InboxStatus::Waiting => RowStatus::Waiting,
            InboxStatus::Idle => RowStatus::Idle,
            InboxStatus::Error => RowStatus::Error,
        },
        title: item.title.clone(),
        subtitle: vec![text(&item.subtitle)],
        cost: None,
        read_only: item.expired,
    }
}

/// A session's row: a running or waiting one says where it runs, any other
/// how long ago it wrote; `done` only once a turn finished. An external
/// agent's session says whose it is first (mockup 27).
fn session_row(entry: &SessionEntry, activity: Activity, project: &Project) -> SidebarRow {
    let age = || SubtitlePart::Age {
        updated_at: entry.info.updated_at.clone(),
    };
    let (status, words) = match activity {
        Activity::Running => (
            RowStatus::Running,
            vec![text(&project.name), text("running")],
        ),
        Activity::WaitingOnYou => (
            RowStatus::Waiting,
            vec![text(&project.name), text("waiting for you")],
        ),
        Activity::Failed => (RowStatus::Error, vec![age(), text("failed")]),
        Activity::Idle if entry.info.turns > 0 => (RowStatus::Idle, vec![age(), text("done")]),
        Activity::Idle => (RowStatus::Idle, vec![age()]),
    };
    SidebarRow {
        id: entry.info.id.clone(),
        session: entry.info.id.clone(),
        status,
        title: entry.name.clone(),
        subtitle: entry.agent.iter().map(|a| text(a)).chain(words).collect(),
        cost: (entry.info.cost_usd > 0.0).then(|| format!("${:.2}", entry.info.cost_usd)),
        read_only: false,
    }
}

impl App {
    /// The sidebar's sections now (T58.4.4): the inbox, the most recent
    /// projects and their sessions from `cox.db`, each session's activity
    /// here; see [`sections`].
    pub fn sidebar(
        &self,
        filter: &str,
        folded: &[String],
    ) -> Result<Vec<SidebarSection>, AppError> {
        let workspace = self.workspace();
        let mut projects = Vec::new();
        for project in workspace.projects(PROJECT_LIMIT)? {
            let sessions = workspace
                .sessions(&project.root, SESSION_LIMIT)?
                .into_iter()
                .map(|entry| {
                    let activity = entry
                        .info
                        .id
                        .parse()
                        .map_or(Activity::Idle, |id| self.activity(id));
                    (entry, activity)
                })
                .collect();
            projects.push((project, sessions));
        }
        Ok(sections(&self.inbox(), &projects, filter, folded))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cox_protocol::SessionId;
    use cox_store::fts::SessionInfo;

    use super::*;
    use crate::inbox::Need;

    fn project(root: &str) -> Project {
        Project {
            root: PathBuf::from(root),
            name: root.rsplit('/').next().unwrap_or(root).into(),
            sessions: 0,
            cost_usd: 0.0,
            updated_at: String::new(),
        }
    }

    fn entry(id: &str, title: Option<&str>, turns: i64, cost_usd: f64) -> SessionEntry {
        let title = title.map(String::from);
        SessionEntry {
            name: SessionEntry::name_of(title.as_deref()),
            info: SessionInfo {
                id: id.into(),
                title,
                cwd: String::new(),
                created_at: String::new(),
                updated_at: "2026-09-29T10:00:00Z".into(),
                turns,
                cost_usd,
            },
            held_by: None,
            agent: None,
            best_of: None,
        }
    }

    fn texts(row: &SidebarRow) -> Vec<&str> {
        row.subtitle
            .iter()
            .map(|p| match p {
                SubtitlePart::Text { text } => text.as_str(),
                SubtitlePart::Age { .. } => "<age>",
            })
            .collect()
    }

    fn titles(sections: &[SidebarSection]) -> Vec<&str> {
        sections.iter().map(|s| s.title.as_str()).collect()
    }

    fn failed(title: &str) -> InboxItem {
        let mut inbox = crate::inbox::Inbox::default();
        let session = SessionId::new();
        inbox.apply(
            session,
            &cox_protocol::types::Event::TurnDone {
                turn: cox_protocol::ids::TurnId::new(),
                stop: cox_protocol::types::StopReason::Refusal {
                    detail: title.into(),
                },
            },
        );
        let item = inbox.items()[0].clone();
        assert!(matches!(item.need, Need::Failed { .. }));
        item
    }

    #[test]
    fn running_sessions_leave_their_project() {
        let cox = project("/src/cox");
        let web = project("/src/web");
        let projects = vec![
            (
                cox.clone(),
                vec![
                    (entry("a", Some("Add jitter"), 1, 0.42), Activity::Running),
                    (entry("b", Some("Fix flake"), 2, 0.0), Activity::Idle),
                ],
            ),
            (
                web,
                vec![(entry("c", Some("Sitemap"), 0, 0.0), Activity::Running)],
            ),
        ];
        let inbox = [failed("Rate limited")];
        let shown = sections(&inbox, &projects, "", &[]);
        assert_eq!(titles(&shown), ["Needs you", "Running", "cox", "web"]);
        assert_eq!(shown[0].kind, SectionKind::Section { count: Some(1) });
        assert_eq!(shown[0].rows[0].title, "Rate limited");
        assert_eq!(texts(&shown[0].rows[0]), ["turn failed"]);
        assert_eq!(shown[1].kind, SectionKind::Section { count: None });
        let running: Vec<_> = shown[1].rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(running, ["a", "c"]);
        assert_eq!(texts(&shown[1].rows[0]), ["cox", "running"]);
        assert_eq!(shown[1].rows[0].cost.as_deref(), Some("$0.42"));
        assert_eq!(
            shown[1].rows[1].cost, None,
            "no cost before it cost anything"
        );
        let cox_rows: Vec<_> = shown[2].rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(cox_rows, ["b"]);
        assert!(shown[3].rows.is_empty(), "a project stays while unfiltered");
    }

    #[test]
    fn a_filter_opens_a_folded_project() {
        let projects = vec![
            (
                project("/src/cox"),
                vec![
                    (entry("a", Some("Add retry JITTER"), 1, 0.0), Activity::Idle),
                    (entry("b", Some("Fix flake"), 1, 0.0), Activity::Idle),
                ],
            ),
            (
                project("/src/web"),
                vec![(entry("c", Some("Sitemap"), 1, 0.0), Activity::Idle)],
            ),
        ];
        let folded = ["/src/cox".to_owned()];
        let unfiltered = sections(&[], &projects, "", &folded);
        assert_eq!(unfiltered[0].kind, SectionKind::Project { expanded: false });
        let shown = sections(&[], &projects, "jitter", &folded);
        assert_eq!(
            titles(&shown),
            ["cox"],
            "a project with no match is left out"
        );
        assert_eq!(shown[0].kind, SectionKind::Project { expanded: true });
        assert_eq!(shown[0].rows.len(), 1);
        // A text part matches, the localized age never does.
        assert_eq!(
            titles(&sections(&[], &projects, "DONE", &[])),
            ["cox", "web"]
        );
        assert!(sections(&[], &projects, "2026", &[]).is_empty());
    }

    #[test]
    fn an_idle_session_says_done_only_after_a_turn() {
        let cox = project("/src/cox");
        let row = |turns, activity| session_row(&entry("a", None, turns, 0.0), activity, &cox);
        assert_eq!(texts(&row(0, Activity::Idle)), ["<age>"]);
        assert_eq!(texts(&row(3, Activity::Idle)), ["<age>", "done"]);
        assert_eq!(row(3, Activity::Idle).status, RowStatus::Idle);
        let failed = row(3, Activity::Failed);
        assert_eq!(
            (texts(&failed), failed.status),
            (vec!["<age>", "failed"], RowStatus::Error)
        );
        let waiting = row(3, Activity::WaitingOnYou);
        assert_eq!(texts(&waiting), ["cox", "waiting for you"]);
        let mut theirs = entry("a", None, 1, 0.0);
        theirs.agent = Some("claude".into());
        let row = session_row(&theirs, Activity::Idle, &cox);
        assert_eq!(texts(&row), ["claude", "<age>", "done"], "the agent first");
    }

    #[test]
    fn an_untitled_session_is_named_untitled() {
        let row = session_row(&entry("a", None, 0, 0.0), Activity::Idle, &project("/p"));
        assert_eq!(row.title, "Untitled session");
        let titled = session_row(
            &entry("a", Some("Add jitter"), 0, 0.0),
            Activity::Idle,
            &project("/p"),
        );
        assert_eq!(titled.title, "Add jitter");
    }
}
