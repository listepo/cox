// The window shell's values from the live stores (DT§5.1, T37.22.5): the toolbar from the open
// session's store, composer and Info, the sidebar from `SidebarStore` and the providers' health,
// copied into CoxUI's `SessionToolbar.State` and `Sidebar.State` field for field. Wiring only:
// CoxModel decided every text and status; separate from the window so its body stays the layout.

import CoxClient
import CoxModel
import CoxTranscript
import CoxUI

/// A session this window opened: its stores, what it reported at open, and its pull, which runs
/// while another session shows.
@MainActor
struct OpenedSession {
  let store: SessionStore
  let composer: ComposerStore
  let pull: Task<Void, Never>
  /// What it reported after it showed; nil until then.
  var info: Info?

  /// Stops the pull; the session keeps running in the core.
  func close() {
    pull.cancel()
    store.session.close()
  }
}

@MainActor
enum ShellState {
  static func toolbar(
    _ open: OpenedSession?, sidebar: SidebarStore, popover: SessionToolbar.Popover?
  ) -> SessionToolbar.State {
    guard let open else { return SessionToolbar.State(popover: popover) }
    let figures = ToolbarState(
      usage: open.store.usage, entry: sidebar.entry(open.store.session.id), info: open.info)
    return SessionToolbar.State(
      title: figures.title, project: figures.project, branch: figures.branch,
      model: open.composer.model ?? "", mode: open.store.status.mode.map(SessionMode.init) ?? .ask,
      cost: figures.cost, context: figures.context, contextFraction: figures.contextFraction,
      isRunning: open.store.isTurnRunning, popover: popover)
  }

  static func sidebar(
    _ store: SidebarStore, selection: String?, providers: ProviderHealth
  ) -> Sidebar.State {
    Sidebar.State(
      filter: store.filter, groups: store.sections.map(group), selection: selection,
      providers: providers.text, providerStatus: status(providers.status))
  }

  private static func group(_ section: SidebarSection) -> Sidebar.Group {
    let kind: Sidebar.Group.Kind =
      switch section.kind {
      case .section(let count): .section(count: count)
      case .project(let isExpanded): .project(isExpanded: isExpanded)
      }
    return Sidebar.Group(
      id: section.id, title: section.title, kind: kind,
      sessions: section.rows.map { row in
        Sidebar.Session(
          id: row.id,
          row: SessionRow.Item(
            status: status(row.status), title: row.title, subtitle: row.subtitle, cost: row.cost),
          session: row.session == row.id ? nil : row.session, isReadOnly: row.isReadOnly)
      })
  }

  private static func status(_ status: SidebarRow.Status) -> StatusDot.Status {
    switch status {
    case .running: .running
    case .waiting: .waiting
    case .idle: .idle
    case .error: .error
    }
  }
}
