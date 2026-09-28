//! `AppHost` (DT§4.4's `Host`): the one trait Swift implements, for what
//! Rust must ask the OS to do — post a notification, open a URL, read a
//! Keychain item. Named apart from Foundation's `Host`, which every Swift
//! file imports. Separate from the exports so its contract is one short
//! file; tests use an in-memory host, never the real Keychain (A49).

use std::sync::Arc;

use cox_app::InboxItem;

/// Implemented in Swift (CoxPlatform).
#[uniffi::export(with_foreign)]
pub trait AppHost: Send + Sync {
    /// A new inbox item arrived; `badge` is the count that blocks a turn.
    fn notify(&self, item: InboxItem, badge: u32);
    /// An MCP server's login page, or a link the person asked to follow.
    fn open_url(&self, url: String);
    /// The Keychain secret stored for a provider section (`anthropic`,
    /// `openai`, a `[providers.<name>]`); its env var, when set, wins.
    fn secret(&self, section: String) -> Option<String>;
}

/// What `cox_session::open_with_keys` asks for a key.
pub(crate) fn keys(host: &Arc<dyn AppHost>) -> cox_session::Keys {
    let host = Arc::clone(host);
    Arc::new(move |section: &str| host.secret(section.to_string()))
}

/// What an MCP server's 401 hands its login URL to.
pub(crate) fn login(host: &Arc<dyn AppHost>) -> Arc<dyn Fn(&str) + Send + Sync> {
    let host = Arc::clone(host);
    Arc::new(move |url: &str| host.open_url(url.to_string()))
}
