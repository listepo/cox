//! Background task list (T9.2): `/tasks` renders the running tasks the
//! core reports via `TaskCreated`/`TaskCompleted` (which `state` already
//! tracks for the status count).

use cox_protocol::ids::TaskId;

/// One line per running task, or the empty-state line when none run.
pub fn list(tasks: &[(TaskId, String)]) -> String {
    if tasks.is_empty() {
        return "no background tasks running".to_string();
    }
    let mut out = format!(
        "{} running background task{}",
        tasks.len(),
        if tasks.len() == 1 { "" } else { "s" }
    );
    for (id, label) in tasks {
        out.push_str(&format!("\n- {id}: {label}"));
    }
    out
}
