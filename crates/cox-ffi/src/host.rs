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
    /// The badge fell with no new item: an approval or question was
    /// answered, or its session closed.
    fn badge(&self, badge: u32);
    /// An MCP server's login page, or a link the person asked to follow.
    fn open_url(&self, url: String);
    /// The Keychain secret stored for a provider section (`anthropic`,
    /// `openai`, a `[providers.<name>]`); its env var, when set, wins.
    fn secret(&self, section: String) -> Option<String>;
}

/// The Swift host as `cox_app::app::Host`, which takes borrowed strings.
pub(crate) struct Bridge(pub Arc<dyn AppHost>);

impl cox_app::app::Host for Bridge {
    fn notify(&self, item: InboxItem, badge: u32) {
        self.0.notify(item, badge);
    }
    fn badge(&self, badge: u32) {
        self.0.badge(badge);
    }
    fn open_url(&self, url: &str) {
        self.0.open_url(url.to_string());
    }
    fn secret(&self, section: &str) -> Option<String> {
        self.0.secret(section.to_string())
    }
}
