//! MCP logins on the Settings screen (DT§5.7, T37.30.3): every MCP server in
//! effect for a directory with whether cox holds a token for it, and Log in /
//! Log out through `cox-mcp`'s OAuth with the login page handed to the host.
//! Here rather than in Swift so the status and the flow's wiring are tested
//! once, over `cox_mcp::auth`'s memory store; the OAuth flow itself, the
//! token store and discovery stay `cox-mcp`'s and `cox-session`'s.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use cox_mcp::auth::{self, Secrets, Status};
use cox_protocol::Config;
use rmcp::transport::auth::{AuthError, CredentialStore};
use serde::Serialize;

/// Whether cox can reach a server without asking the person to log in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum McpLogin {
    /// A stdio server: it runs locally and has no login.
    Stdio,
    /// No token: the server may not ask for one, or the person has not
    /// logged in yet.
    LoggedOut,
    /// A token the server still accepts; `expires` is how long it has left
    /// (`3h`), `None` when the server gave no expiry.
    LoggedIn { expires: Option<String> },
    /// Expired with no refresh token: log in again.
    Expired,
    /// The token store could not be read (a locked keychain).
    Unreadable { error: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpServer {
    pub name: String,
    /// Where it is configured: `config`, `.mcp.json`, `~/.claude.json`.
    pub source: String,
    pub login: McpLogin,
}

#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error("no MCP server named `{0}`")]
    Unknown(String),
    #[error("`{0}` is a stdio server; only HTTP servers log in")]
    Stdio(String),
    #[error(transparent)]
    Auth(#[from] AuthError),
}

type Pending<'a> = Pin<Box<dyn Future<Output = Result<(), AuthError>> + Send + 'a>>;

/// How a login runs: [`Browser`] is `cox-mcp`'s authorization-code flow; a
/// test scripts the browser's callback instead.
pub trait Flow: Send + Sync {
    /// Logs in to the server at `url`, handing its login page to `open`,
    /// and leaves the token in `store`.
    fn login<'a>(
        &'a self,
        url: &'a str,
        store: Arc<dyn CredentialStore>,
        open: &'a (dyn Fn(&str) + Sync),
    ) -> Pending<'a>;
}

/// `cox_mcp::auth::login`: discovery, registration, PKCE and the loopback
/// callback, as `cox mcp login` runs them.
pub struct Browser;

impl Flow for Browser {
    fn login<'a>(
        &'a self,
        url: &'a str,
        store: Arc<dyn CredentialStore>,
        open: &'a (dyn Fn(&str) + Sync),
    ) -> Pending<'a> {
        Box::pin(auth::login(url, store, None, open))
    }
}

/// Where tokens live and how a login runs. The app's is the keyring the CLI
/// and every session use (`cox/mcp/<server>`) and the browser flow.
#[derive(Clone)]
pub struct McpAuth {
    pub secrets: Arc<dyn Secrets>,
    pub flow: Arc<dyn Flow>,
}

impl Default for McpAuth {
    fn default() -> Self {
        Self {
            secrets: Arc::new(auth::Keyring),
            flow: Arc::new(Browser),
        }
    }
}

/// The servers a session in `cwd` would connect, sorted by name, each with
/// its login.
pub async fn servers(config: &Config, cwd: &Path, secrets: &dyn Secrets) -> Vec<McpServer> {
    let found = cox_session::mcp_servers(config, cwd);
    let mut names: Vec<&String> = found.servers.keys().collect();
    names.sort();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let login = match found.servers.get(name).and_then(|s| s.url.as_ref()) {
            None => McpLogin::Stdio,
            Some(_) => login_of(secrets.store(name).load().await),
        };
        out.push(McpServer {
            name: name.clone(),
            source: found.sources.get(name).cloned().unwrap_or_default(),
            login,
        });
    }
    out
}

fn login_of(
    stored: Result<Option<rmcp::transport::auth::StoredCredentials>, AuthError>,
) -> McpLogin {
    match stored {
        Err(e) => McpLogin::Unreadable {
            error: e.to_string(),
        },
        Ok(creds) => match auth::status(creds.as_ref(), auth::now()) {
            Status::None => McpLogin::LoggedOut,
            Status::Ok { expires_in } => McpLogin::LoggedIn {
                expires: expires_in.map(auth::human),
            },
            Status::Expired => McpLogin::Expired,
        },
    }
}

/// Logs in to (`log_in`) or out of the HTTP server `name` in effect for
/// `cwd`; a login's page goes to `open`.
pub async fn set_login(
    config: &Config,
    cwd: &Path,
    name: &str,
    log_in: bool,
    mcp: &McpAuth,
    open: &(dyn Fn(&str) + Sync),
) -> Result<(), LoginError> {
    let found = cox_session::mcp_servers(config, cwd);
    let server = found
        .servers
        .get(name)
        .ok_or_else(|| LoginError::Unknown(name.to_string()))?;
    let url = server
        .url
        .as_deref()
        .ok_or_else(|| LoginError::Stdio(name.to_string()))?;
    let store = mcp.secrets.store(name);
    if log_in {
        mcp.flow.login(url, store, open).await?;
    } else {
        auth::logout(store).await?;
    }
    Ok(())
}
