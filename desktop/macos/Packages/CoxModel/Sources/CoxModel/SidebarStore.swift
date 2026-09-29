// The sidebar's session list (DT§5.1, DS§6.4 `Sidebar`): the core's sections (T58.4.4) — "Needs
// you", "Running", then every project — with each row's subtitle parts joined and its age worded
// in the locale; and the providers' footer. Here, not in CoxUI, because the store owns what the
// list shows (DS§1); the app copies each section into CoxUI's `Sidebar.Group` field for field;
// `watch` re-reads the workspace each time it changed.

import CoxClient
import Foundation
import Observation

/// One row: an inbox item's or a session's.
public struct SidebarRow: Identifiable, Equatable, Sendable {
  /// The status glyph, named as CoxUI's `StatusDot.Status`.
  public enum Status: Equatable, Sendable { case running, waiting, idle, error }

  public let id: String
  /// The session a click opens.
  public let session: String
  public let status: Status
  public let title: String
  /// `cox · running`, `2h ago · done`.
  public let subtitle: String
  /// `$0.42`; `nil` before it cost anything.
  public let cost: String?
  public let isReadOnly: Bool
}

/// A status section or a project, as CoxUI's `Sidebar.Group`.
public struct SidebarSection: Identifiable, Equatable, Sendable {
  public enum Kind: Equatable, Sendable {
    case section(count: String?)
    case project(isExpanded: Bool)
    /// A remote host's sessions (T52.21), and whether its connection holds.
    case host(isConnected: Bool)
  }

  public let id: String
  public let title: String
  public let kind: Kind
  public let rows: [SidebarRow]
}

@Observable
@MainActor
public final class SidebarStore {
  /// The core matches it against each row's title and words; empty lists everything.
  public var filter = "" {
    didSet { if filter != oldValue { reload() } }
  }
  /// The roots of the projects folded away.
  public private(set) var folded: Set<String> = []
  /// The sections as the core last sent them.
  private var groups: [SidebarGroup] = []
  /// Why the last read failed; the next success clears it.
  public private(set) var failure: String?
  private var projects: [(project: Project, sessions: [SessionEntry])] = []
  private var readAt = Date.distantPast
  @ObservationIgnored private let workspace: (any WorkspaceClient)?
  @ObservationIgnored private let inbox: InboxStore?
  @ObservationIgnored private let locale: Locale
  /// Called after each successful read; the app re-syncs its Spotlight index from `listed`
  /// (T51.16).
  @ObservationIgnored public var didRefresh: (@MainActor () -> Void)?

  /// A fixture launch has no workspace, a recording no inbox: the list shows what there is.
  public init(
    workspace: (any WorkspaceClient)?, inbox: InboxStore?, locale: Locale = .current
  ) {
    (self.workspace, self.inbox, self.locale) = (workspace, inbox, locale)
  }

  /// The projects the list reads, and the sessions it reads of each.
  static let projectLimit: UInt32 = 20
  static let sessionLimit: UInt32 = 20

  /// Reads the inbox, the projects and their sessions again.
  public func refresh(now: Date = .now) {
    inbox?.refresh()
    guard let workspace else { return }
    do {
      projects = try workspace.projects(limit: Self.projectLimit).map {
        ($0, try workspace.sessions(project: $0.root, limit: Self.sessionLimit))
      }
      groups = try workspace.sidebar(filter: filter, folded: folded.sorted())
      (readAt, failure) = (now, nil)
      didRefresh?()
    } catch {
      failure = String(describing: error)
    }
  }

  /// Re-reads the list now and each time the workspace may read differently, until cancelled:
  /// a commit to `cox.db` from a session or another process, or a session here that started,
  /// stopped or began to wait (T37.22.6). A failed wait re-reads after `retry` instead.
  public func watch(retry: Duration = .seconds(2)) async {
    refresh()
    guard let workspace else {
      // A recording's inbox still changes as it replays; there is nothing to wait on.
      while !Task.isCancelled {
        try? await Task.sleep(for: retry)
        refresh()
      }
      return
    }
    while !Task.isCancelled {
      do {
        try await workspace.changed()
      } catch {
        try? await Task.sleep(for: retry)
      }
      guard !Task.isCancelled else { return }
      refresh()
    }
  }

  /// Renames a session no window here has open (A113), then reads the list again.
  public func rename(_ session: String, to title: String) {
    guard let workspace else { return }
    do {
      _ = try workspace.rename(session: session, title: title)
      refresh()
    } catch {
      failure = String(describing: error)
    }
  }

  public func toggle(_ project: String) {
    if folded.remove(project) == nil { folded.insert(project) }
    reload()
  }

  /// Asks the core for the sections again after the filter or a fold changed.
  private func reload() {
    guard let workspace else { return }
    do {
      groups = try workspace.sidebar(filter: filter, folded: folded.sorted())
      failure = nil
    } catch {
      failure = String(describing: error)
    }
  }

  /// The session and its project, for the toolbar's title and breadcrumb.
  public func entry(_ session: String) -> (session: SessionEntry, project: Project)? {
    for (project, sessions) in projects {
      if let entry = sessions.first(where: { $0.id == session }) { return (entry, project) }
    }
    return nil
  }

  /// Every session the list read, with its project and its last write; Spotlight mirrors it
  /// (T51.16).
  public var listed: [ListedSession] {
    projects.flatMap { entry in
      entry.sessions.map {
        ListedSession(
          session: $0, project: entry.project, updated: ChangesTabState.date($0.updatedAt))
      }
    }
  }

  /// The listed sessions as command-palette rows (T37.44.13): each title over its project and
  /// how long ago it last wrote, as the sidebar says it.
  public var paletteItems: [PaletteItem] {
    let ages = self.ages
    return listed.map { listed in
      let ago = listed.updated.map { ages.localizedString(for: $0, relativeTo: readAt) }
      return PaletteItem(
        kind: .session, id: listed.session.id, title: listed.session.name,
        detail: [listed.project.name, ago].compactMap { $0 }.joined(separator: " · "))
    }
  }

  /// The inbox's items as the core sent them; the menu bar reads them (T51.14).
  public var inboxItems: [InboxItem] { inbox?.items ?? [] }

  /// The core's sections, each row's subtitle parts joined with ` · ` and its age worded here.
  /// Without a workspace (a fixture launch) nothing ranks the list, so the inbox's rows alone
  /// show as "Needs you".
  public var sections: [SidebarSection] {
    guard workspace != nil else { return inboxOnly }
    let ages = self.ages
    return groups.map { group in
      let kind: SidebarSection.Kind =
        switch group.kind {
        case .section(let count): .section(count: count.map { "\($0)" })
        case .project(let isExpanded): .project(isExpanded: isExpanded)
        }
      return SidebarSection(
        id: group.id, title: group.title, kind: kind,
        rows: group.rows.map { row($0, ages: ages) })
    }
  }

  private var inboxOnly: [SidebarSection] {
    let rows = (inbox?.rows ?? []).map(Self.row)
    guard !rows.isEmpty else { return [] }
    return [
      SidebarSection(
        id: "needs-you", title: "Needs you", kind: .section(count: inbox?.count), rows: rows)
    ]
  }

  private static func row(_ inbox: InboxRow) -> SidebarRow {
    SidebarRow(
      id: inbox.id, session: inbox.session, status: SidebarRow.Status(inbox.status),
      title: inbox.title, subtitle: inbox.subtitle, cost: nil, isReadOnly: inbox.isReadOnly)
  }

  private func row(_ entry: SidebarEntry, ages: RelativeDateTimeFormatter) -> SidebarRow {
    let words = entry.subtitle.compactMap { part -> String? in
      switch part {
      case .text(let text): text
      case .age(let updatedAt):
        ChangesTabState.date(updatedAt).map { ages.localizedString(for: $0, relativeTo: readAt) }
      }
    }
    return SidebarRow(
      id: entry.id, session: entry.session, status: SidebarRow.Status(entry.status),
      title: entry.title, subtitle: words.joined(separator: " · "), cost: entry.cost,
      isReadOnly: entry.isReadOnly)
  }

  /// `2h ago`, `yesterday`: how long ago a session last wrote, as of the last read.
  private var ages: RelativeDateTimeFormatter {
    let ages = RelativeDateTimeFormatter()
    (ages.locale, ages.unitsStyle, ages.dateTimeStyle) = (locale, .abbreviated, .named)
    return ages
  }
}

/// The footer's providers (DT§5.1): how many a turn could run on now (A110: a key found or a local
/// server listening, not every configured section), and the health of the one the code tier runs
/// on as the first-run checklist's `provider_key` row found it.
public struct ProviderHealth: Equatable, Sendable {
  /// `3 providers`; empty before the core counted them.
  public var text = ""
  /// The dot, as a row's: `running` glows green.
  public var status = SidebarRow.Status.idle

  public init() {}

  /// `usable` is nil until the core has probed the providers.
  public init(usable: [String]?, check: CheckRow?) {
    if let usable {
      text = "\(usable.count) \(usable.count == 1 ? "provider" : "providers")"
    }
    status =
      switch check?.status {
      case .passed: .running
      case .warning: .waiting
      case .failed: .error
      case nil: .idle
      }
  }
}

extension SidebarRow.Status {
  init(_ status: RowStatus) {
    self =
      switch status {
      case .running: .running
      case .waiting: .waiting
      case .idle: .idle
      case .error: .error
      }
  }

  init(_ status: InboxStatus) {
    self =
      switch status {
      case .waiting: .waiting
      case .idle: .idle
      case .error: .error
      }
  }
}

/// One row of `SidebarStore.listed`: a session, its project and its last write.
public struct ListedSession: Sendable {
  public let session: SessionEntry
  public let project: Project
  public let updated: Date?
}
