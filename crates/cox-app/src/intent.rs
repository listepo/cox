//! The one enum the app sends (DT§4.3) and what each intent means to the
//! core: a `Submission`, a turn to spawn or hold, or a lineage call. Pure,
//! so a test checks an intent without a session, and separate from the
//! controller that executes the `Dispatch` (it owns the session and the
//! runtime).

use cox_protocol::CallId;
use cox_protocol::types::{
    Attachment, Decision, Effort, ModelId, PermissionMode, SlashCommand, Submission, Tier,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    /// `confirm_think`: this one turn goes to the think tier, as `/think`
    /// sends it (A103).
    Send {
        text: String,
        attachments: Vec<Attachment>,
        #[serde(default)]
        confirm_think: bool,
    },
    Approve {
        call: CallId,
        decision: Decision,
    },
    /// `None` dismisses the question unanswered.
    Answer {
        question: CallId,
        text: Option<String>,
    },
    Interrupt,
    /// Sent while a turn runs: becomes the next turn when it ends, with
    /// the `confirm_think` it was sent with.
    Queue {
        text: String,
        attachments: Vec<Attachment>,
        #[serde(default)]
        confirm_think: bool,
    },
    Compact {
        focus: Option<String>,
    },
    SetMode {
        mode: PermissionMode,
    },
    SwitchModel {
        tier: Tier,
        model: Option<ModelId>,
    },
    SetEffort {
        effort: Option<Effort>,
    },
    Rewind {
        to_turn: u32,
        code: bool,
        conversation: bool,
    },
    Redo,
    /// Restore one file to before `to_turn` (DT§5.4); `path` as the
    /// Changes tab lists it.
    RevertFile {
        path: String,
        to_turn: u32,
    },
    /// Put hunk `hunk` of Review's diff of `path` back (T51.20, DT§5.4):
    /// `to_turn` as for `RevertFile`, `now_digest` the `DiffModel`'s
    /// digest, so the core refuses bytes Review did not show.
    RevertHunk {
        path: String,
        to_turn: u32,
        hunk: u32,
        now_digest: String,
    },
    Fork {
        turn: Option<u32>,
    },
    Handoff {
        objective: String,
    },
    Background {
        call: CallId,
    },
    Shell {
        command: String,
        share: bool,
    },
    /// A composer line: `!cmd`/`!!cmd`, or `/name args`.
    Command {
        line: String,
    },
    /// The user's title for the session (A113): the toolbar's or a
    /// sidebar row's rename, as `/rename` sends it.
    Rename {
        title: String,
    },
}

/// What the controller does with an intent.
#[derive(Debug, Clone, PartialEq)]
pub enum Dispatch {
    /// Submit to the session. `spawn`: a turn, spawned and never awaited
    /// (R9.4.3), so `send` returns while it runs.
    Submit { submission: Submission, spawn: bool },
    /// Hold until the running turn ends, then spawn it.
    Queue(Submission),
    /// `cox_session::fork` the session up to `turn` and open the child.
    Fork { turn: Option<u32> },
    /// `cox_session::handoff` with a summary, then open the child.
    Handoff { objective: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IntentError {
    #[error("nothing to send")]
    Empty,
    #[error("`{0}` is neither a `/command` nor a `!` shell line")]
    NotACommand(String),
}

/// Maps `intent` to what the core should do with it.
pub fn dispatch(intent: Intent) -> Result<Dispatch, IntentError> {
    let now = |submission| {
        Ok(Dispatch::Submit {
            submission,
            spawn: false,
        })
    };
    match intent {
        Intent::Send {
            text,
            attachments,
            confirm_think,
        } => Ok(Dispatch::Submit {
            submission: turn(text, attachments, confirm_think)?,
            spawn: true,
        }),
        Intent::Queue {
            text,
            attachments,
            confirm_think,
        } => Ok(Dispatch::Queue(turn(text, attachments, confirm_think)?)),
        Intent::Approve { call, decision } => now(Submission::Approve {
            call_id: call,
            decision,
        }),
        Intent::Answer { question, text } => now(Submission::Answer {
            call_id: question,
            text,
        }),
        Intent::Interrupt => now(Submission::Interrupt),
        Intent::Compact { focus } => now(Submission::Compact { focus }),
        Intent::SetMode { mode } => now(Submission::SetPermissionMode { mode }),
        Intent::SwitchModel { tier, model } => now(Submission::SwitchModel { tier, model }),
        Intent::SetEffort { effort } => now(Submission::SetEffort { effort }),
        Intent::Rewind {
            to_turn,
            code,
            conversation,
        } => now(Submission::Rewind {
            to_turn,
            code,
            conversation,
        }),
        Intent::Redo => now(Submission::Redo),
        Intent::RevertFile { path, to_turn } => now(Submission::RevertFile { path, to_turn }),
        Intent::RevertHunk {
            path,
            to_turn,
            hunk,
            now_digest,
        } => now(Submission::RevertHunk {
            path,
            to_turn,
            hunk,
            now_digest,
        }),
        Intent::Fork { turn } => Ok(Dispatch::Fork { turn }),
        Intent::Handoff { objective } if objective.trim().is_empty() => Err(IntentError::Empty),
        Intent::Handoff { objective } => Ok(Dispatch::Handoff { objective }),
        Intent::Background { call } => now(Submission::Background { call_id: call }),
        Intent::Shell { command, share } => shell(command, share),
        Intent::Command { line } => command(&line),
        Intent::Rename { title } => rename(title),
    }
}

/// A turn needs text or an attachment.
fn turn(
    text: String,
    attachments: Vec<Attachment>,
    confirm_think: bool,
) -> Result<Submission, IntentError> {
    if text.trim().is_empty() && attachments.is_empty() {
        return Err(IntentError::Empty);
    }
    Ok(Submission::UserTurn {
        text,
        attachments,
        confirm_think,
    })
}

/// A rename needs a title; the core keeps its first line, sanitized.
fn rename(title: String) -> Result<Dispatch, IntentError> {
    if title.trim().is_empty() {
        return Err(IntentError::Empty);
    }
    Ok(Dispatch::Submit {
        submission: Submission::Rename { title },
        spawn: false,
    })
}

fn shell(command: String, share: bool) -> Result<Dispatch, IntentError> {
    if command.trim().is_empty() {
        return Err(IntentError::Empty);
    }
    Ok(Dispatch::Submit {
        submission: Submission::UserShell { command, share },
        spawn: true,
    })
}

/// `!!cmd` shares its output with the model, `!cmd` does not (T25.3);
/// `/name args` goes to the core as `Submission::Command`, the shape the
/// TUI submits for file commands and built-ins without a dedicated arm.
/// Built-ins that have their own intent (`/fork`, `/effort`, …) are sent as
/// that intent by the palette.
fn command(line: &str) -> Result<Dispatch, IntentError> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix('!') {
        return match rest.strip_prefix('!') {
            Some(cmd) => shell(cmd.trim().to_string(), true),
            None => shell(rest.trim().to_string(), false),
        };
    }
    let mut words = line
        .strip_prefix('/')
        .ok_or_else(|| IntentError::NotACommand(line.to_string()))?
        .split_whitespace();
    let name = words.next().ok_or(IntentError::Empty)?.to_string();
    if name == "rename" {
        return rename(words.collect::<Vec<_>>().join(" "));
    }
    let args = words.map(str::to_string).collect();
    Ok(Dispatch::Submit {
        submission: Submission::Command {
            command: SlashCommand { name, args },
        },
        spawn: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revert_hunk_intent_maps_to_the_submission() {
        let intent = Intent::RevertHunk {
            path: "src/a.rs".into(),
            to_turn: 2,
            hunk: 1,
            now_digest: "00ff".into(),
        };
        assert_eq!(
            dispatch(intent),
            Ok(Dispatch::Submit {
                submission: Submission::RevertHunk {
                    path: "src/a.rs".into(),
                    to_turn: 2,
                    hunk: 1,
                    now_digest: "00ff".into(),
                },
                spawn: false,
            })
        );
    }
}
