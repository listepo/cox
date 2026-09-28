//! One open session behind the FFI (DT§4.4, §4.5): forwards to
//! `cox_app::LiveSession`, which builds it, feeds the inbox and runs each
//! intent; this side only moves the calls onto the runtime. Separate from
//! `lib.rs`, which owns what outlives one session.

use std::sync::Arc;

use cox_app::live::LiveSession;
use cox_app::{Block, Completion, Intent, TimelinePatch};
use cox_protocol::ids::SessionId;

use crate::{AppError, on_runtime};

#[derive(uniffi::Object)]
pub struct SessionHandle {
    live: Arc<LiveSession>,
}

impl SessionHandle {
    pub(crate) fn new(live: Arc<LiveSession>) -> Arc<Self> {
        Arc::new(Self { live })
    }
}

#[uniffi::export]
impl SessionHandle {
    pub fn id(&self) -> SessionId {
        self.live.id()
    }

    /// What was skipped while the session was built (D14).
    pub fn warnings(&self) -> Vec<String> {
        self.live.warnings().to_vec()
    }

    /// Every block; the next pull continues from here.
    pub fn snapshot(&self) -> Vec<Block> {
        self.live.snapshot()
    }

    /// The next batch, at most one per frame; `None` once closed.
    pub async fn next_patches(self: Arc<Self>) -> Option<Vec<TimelinePatch>> {
        on_runtime(async move { self.live.next_patches().await })
            .await
            .ok()
            .flatten()
    }

    /// Returns at once for a turn; a fork or handoff returns its child.
    pub async fn send(
        self: Arc<Self>,
        intent: Intent,
    ) -> Result<Option<Arc<SessionHandle>>, AppError> {
        Ok(on_runtime(async move { self.live.send(intent).await })
            .await??
            .map(Self::new))
    }

    /// `/` commands and `@` files for the composer's token.
    pub fn complete(&self, token: String, limit: u32) -> Vec<Completion> {
        self.live
            .complete(&token, usize::try_from(limit).unwrap_or(usize::MAX))
    }

    /// Stops the pull; the session keeps running (DT§4.5).
    pub fn close(&self) {
        self.live.close();
    }

    /// Quitting: ends every turn and kills what it detached (T38.2).
    pub fn end(&self) {
        self.live.end();
    }
}
