//! Instruction files (`AGENTS.md`/`CLAUDE.md` hierarchy), skills
//! (`SKILL.md`), slash commands, subagent definitions, and hook config.
//! Separate because these are user- and repo-supplied extension points, not
//! core agent logic.

pub mod agents;
pub mod commands;
pub mod frontmatter;
pub mod instructions;
pub mod skills;
