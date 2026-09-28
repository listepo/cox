// The sidebar's session list (DT§5.1, DS§6.4 `Sidebar`): "Needs you" from the inbox, "Running"
// from each session's activity, then every project with its other sessions, each row with its
// status, what it did last and its cost; and the providers' footer. Here, not in CoxUI, because
// these decide what the list shows (DS§1); the app copies each section into CoxUI's
// `Sidebar.Group` field for field; `watch` re-reads the workspace each time it changed.

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
  }

  public let id: String
  public let title: String
  public let kind: Kind
  public let rows: [SidebarRow]
}

@Observable
@MainActor
public final class SidebarStore {
  /// Matched against each row's title and subtitle; empty lists everything.
  public var filter = ""
  /// The roots of the projects folded away.
  public private(set) var folded: Set<String> = []
  /// Why the last read failed; the next success clears it.
  public private(set) var failure: String?
  private var projects: [(project: Project, sessions: [SessionEntry])] = []
  private var activity: [String: Activity] = [:]
  private var readAt = Date.distantPast
  @ObservationIgnored private let workspace: (any WorkspaceClient)?
  @ObservationIgnored private let inbox: InboxStore?
  @ObservationIgnored private let locale: Locale

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
      activity = Dictionary(
        projects.flatMap(\.sessions).map { ($0.id, workspace.activity(session: $0.id)) },
        uniquingKeysWith: { first, _ in first })
      (readAt, failure) = (now, nil)
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
  }

  /// The session and its project, for the toolbar's title and breadcrumb.
  public func entry(_ session: String) -> (session: SessionEntry, project: Project)? {
    for (project, sessions) in projects {
      if let entry = sessions.first(where: { $0.id == session }) { return (entry, project) }
    }
    return nil
  }

  /// "Needs you" and "Running" while they hold a row, then every project that does; with a
  /// filter, a folded project opens to show what matched.
  public var sections: [SidebarSection] {
    let needs = (inbox?.rows ?? []).map(Self.row).filter(matches)
    var running: [SidebarRow] = []
    var groups: [SidebarSection] = []
    let ages = self.ages
    for (project, sessions) in projects {
      var rows: [SidebarRow] = []
      for entry in sessions {
        let row = row(entry, in: project, ages: ages)
        guard matches(row) else { continue }
        if row.status == .running { running.append(row) } else { rows.append(row) }
      }
      guard !rows.isEmpty || filter.isEmpty else { continue }
      let isExpanded = !folded.contains(project.root) || !filter.isEmpty
      groups.append(
        SidebarSection(
          id: project.root, title: project.name, kind: .project(isExpanded: isExpanded),
          rows: rows))
    }
    var sections: [SidebarSection] = []
    if !needs.isEmpty {
      sections.append(
        SidebarSection(
          id: "needs-you", title: "Needs you", kind: .section(count: "\(needs.count)"), rows: needs)
      )
    }
    if !running.isEmpty {
      sections.append(
        SidebarSection(id: "running", title: "Running", kind: .section(count: nil), rows: running))
    }
    return sections + groups
  }

  private func matches(_ row: SidebarRow) -> Bool {
    filter.isEmpty || row.title.localizedStandardContains(filter)
      || row.subtitle.localizedStandardContains(filter)
  }

  private static func row(_ inbox: InboxRow) -> SidebarRow {
    let status: SidebarRow.Status =
      switch inbox.status {
      case .waiting: .waiting
      case .idle: .idle
      case .error: .error
      }
    return SidebarRow(
      id: inbox.id, session: inbox.session, status: status, title: inbox.title,
      subtitle: inbox.subtitle, cost: nil, isReadOnly: inbox.isReadOnly)
  }

  private func row(
    _ entry: SessionEntry, in project: Project, ages: RelativeDateTimeFormatter
  ) -> SidebarRow {
    let ago = ChangesTabState.date(entry.updatedAt).map {
      ages.localizedString(for: $0, relativeTo: readAt)
    }
    let (status, subtitle): (SidebarRow.Status, [String?]) =
      switch activity[entry.id] ?? .idle {
      case .running: (.running, [project.name, "running"])
      case .waitingOnYou: (.waiting, [project.name, "waiting for you"])
      case .failed: (.error, [ago, "failed"])
      case .idle: (.idle, [ago, entry.turns > 0 ? "done" : nil])
      }
    return SidebarRow(
      id: entry.id, session: entry.id, status: status, title: entry.name,
      subtitle: subtitle.compactMap { $0 }.joined(separator: " · "),
      cost: entry.costUsd > 0 ? usd(entry.costUsd) : nil, isReadOnly: false)
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

extension SessionEntry {
  /// What the toolbar and the sidebar call it: its title, or `Untitled session` until it has one.
  public var name: String { title ?? Self.untitled }
  public static let untitled = "Untitled session"
}
