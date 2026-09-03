//! `cox ext` (plan.md T7.3 step 3): what instruction files, skills, commands
//! and agent definitions are in effect for this cwd. Lives in the binary
//! because it needs both the config roots and every cox-ext discoverer;
//! hooks and MCP servers join the report in T7.4/T7.6.

use std::fmt::Write as _;
use std::path::Path;

use cox_ext::{agents, commands, instructions, skills};

use crate::cli::Cli;
use crate::config_load::{self, cox_home, find_git_root, home_dir};

pub fn report(cli: &Cli, cwd: &Path) -> String {
    let config = config_load::load(cwd, cli)
        .map(|l| l.config)
        .unwrap_or_default();
    let cox_home = cli.home.clone().unwrap_or_else(cox_home);
    let claude_home = home_dir().join(".claude");
    let git_root = find_git_root(cwd);
    let project = git_root.clone().unwrap_or_else(|| cwd.to_path_buf());
    let roots = instructions::Roots {
        cox_home: Some(cox_home.clone()),
        claude_home: Some(claude_home.clone()),
        git_root,
        cwd: cwd.to_path_buf(),
    };
    let mut out = String::new();
    let loaded = instructions::load(&roots, u32::MAX);
    section(
        &mut out,
        "instructions",
        loaded.files.iter().map(String::as_str),
    );
    let (ch, cl, pr) = (
        Some(cox_home.as_path()),
        Some(claude_home.as_path()),
        Some(project.as_path()),
    );
    let found = skills::discover(&skills::skill_dirs(ch, cl, pr));
    section(
        &mut out,
        "skills",
        found.skills.iter().map(|s| s.name.as_str()),
    );
    let cmds = commands::discover(&commands::command_dirs(ch, cl, pr));
    section(
        &mut out,
        "commands",
        cmds.commands.iter().map(|c| c.name.as_str()),
    );
    let defs = agents::discover(&agents::agent_dirs(ch, cl, pr));
    section(
        &mut out,
        "agents",
        defs.agents.iter().map(|a| a.name.as_str()),
    );
    let mcp = cox_mcp::discovery::discover(&config.mcp.servers, Some(&project), Some(&home_dir()));
    let mut servers: Vec<String> = mcp
        .servers
        .keys()
        .map(|n| format!("{n} ({})", mcp.sources[n]))
        .collect();
    servers.sort();
    section(&mut out, "mcp servers", servers.iter().map(String::as_str));
    let notices: Vec<String> = loaded
        .notices
        .into_iter()
        .chain(found.notices)
        .chain(cmds.notices)
        .chain(defs.notices)
        .chain(mcp.notices)
        .collect();
    section(&mut out, "notices", notices.iter().map(String::as_str));
    out
}

fn section<'a>(out: &mut String, title: &str, items: impl Iterator<Item = &'a str>) {
    let items: Vec<&str> = items.collect();
    if items.is_empty() {
        let _ = writeln!(out, "{title}: none");
        return;
    }
    let _ = writeln!(out, "{title}:");
    for item in items {
        let _ = writeln!(out, "  {item}");
    }
}
