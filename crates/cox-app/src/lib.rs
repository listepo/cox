//! `cox-app` (DT§4.3): the UI-agnostic application core the desktop app
//! drives through `cox-ffi`: the pure fold over the core's `Event` stream,
//! the drain task that feeds it (`controller`) and the sessions themselves
//! (`app`, `live`), kept out of every surface crate so the TUI could share
//! it later and no UI toolkit leaks in (`crates/cox/tests/deps.rs`).

pub mod app;
pub mod coalesce;
pub mod complete;
pub mod controller;
pub mod inbox;
pub mod intent;
pub mod live;
pub mod onboarding;
pub mod patch;
pub mod settings;
pub mod summary;
pub mod timeline;
pub mod usage;
pub mod workspace;

pub use complete::{Completer, Completion};
pub use controller::Controller;
pub use inbox::{Activity, Inbox, InboxItem, Need};
pub use intent::{Dispatch, Intent, IntentError, dispatch};
pub use onboarding::{CheckId, CheckRow, CheckStatus};
pub use patch::{Block, BlockId, BlockKind, TimelinePatch, ToolState};
pub use settings::{Layer, Setting, SettingKind, SettingsError, SettingsView};
pub use summary::Icon;
pub use timeline::Timeline;
pub use usage::{Meter, Tally, TurnUsage, UsageView};
pub use workspace::{ProjectRow, SearchHit, SessionEntry, Workspace, WorkspaceError};

// What the exported types carry, named here so `cox-ffi` depends on no
// other workspace crate (T37.39, DT§4.2).
pub use cox_render::diffmodel;
pub use cox_render::doc;
pub use cox_store::fts::SessionInfo;
pub use cox_store::lock::Holder;
