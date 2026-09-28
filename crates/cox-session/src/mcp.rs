//! MCP for a session (T7.6, T22.5, T33.42): which servers are in effect
//! for a directory, the auth a surface brings, and connecting them — every
//! stdio server under the sandbox wrap first. Separate from `open` because
//! `cox mcp` lists and logs in to the same servers without a session.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_protocol::Config;
use cox_protocol::config::McpServerConfig;
use cox_protocol::traits::Tool;

use crate::sandbox::sandboxed_argv;

/// The MCP servers in effect for `cwd`: config, `.mcp.json`, `~/.claude.json`.
pub fn mcp_servers(config: &Config, cwd: &Path) -> cox_mcp::discovery::Discovered {
    let project = cox_config::load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let home = cox_config::load::home_dir();
    cox_mcp::discovery::discover(&config.mcp.servers, Some(&project), Some(&home))
}

/// T22.5: tokens live in the keyring; with a person present (`login`, the
/// surface's own way to hand over the URL) a 401 starts a browser login,
/// headless surfaces get a notice.
pub fn mcp_auth(login: Option<cox_mcp::client::Prompt>) -> cox_mcp::client::Auth {
    cox_mcp::client::Auth {
        secrets: Arc::new(cox_mcp::auth::Keyring),
        prompt: login,
        ask: None,
    }
}

/// T7.6: every discovered MCP server's tools, connected on the runtime the
/// session will run on (the sessions live in the tools). A server that will
/// not start is a warning and no tools (D14).
/// `plugins` (T33.19) join discovery as its lowest-precedence source, and
/// every stdio server (theirs and the user's) is sandboxed (T33.42) before
/// `connect_all` spawns it. Returns the tools and the warnings, in order.
pub(crate) async fn mcp_tools(
    config: &Config,
    cwd: &Path,
    login: Option<cox_mcp::client::Prompt>,
    plugins: Vec<(String, Vec<(String, McpServerConfig)>)>,
    writable: &[PathBuf],
) -> (Vec<Arc<dyn Tool>>, Vec<String>) {
    let mut found = mcp_servers(config, cwd);
    for (id, servers) in plugins {
        cox_mcp::discovery::add_plugin(&mut found, &id, servers);
    }
    sandbox_stdio_servers(&mut found, config, writable);
    let timeout = std::time::Duration::from_secs(u64::from(config.mcp.timeout_s));
    let (_clients, tools, notices) = cox_mcp::client::connect_all(
        &found.servers,
        timeout,
        config.mcp.deferred,
        &mcp_auth(login),
    )
    .await;
    let warnings = found.notices.into_iter().chain(notices).collect();
    (tools, warnings)
}

/// T33.42 (`docs/design/plugins.md` §7c/§14 decision 4): every stdio MCP
/// server — config, `.mcp.json`, `~/.claude.json` — runs under the same
/// `sandboxed_argv` wrap a plugin's server gets, unless its config sets
/// `sandbox = false` or the session is `danger-full-access` (no wrap
/// either way). A plugin's own servers are skipped here: `plugin_mcp`
/// already wrapped them, refusing outright where the wrap is impossible
/// (T33.19). A user-configured server must not silently stop working on a
/// Landlock-only or backend-less host, so it keeps running unwrapped
/// there; one notice names every server that could not be sandboxed,
/// not one per server.
fn sandbox_stdio_servers(
    found: &mut cox_mcp::discovery::Discovered,
    config: &Config,
    writable: &[PathBuf],
) {
    if config.sandbox.mode == cox_protocol::SandboxMode::DangerFullAccess {
        return;
    }
    let mut names: Vec<String> = found.servers.keys().cloned().collect();
    names.sort();
    let mut unwrapped = Vec::new();
    for name in names {
        let is_plugin = found
            .sources
            .get(&name)
            .is_some_and(|s| s.starts_with("plugin:"));
        let Some(cfg) = found.servers.get(&name) else {
            continue;
        };
        if is_plugin || !cfg.sandbox {
            continue;
        }
        let Some(command) = cfg.command.clone() else {
            continue; // an HTTP server has nothing to wrap
        };
        let args = cfg.args.clone();
        match sandboxed_argv(Path::new(&command), &args, config, writable) {
            Ok(mut argv) => {
                if let Some(cfg) = found.servers.get_mut(&name) {
                    cfg.command = Some(argv.remove(0));
                    cfg.args = argv;
                }
            }
            Err(_) => unwrapped.push(name),
        }
    }
    if !unwrapped.is_empty() {
        found.notices.push(format!(
            "mcp: this host's sandbox cannot wrap a stdio server's argv; running unsandboxed: {}",
            unwrapped.join(", ")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T33.42 Check: `every_stdio_server_runs_under_sandbox_by_default`.
    /// Same proof as `plugin_stdio_server_runs_under_sandbox`, through the
    /// path a config/`.mcp.json`-sourced server takes instead of a
    /// plugin's: a write inside the workspace lands, one under `$HOME` is
    /// denied.
    #[cfg(unix)]
    #[tokio::test]
    async fn every_stdio_server_runs_under_sandbox_by_default() {
        use cox_tools::sandbox::{Backend, backend};
        use std::os::unix::fs::PermissionsExt as _;

        match backend(cox_protocol::LinuxBackend::Auto) {
            Some(Backend::Seatbelt | Backend::Bwrap) => {}
            other => {
                eprintln!("skipped: no argv sandbox backend here ({other:?})");
                return;
            }
        }
        let ws = tempfile::tempdir().expect("tempdir");
        let server = ws.path().join("server.sh");
        std::fs::write(
            &server,
            "#!/bin/sh\necho in > \"$1/inside\"\necho x > \"$2\"\n",
        )
        .expect("server");
        std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let home = std::env::var("HOME").expect("HOME");
        let outside = format!("{home}/.cox-mcp-escape-{}", std::process::id());
        let ws_arg = ws.path().display().to_string();

        let mut config = Config::default();
        config.core.workspace_roots = vec![ws.path().to_path_buf()];
        let writable = config.core.workspace_roots.clone();

        let mut found = cox_mcp::discovery::Discovered::default();
        found.servers.insert(
            "s".to_string(),
            McpServerConfig {
                command: Some(server.display().to_string()),
                args: vec![ws_arg, outside.clone()],
                ..McpServerConfig::default()
            },
        );
        found.sources.insert("s".to_string(), "config".to_string());

        sandbox_stdio_servers(&mut found, &config, &writable);
        assert!(found.notices.is_empty(), "{:?}", found.notices);

        // Not an MCP server, so the handshake fails once the script exits,
        // after both writes were tried.
        let timeout = std::time::Duration::from_secs(10);
        let auth = cox_mcp::client::Auth::none();
        let _ = cox_mcp::client::McpClient::connect("s", &found.servers["s"], timeout, &auth).await;

        let leaked = Path::new(&outside).exists();
        let _ = std::fs::remove_file(&outside);
        assert!(ws.path().join("inside").exists(), "the server never ran");
        assert!(!leaked, "the sandbox let a config server write {outside}");
    }

    /// T33.42 Check: `sandbox_false_opts_a_named_server_out`. A server
    /// with `sandbox = false` reaches `connect_all` with its argv
    /// untouched — not even the `/bin/sh -c 'exec "$0" "$@"'` shell hop.
    #[test]
    fn sandbox_false_opts_a_named_server_out() {
        let mut config = Config::default();
        config.core.workspace_roots = vec![PathBuf::from("/tmp")];
        let mut found = cox_mcp::discovery::Discovered::default();
        found.servers.insert(
            "s".to_string(),
            McpServerConfig {
                command: Some("echo".to_string()),
                args: vec!["hi".to_string()],
                sandbox: false,
                ..McpServerConfig::default()
            },
        );
        found.sources.insert("s".to_string(), "config".to_string());

        sandbox_stdio_servers(&mut found, &config, &[]);

        assert_eq!(found.servers["s"].command.as_deref(), Some("echo"));
        assert_eq!(found.servers["s"].args, ["hi"]);
        assert!(found.notices.is_empty(), "{:?}", found.notices);
    }
}
