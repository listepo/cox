// The window shell's values from the live stores (DT§5.1, T37.22.5): the toolbar and its model
// popover from the open session's store, Info and model catalog (T37.22.6), the sidebar from
// `SidebarStore` and the providers' health, copied into CoxUI's `SessionToolbar.State`,
// `ModelPopover.State` and `Sidebar.State` field for field. Wiring only:
// CoxModel decided every text and status; separate from the window so its body stays the layout.

import CoxClient
import CoxModel
import CoxTranscript
import CoxUI
import Foundation

/// A session this window shows: the stores it shares through `AppStore` with every other window
/// on the session (their pull runs while another session shows), what it reported at open, and
/// this window's terminal tabs' views.
@MainActor
struct OpenedSession {
  let store: SessionStore
  let composer: ComposerStore
  /// What it reported after it showed; nil until then.
  var info: Info?
  /// The models its cwd's config offers; empty until read.
  var models: [ModelChoice] = []
  /// Its terminal tabs' views, kept while each tab is open (T51.6).
  let terminals = TerminalSurfaces()

  /// The toolbar's model menu for what the session runs on now.
  var menu: ModelMenu { ModelMenu(choices: models, status: store.status) }

  init(_ shared: AppStore.Shared) {
    (store, composer) = (shared.store, shared.composer)
  }

  /// This window stops showing the session: its terminal views detach, and the registry closes
  /// the session once no window shows it. The core keeps a running turn either way.
  func close(in registry: AppStore, window: UUID) {
    terminals.endAll()
    registry.release(store.session.id, window: window)
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

  static func models(_ menu: ModelMenu?) -> ModelPopover.State {
    ModelPopover.State(
      sections: (menu?.sections ?? []).map { section in
        ModelPopover.Section(
          title: section.title,
          rows: section.rows.map {
            CompletionList.Row(id: $0.id, title: $0.name, detail: $0.detail)
          },
          selected: section.rows.first(where: \.isSelected)?.id)
      })
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
