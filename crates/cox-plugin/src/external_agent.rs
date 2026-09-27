//! A granted `[[external_agents]]` entry resolved to the process the host
//! spawns (EA§2, T35.2). Its own module for two reasons. The program
//! resolution — a PATH program as approved, or a regular file inside the
//! package reached through no symlink — is the one T33.19 gives a plugin's
//! `[[mcp]]` stdio server, so both call [`package_program`] here instead of
//! each keeping a copy. And the only constructor of [`ExternalAgentCommand`]
//! takes the caller's sandbox wrap, so an unwrapped external agent cannot be
//! built by accident: the host owns `sandbox::Policy`, this crate does not.

use std::ffi::OsStr;
use std::fmt::Display;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use cox_plugin_api::{AgentMode, ExternalAgentDecl};
use thiserror::Error;

/// Why a manifest `command` does not resolve to a program the grant covers.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProgramError {
    /// Absolute, or climbs out with `..`.
    #[error("command {0:?} must be a PATH program or a path inside the package")]
    Outside(String),
    /// A symlink on the way: the digest hashes regular files only.
    #[error("{0} is a symlink, which the package digest does not cover")]
    Symlink(String),
    /// Missing or unreadable.
    #[error("{path}: {message}")]
    Io {
        /// The path that failed.
        path: String,
        /// The OS error, as text.
        message: String,
    },
    /// Resolves to a directory or another non-file.
    #[error("{0} is not a file")]
    NotFile(String),
}

/// Why a granted `[[external_agents]]` entry is not offered this session.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExternalAgentError {
    /// `command` does not resolve to a program the grant covers.
    #[error(transparent)]
    Program(#[from] ProgramError),
    /// The sandbox wrap refused: plugin-shipped code never runs bare.
    #[error("cannot run under the sandbox: {0}")]
    Sandbox(String),
}

/// A bare name is a PATH program, approved as shown. Anything else must be
/// a regular file inside `dir`, reached through no symlink: the package
/// digest hashes regular files only, so a symlink would run bytes the grant
/// never covered.
pub fn package_program(dir: &Path, command: &str) -> Result<PathBuf, ProgramError> {
    let path = Path::new(command);
    let mut parts = path.components();
    if matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    ) {
        return Ok(path.to_path_buf());
    }
    if !path
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(ProgramError::Outside(command.to_string()));
    }
    let mut at = dir.to_path_buf();
    for part in path.components() {
        at.push(part);
        let meta = std::fs::symlink_metadata(&at).map_err(|e| ProgramError::Io {
            path: at.display().to_string(),
            message: e.to_string(),
        })?;
        if meta.file_type().is_symlink() {
            return Err(ProgramError::Symlink(at.display().to_string()));
        }
    }
    if !std::fs::metadata(&at).is_ok_and(|m| m.is_file()) {
        return Err(ProgramError::NotFile(at.display().to_string()));
    }
    Ok(at)
}

/// One granted entry, resolved and already wrapped by the host's sandbox:
/// what a driver spawns (EA§4 ACP, EA§5 stream-json). The fields are
/// private so the only way to one is [`ExternalAgentCommand::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAgentCommand {
    plugin: String,
    name: String,
    mode: AgentMode,
    key_env: String,
    /// The agent's own program before the wrap: a bare PATH name or the
    /// in-package file `package_program` checked.
    cli: PathBuf,
    program: String,
    args: Vec<String>,
}

impl ExternalAgentCommand {
    /// Resolves `decl` inside the package at `dir` with [`package_program`]
    /// and hands the program and its args to `wrap`, whose argv is what
    /// runs. A wrap error refuses the entry; there is no unwrapped fallback.
    pub fn resolve<E: Display>(
        plugin: &str,
        dir: &Path,
        decl: &ExternalAgentDecl,
        wrap: impl FnOnce(&Path, &[String]) -> Result<Vec<String>, E>,
    ) -> Result<Self, ExternalAgentError> {
        let cli = package_program(dir, &decl.command)?;
        let mut argv = wrap(&cli, &decl.args)
            .map_err(|e| ExternalAgentError::Sandbox(e.to_string()))?
            .into_iter();
        let program = argv
            .next()
            .ok_or_else(|| ExternalAgentError::Sandbox(String::from("the wrap gave no program")))?;
        Ok(Self {
            plugin: plugin.to_string(),
            name: decl.name.clone(),
            mode: decl.mode,
            key_env: decl.key_env.clone(),
            cli,
            program,
            args: argv.collect(),
        })
    }

    /// The plugin that declared the entry.
    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    /// The `agent(preset: <name>)` dispatch name (EA§3).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Which driver speaks to the process.
    pub fn mode(&self) -> AgentMode {
        self.mode
    }

    /// The env var the host resolves the key from (`resolve_key`, D12/A49).
    pub fn key_env(&self) -> &str {
        &self.key_env
    }

    /// The agent's program as the manifest names it, when it cannot run
    /// here: a bare name found in no `path` directory (the caller passes
    /// `PATH`). An in-package program was already checked by `resolve`.
    /// EA§7: such an entry is left out with one warning, never spawned to
    /// fail on every turn.
    pub fn missing_cli(&self, path: Option<&OsStr>) -> Option<&Path> {
        missing_on_path(&self.cli, path).then_some(self.cli.as_path())
    }

    /// The wrapped argv: the sandbox launcher first, the agent after it.
    pub fn argv(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.program.as_str()).chain(self.args.iter().map(String::as_str))
    }

    /// A fresh `Command` for the wrapped argv. Environment, stdio and the
    /// working directory are the driver's to set: the key goes in from
    /// `key_env` there, never through the manifest.
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd
    }
}

/// Whether `program`, as [`package_program`] resolved it, is a bare PATH
/// name found as an executable in no `path` directory. The one PATH lookup
/// for an external agent's CLI: the session leaves such an entry out
/// ([`ExternalAgentCommand::missing_cli`]) and `cox doctor` reports it
/// (T35.8), so the two can never disagree.
pub fn missing_on_path(program: &Path, path: Option<&OsStr>) -> bool {
    // `package_program` returns a bare name only for a PATH program; an
    // in-package one comes back joined onto the package directory.
    if program.components().count() > 1 {
        return false;
    }
    !path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .any(|dir| is_executable(&dir.join(program)))
}

/// What `exec` would run: a regular file, with an execute bit on unix.
fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(command: &str) -> ExternalAgentDecl {
        ExternalAgentDecl {
            name: "cursor".into(),
            command: command.into(),
            args: vec!["acp".into()],
            mode: AgentMode::Acp,
            key_env: "CURSOR_API_KEY".into(),
        }
    }

    fn wrap(program: &Path, args: &[String]) -> Result<Vec<String>, String> {
        let mut argv = vec![String::from("sandbox-exec"), String::from("-p")];
        argv.push(program.display().to_string());
        argv.extend(args.iter().cloned());
        Ok(argv)
    }

    #[test]
    fn resolved_command_runs_the_wrap_not_the_program() {
        let dir = tempfile::tempdir().expect("tempdir");
        let agent = ExternalAgentCommand::resolve("cur", dir.path(), &decl("agent"), wrap)
            .expect("resolves");
        let cmd = agent.command();
        assert_eq!(cmd.get_program(), "sandbox-exec");
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["-p", "agent", "acp"]);
        assert_eq!(
            agent.argv().collect::<Vec<_>>(),
            ["sandbox-exec", "-p", "agent", "acp"]
        );
        assert_eq!((agent.plugin(), agent.name()), ("cur", "cursor"));
        assert_eq!(
            (agent.mode(), agent.key_env()),
            (AgentMode::Acp, "CURSOR_API_KEY")
        );
    }

    #[test]
    fn wrap_failure_refuses_the_agent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let refused = ExternalAgentCommand::resolve("cur", dir.path(), &decl("agent"), |_, _| {
            Err::<Vec<String>, _>("no sandbox backend on this host")
        });
        assert_eq!(
            refused,
            Err(ExternalAgentError::Sandbox(
                "no sandbox backend on this host".into()
            ))
        );
        let empty = ExternalAgentCommand::resolve("cur", dir.path(), &decl("agent"), |_, _| {
            Ok::<_, String>(Vec::new())
        });
        assert!(matches!(empty, Err(ExternalAgentError::Sandbox(_))));
    }

    #[test]
    fn in_package_program_resolves_inside_the_package() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("bin")).expect("mkdir");
        std::fs::write(dir.path().join("bin/agent"), b"#!/bin/sh\n").expect("agent");
        let agent = ExternalAgentCommand::resolve("cur", dir.path(), &decl("bin/agent"), wrap)
            .expect("resolves");
        let inside = dir.path().join("bin/agent").display().to_string();
        assert_eq!(agent.argv().nth(2), Some(inside.as_str()));
        let missing = ExternalAgentCommand::resolve("cur", dir.path(), &decl("bin/gone"), wrap);
        assert!(matches!(
            missing,
            Err(ExternalAgentError::Program(ProgramError::Io { .. }))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn missing_cli_is_a_path_name_found_in_no_directory() {
        use std::os::unix::fs::PermissionsExt as _;

        let pkg = tempfile::tempdir().expect("tempdir");
        let bin = tempfile::tempdir().expect("tempdir");
        let agent = ExternalAgentCommand::resolve("cur", pkg.path(), &decl("agent"), wrap)
            .expect("resolves");
        let path = std::env::join_paths([bin.path()]).expect("path");
        assert_eq!(agent.missing_cli(Some(&path)), Some(Path::new("agent")));
        assert_eq!(agent.missing_cli(None), Some(Path::new("agent")));
        std::fs::write(bin.path().join("agent"), b"#!/bin/sh\n").expect("agent");
        assert!(agent.missing_cli(Some(&path)).is_some(), "not executable");
        std::fs::set_permissions(
            bin.path().join("agent"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
        assert_eq!(agent.missing_cli(Some(&path)), None);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_server_binary_is_refused() {
        let pkg = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(pkg.path().join("bin")).expect("mkdir");
        std::os::unix::fs::symlink("/bin/sh", pkg.path().join("bin/server")).expect("symlink");
        let err = package_program(pkg.path(), "bin/server").expect_err("not covered by the digest");
        assert!(matches!(err, ProgramError::Symlink(_)), "{err}");
        assert!(matches!(
            package_program(pkg.path(), "../outside"),
            Err(ProgramError::Outside(_))
        ));
        assert!(package_program(pkg.path(), "/bin/sh").is_err());
        assert_eq!(package_program(pkg.path(), "npx"), Ok(PathBuf::from("npx")));
    }
}
