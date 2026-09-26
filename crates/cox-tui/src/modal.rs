//! Approval modal (T5.4): what `ApprovalRequired` shows and the keys that
//! decide it — `y` allow, `s` allow for the session, `n` deny, `e` edit a
//! bash command inline and resubmit it as `Decision::Edit`. Separate from
//! `state` so the key table and the drawing sit together and one snapshot
//! covers both. The `/context` modal (T25.7) lives here for the same reason.
//! An `edit` call's proposed change prints through `diff::lines` (T24.5),
//! the renderer the edit card and `Ctrl+G` use. The `?` keymap overlay
//! (T24.6) draws here too, from the live `keymap::Keymap` (T25.5).

use cox_protocol::GrantScope;
use cox_protocol::ids::CallId;
use cox_protocol::types::{Decision, Diff, ToolCall, Why};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::cells::Look;
use crate::commands::{Context, label};
use crate::diff;
use crate::glyph::Glyphs;
use crate::keymap::Keymap;
use crate::text::sanitize;
use crate::theme::Theme;

/// The bash tool's input field the `e` key rewrites.
const COMMAND_FIELD: &str = "command";

/// Diff rows the approval modal shows before folding the rest into one
/// line, so a large edit never pushes the composer off screen.
const DIFF_ROWS: usize = 12;

#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub call: ToolCall,
    pub why: Why,
    /// `e`: the command as edited so far; the cursor sits at its end.
    pub editing: Option<String>,
    /// The subagent asking (T27.2); `None` for the session itself.
    pub agent: Option<String>,
}

impl Approval {
    pub fn new(call: ToolCall, why: Why) -> Self {
        Self {
            call,
            why,
            editing: None,
            agent: None,
        }
    }

    /// Labels the prompt with the subagent it came from.
    pub fn from_agent(mut self, agent: Option<String>) -> Self {
        self.agent = agent;
        self
    }

    fn editable(&self) -> bool {
        self.call.name == "bash"
    }

    /// `Some` once a key decided the call; `None` keeps the modal open.
    pub fn key(&mut self, key: KeyEvent) -> Option<Decision> {
        if let Some(text) = &mut self.editing {
            match key.code {
                KeyCode::Enter => {
                    let edited = std::mem::take(text);
                    self.editing = None;
                    if self.command() == edited {
                        return Some(Decision::Allow);
                    }
                    let mut input = self.call.input.clone();
                    if let Some(obj) = input.as_object_mut() {
                        obj.insert(COMMAND_FIELD.into(), edited.into());
                    }
                    return Some(Decision::Edit { input });
                }
                KeyCode::Esc => self.editing = None,
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => text.push(c),
                _ => {}
            }
            return None;
        }
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => Some(Decision::Allow),
            KeyCode::Char('s') => Some(Decision::AllowForSession),
            KeyCode::Char('n') | KeyCode::Esc => Some(Decision::Deny {
                reason: "denied by user".into(),
            }),
            KeyCode::Char('e') if self.editable() => {
                self.editing = Some(self.command());
                None
            }
            _ => None,
        }
    }

    /// The command as the model wrote it; the subject is the fallback for a
    /// call whose input does not carry one.
    fn command(&self) -> String {
        self.call.input[COMMAND_FIELD]
            .as_str()
            .map_or_else(|| self.call.subject.clone(), str::to_string)
    }

    /// An `edit` call's `old` → `new` as a unified diff, so the modal shows
    /// what `y` would write. Line numbers count from the snippet, not the
    /// file: the TUI never reads the disk. `None` for any other tool.
    fn proposed(&self) -> Option<Diff> {
        if self.call.name != "edit" {
            return None;
        }
        let field = |k: &str| self.call.input[k].as_str().map(sanitize);
        let (path, old, new) = (field("path")?, field("old")?, field("new")?);
        // A trailing newline on both sides keeps `\ No newline` markers out
        // of a snippet that simply ends mid-file.
        let eol = |s: String| if s.ends_with('\n') { s } else { s + "\n" };
        let (old, new) = (eol(old), eol(new));
        let text = similar::TextDiff::from_lines(&old, &new);
        let mut unified = text.unified_diff();
        unified.context_radius(3).header(&path, &path);
        Some(Diff {
            path: path.clone().into(),
            unified: unified.to_string(),
        })
    }

    /// The prompt, the proposed diff when there is one (at most
    /// `DIFF_ROWS`), the reason and the keys.
    pub fn lines(&self, look: &Look) -> Vec<Line<'static>> {
        let (g, theme) = (&look.glyphs, &look.colors);
        let why = match &self.why {
            Why::RuleAsk { rule } => format!("rule {rule} asks"),
            Why::Risk { risk } => format!("{risk:?} risk needs approval").to_lowercase(),
            Why::SandboxDenied { detail } => format!("sandbox denied: {}", sanitize(detail)),
            Why::Policy { policy } => format!("approval policy {policy:?}").to_lowercase(),
        };
        let keys = match &self.editing {
            Some(text) => format!(
                " edit> {text}{}   Enter runs {} Esc cancels",
                g.caret, g.sep
            ),
            None if self.editable() => " [y]es  [s]ession  [n]o  [e]dit".to_string(),
            None => " [y]es  [s]ession  [n]o".to_string(),
        };
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let mut header = Line::styled(
            format!(
                " approve {} {}?",
                sanitize(&self.call.name),
                sanitize(&self.call.subject)
            ),
            bold.fg(theme.warn),
        );
        if let Some(agent) = &self.agent {
            let asks = format!(" {} asks:", sanitize(agent));
            header
                .spans
                .insert(0, Span::styled(asks, bold.fg(theme.agent)));
        }
        let mut out = vec![header];
        if let Some(d) = self.proposed() {
            let look = Look {
                show_diffs: true,
                ..*look
            };
            let rows = diff::lines(&d, &look);
            let hidden = rows.len().saturating_sub(DIFF_ROWS);
            out.extend(rows.into_iter().take(DIFF_ROWS));
            if hidden > 0 {
                out.push(Line::styled(
                    format!("  {} {hidden} more lines", g.ellipsis),
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }
        }
        out.push(Line::styled(
            format!(" {why}"),
            Style::default().add_modifier(Modifier::DIM),
        ));
        out.push(Line::raw(keys));
        out
    }
}

/// `Modal::PluginGrant` (T33.8, PL§3): one `NeedsApproval` plugin from
/// session open. More than one queues in `state.rs`'s `pending_grants`,
/// since the TUI has one modal slot. `y` grants the full requested
/// capability list at this digest; `n` skips it for this session only —
/// there is no "always" key, since a grant is always decided per digest.
/// `crates/cox` builds one of these per plugin (`session::plugin_grant_requests`)
/// and never this crate: no store read, no filesystem, matches every other
/// modal here.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginGrantDialog {
    pub plugin_id: String,
    /// The grant key (`crates/cox`'s `discover::Plugin::grant_digest()`):
    /// the real package digest, or — for a linked plugin (`dev` below,
    /// T33.41) — the fixed `link_digest()`. `y` writes the grant under
    /// this value, so it must be the same one `grant_digest()` looks up
    /// under next time, not the plugin's live, rebuild-volatile digest.
    pub digest: String,
    pub scope: GrantScope,
    /// The full requested capability list (PL§3's unit of approval) — what
    /// `y` grants, not just `added`.
    pub capabilities: Vec<String>,
    name: String,
    description: String,
    /// Requested and not granted, sorted (mirrors `grant::Verdict`'s field
    /// of the same name; for a brand-new plugin this is the whole request).
    added: Vec<String>,
    /// Granted and no longer requested, sorted.
    removed: Vec<String>,
    /// A project plugin's repository root (PL§3: shown in warning style);
    /// `None` for a user plugin.
    repo: Option<String>,
    /// True for a `cox plugin link`ed plugin (T33.41, PL§13's dev loop):
    /// shown so an approval at session open still says why this one asks
    /// again on a widened capability list rather than on any byte change.
    dev: bool,
}

impl PluginGrantDialog {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plugin_id: String,
        digest: String,
        scope: GrantScope,
        name: String,
        description: String,
        capabilities: Vec<String>,
        added: Vec<String>,
        removed: Vec<String>,
        repo: Option<String>,
        dev: bool,
    ) -> Self {
        Self {
            plugin_id,
            digest,
            scope,
            capabilities,
            name,
            description,
            added,
            removed,
            repo,
            dev,
        }
    }

    /// `Some(true)`: grant. `Some(false)`: skip for this session. `None`
    /// keeps the modal open — only `y`/`n` decide it (PL§3: no "always").
    pub fn key(&self, key: KeyEvent) -> Option<bool> {
        match key.code {
            KeyCode::Char('y') => Some(true),
            KeyCode::Char('n') | KeyCode::Esc => Some(false),
            _ => None,
        }
    }

    /// The prompt, the plugin's description, the repository line for a
    /// project plugin, the capability diff and the keys. Every manifest
    /// string (`name`, `description`, `repo`, each capability) passes
    /// `sanitize`: a plugin's own `plugin.toml` is untrusted repository or
    /// download content, the same boundary `Approval`'s `why` crosses.
    pub fn lines(&self, g: &Glyphs, theme: &Theme) -> Vec<Line<'static>> {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let digest12 = &self.digest[..self.digest.len().min(12)];
        let dev_tag = if self.dev { " (dev)" } else { "" };
        let mut out = vec![Line::styled(
            format!(
                " plugin {} ({}) wants to load{dev_tag} {} digest {}",
                sanitize(&self.name),
                sanitize(&self.plugin_id),
                g.sep,
                sanitize(digest12),
            ),
            bold.fg(theme.warn),
        )];
        if self.dev {
            out.push(Line::styled(
                " linked plugin — asks again only if capabilities widen, not on a rebuild"
                    .to_string(),
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        if !self.description.is_empty() {
            out.push(Line::raw(format!(" {}", sanitize(&self.description))));
        }
        if let Some(repo) = &self.repo {
            out.push(Line::styled(
                format!(" project plugin {} repository {}", g.sep, sanitize(repo)),
                Style::default().fg(theme.warn),
            ));
        }
        let joined = |caps: &[String]| {
            caps.iter()
                .map(|c| sanitize(c))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !self.added.is_empty() {
            out.push(Line::raw(format!(" wants: {}", joined(&self.added))));
        }
        if !self.removed.is_empty() {
            out.push(Line::styled(
                format!(" no longer asks for: {}", joined(&self.removed)),
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        out.push(Line::raw(" [y]es  [n]o"));
        out
    }
}

/// The `?` overlay (T24.6): the keymap as bound now (T25.5) by context, a
/// bold header per context and its rows packed into as few `width`-column
/// lines as fit, so the whole table stays inside the inline viewport.
pub fn help_lines(g: &Glyphs, theme: &Theme, keymap: &Keymap, width: u16) -> Vec<Line<'static>> {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let mut out = vec![Line::styled(
        format!(" keys {} Esc or ? closes", g.sep),
        bold,
    )];
    for ctx in Context::ALL {
        out.push(Line::styled(format!(" {}", ctx.name()), bold));
        let (mut spans, mut used): (Vec<Span<'static>>, usize) = (Vec::new(), 0);
        let dim = Style::default().fg(theme.dim);
        for (key, action) in keymap.rows(ctx) {
            let label = label(action);
            let cell = key.len() + 1 + label.len();
            if used > 0 && used + 3 + cell > usize::from(width) {
                out.push(Line::from(std::mem::take(&mut spans)));
                used = 0;
            }
            if used == 0 {
                spans.push(Span::raw("   "));
            } else {
                spans.push(Span::styled(format!(" {} ", g.sep), dim));
            }
            spans.push(Span::styled(
                key.to_string(),
                Style::default().fg(theme.accent),
            ));
            spans.push(Span::styled(format!(" {label}"), dim));
            used += 3 + cell;
        }
        out.push(Line::from(spans));
    }
    out
}

/// `Modal::Plugin`'s overlay before its first render lands, or once the
/// slot has stopped (T33.24, PL§8): fail open like a missed status
/// segment — a line, never a crash or a stuck blank screen.
pub fn plugin_overlay_placeholder(id: &str, theme: &Theme) -> Line<'static> {
    Line::styled(
        format!(" {} …", sanitize(id)),
        Style::default().fg(theme.dim),
    )
}

/// `ask_user`'s answer, once a key decides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionAnswer {
    /// A `1`-`9` option pick, or whatever `Enter` sent from the free-text row.
    Text(String),
    /// `Esc`: the reply channel is dropped rather than sent, so the tool
    /// call fails ("dismissed without an answer") instead of succeeding
    /// with an empty answer.
    Dismissed,
}

/// `ask_user` modal (T22.1): the model's question, its suggested options
/// picked by digit, and a free-text row `Enter` sends. Shares `Approval`'s
/// shape — a `key`/`height`/`lines` triple — so `state.rs` and `view.rs`
/// drive both the same way.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub call: CallId,
    question: String,
    options: Vec<String>,
    /// What the user has typed so far; sent verbatim on `Enter`.
    input: String,
    /// The subagent asking (T34.3), same shape as `Approval::agent`;
    /// `None` for the session itself.
    agent: Option<String>,
}

impl Question {
    pub fn new(call: CallId, question: String, options: Vec<String>) -> Self {
        Self {
            call,
            question,
            options,
            input: String::new(),
            agent: None,
        }
    }

    /// Labels the prompt with the subagent it came from, mirroring
    /// `Approval::from_agent`.
    pub fn from_agent(mut self, agent: Option<String>) -> Self {
        self.agent = agent;
        self
    }

    /// `Some` once a key decided the answer; `None` keeps the modal open.
    pub fn key(&mut self, key: KeyEvent) -> Option<QuestionAnswer> {
        match key.code {
            KeyCode::Esc => Some(QuestionAnswer::Dismissed),
            KeyCode::Enter => Some(QuestionAnswer::Text(std::mem::take(&mut self.input))),
            KeyCode::Backspace => {
                self.input.pop();
                None
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // A bare digit picks an option, but only as the first
                // keystroke — once the free-text row holds anything, a
                // digit joins it like any other character.
                if self.input.is_empty()
                    && let Some(n) = c.to_digit(10).filter(|n| (1..=9).contains(n))
                    && let Some(opt) = self.options.get(n as usize - 1)
                {
                    return Some(QuestionAnswer::Text(opt.clone()));
                }
                self.input.push(c);
                None
            }
            _ => None,
        }
    }

    pub fn height(&self) -> u16 {
        3
    }

    pub fn lines(&self, g: &Glyphs, theme: &Theme) -> Vec<Line<'static>> {
        let options = if self.options.is_empty() {
            " type an answer".to_string()
        } else {
            self.options
                .iter()
                .take(9)
                .enumerate()
                .map(|(i, o)| format!(" {}) {}", i + 1, sanitize(o)))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let mut header = Line::styled(
            format!(" ask_user {}", sanitize(&self.question)),
            bold.fg(theme.warn),
        );
        if let Some(agent) = &self.agent {
            let asks = format!(" {} asks:", sanitize(agent));
            header
                .spans
                .insert(0, Span::styled(asks, bold.fg(theme.agent)));
        }
        vec![
            header,
            Line::styled(options, Style::default().add_modifier(Modifier::DIM)),
            Line::raw(format!(
                " > {}{}   Enter sends {} Esc dismisses",
                self.input, g.caret, g.sep
            )),
        ]
    }
}

/// `/context` (T25.7): where the next request's tokens go — one bar per
/// §1.9 segment scaled to `max_context`, numbers right-aligned, and the
/// compaction threshold as a marker. Same `height`/`lines` shape as the
/// sibling modals; bars are ASCII so both glyph sets hold (T14.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ContextBars {
    /// `(label, estimated tokens)` per §1.9 segment, display order — the
    /// numbers arrive as rows because `cox-core`'s `Breakdown` is not
    /// exported across the crate boundary and this modal only displays.
    pub segments: Vec<(&'static str, u32)>,
    pub total: u32,
    pub cached: u32,
    pub max_context: u32,
    pub compact_at: f64,
}

/// Bar geometry, fixed so the rows and the threshold marker align; the
/// label column is 17 so the widest label ("history verbatim") keeps a gap.
const LABEL: usize = 17;
const BAR: usize = 24;

impl ContextBars {
    pub fn height(&self) -> u16 {
        // header + one row per segment + total + cached + threshold caption.
        u16::try_from(self.segments.len() + 4).unwrap_or(u16::MAX)
    }

    pub fn lines(&self, g: &Glyphs, theme: &Theme) -> Vec<Line<'static>> {
        let marker = ((self.compact_at.clamp(0.0, 1.0) * BAR as f64).round() as usize).min(BAR - 1);
        let bar = |tokens: u32| {
            let filled = u64::from(tokens) * BAR as u64 / u64::from(self.max_context.max(1));
            (0..BAR)
                .map(|i| match (i == marker, (i as u64) < filled) {
                    (true, _) => '|',
                    (false, true) => '#',
                    (false, false) => ' ',
                })
                .collect::<String>()
        };
        let row = |label: &str, tokens: u32, style: Style| {
            Line::from(vec![
                Span::styled(
                    format!(" {label:<w$}", w = LABEL),
                    Style::default().fg(theme.dim),
                ),
                Span::styled(bar(tokens), Style::default().fg(theme.accent)),
                Span::styled(format!("  {tokens:>8}"), style),
            ])
        };
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let mut lines = vec![Line::styled(
            format!(
                " context {} {} / {} tokens {} cached {}",
                g.sep, self.total, self.max_context, g.sep, self.cached
            ),
            bold,
        )];
        lines.extend(
            self.segments
                .iter()
                .map(|(label, tokens)| row(label, *tokens, Style::default())),
        );
        lines.push(row("total", self.total, bold));
        lines.push(row(
            "cached",
            self.cached,
            Style::default().add_modifier(Modifier::DIM),
        ));
        let threshold = (self.compact_at.clamp(0.0, 1.0) * f64::from(self.max_context)) as u32;
        lines.push(Line::styled(
            format!(
                " {}^ compact_at = {} = {} tokens",
                " ".repeat(LABEL + marker),
                self.compact_at,
                threshold
            ),
            Style::default().add_modifier(Modifier::DIM),
        ));
        lines
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::{Paragraph, Widget};

    use super::*;

    /// T25.7 `/context`: bars scaled to `max_context`, right-aligned
    /// numbers, the compaction threshold marked.
    #[test]
    fn context_modal_snapshot() {
        let bars = ContextBars {
            segments: vec![
                ("tools", 21_500),
                ("system", 3_200),
                ("instructions", 120),
                ("skills", 0),
                ("memory", 0),
                ("volatile", 240),
                ("history verbatim", 41_000),
                ("history pointers", 1_250),
                ("summary", 900),
            ],
            total: 68_210,
            cached: 51_000,
            max_context: 200_000,
            compact_at: 0.75,
        };
        let mut term = Terminal::new(TestBackend::new(72, bars.height())).expect("test terminal");
        term.draw(|f| {
            Paragraph::new(bars.lines(&Glyphs::default(), &Theme::dark()))
                .render(f.area(), f.buffer_mut());
        })
        .expect("draw");
        insta::assert_snapshot!(crate::view::buffer_to_string(term.backend().buffer()));
    }

    fn plugin_grant(added: &[&str], removed: &[&str], repo: Option<&str>) -> PluginGrantDialog {
        let added: Vec<String> = added.iter().map(|s| s.to_string()).collect();
        let removed: Vec<String> = removed.iter().map(|s| s.to_string()).collect();
        PluginGrantDialog::new(
            "git-glance".into(),
            "a".repeat(64),
            repo.map_or(GrantScope::User, |r| {
                GrantScope::Project(std::path::PathBuf::from(r))
            }),
            "Git Glance".into(),
            "Shows a one-line git summary in the status bar".into(),
            added.clone(),
            added,
            removed,
            repo.map(String::from),
            false,
        )
    }

    fn render_grant(dialog: &PluginGrantDialog) -> String {
        let lines = dialog.lines(&Glyphs::default(), &Theme::dark());
        let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
        let mut term = Terminal::new(TestBackend::new(72, height)).expect("test terminal");
        term.draw(|f| Paragraph::new(lines).render(f.area(), f.buffer_mut()))
            .expect("draw");
        crate::view::buffer_to_string(term.backend().buffer())
    }

    /// T33.8, PL§3: a brand-new plugin's `added` is its whole request
    /// (`grant::check` with no stored grant), no `removed`, no repository.
    #[test]
    fn new_plugin_grant_snapshot() {
        insta::assert_snapshot!(render_grant(&plugin_grant(
            &["kv", "net:api.github.com"],
            &[],
            None
        )));
    }

    /// A plugin whose package widened: `added`/`removed` show the diff
    /// (`grant::Verdict::NeedsApproval`).
    #[test]
    fn widened_plugin_grant_shows_the_diff() {
        insta::assert_snapshot!(render_grant(&plugin_grant(
            &["model:code"],
            &["model:cheap"],
            None
        )));
    }

    /// PL§3: a project plugin's repository shows in warning style.
    #[test]
    fn project_plugin_grant_shows_its_repository() {
        insta::assert_snapshot!(render_grant(&plugin_grant(
            &["fs.write:$WORKSPACE"],
            &[],
            Some("/home/user/repo")
        )));
    }

    /// A plugin's `plugin.toml` is untrusted content (repository or
    /// download): `description` must never carry an escape sequence or a
    /// bidi override into the terminal.
    #[test]
    fn grant_dialog_sanitizes_description() {
        let mut dialog = plugin_grant(&["kv"], &[], None);
        dialog.description = "\u{1b}[31mred\u{1b}[0m \u{202e}evil\u{202c}".into();
        let text = render_grant(&dialog);
        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert!(!text.contains('\u{202e}'), "{text:?}");
        assert!(text.contains("red evil"), "{text:?}");
    }

    /// T33.41 Check `linked_plugin_marked_dev_everywhere`: the TUI grant
    /// dialog is one of the surfaces that must mark a `cox plugin link`ed
    /// plugin as `dev` (alongside `cox plugin list` and `cox doctor`).
    #[test]
    fn plugin_grant_dialog_marks_a_linked_plugin_dev() {
        let mut dialog = plugin_grant(&["model:code"], &["model:cheap"], None);
        dialog.dev = true;
        let text = render_grant(&dialog);
        assert!(text.contains("(dev)"), "{text:?}");
        assert!(
            text.contains("asks again only if capabilities widen"),
            "{text:?}"
        );
    }
}
