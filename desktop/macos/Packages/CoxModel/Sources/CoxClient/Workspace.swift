// The sessions across projects the sidebar lists (DT§5.1, DT§4.3), field for field as cox-ffi
// exports `cox_app::Workspace` and the inbox's per-session activity. Separate from the session
// seam because it answers for every session in `cox.db`, not one open handle.

/// `cox_app::Project`: a git root with sessions, newest first.
public struct Project: Equatable, Sendable {
  public var root: String
  /// The root's last path component.
  public var name: String
  public var costUsd: Double

  public init(root: String, name: String, costUsd: Double = 0) {
    (self.root, self.name, self.costUsd) = (root, name, costUsd)
  }
}

/// `cox_app::SessionEntry`: one session row.
public struct SessionEntry: Equatable, Sendable {
  public var id: String
  /// `nil` until the core titled it.
  public var title: String?
  /// What the toolbar and the sidebar call it, as the core named it: its title, or `Untitled
  /// session` until it has one (T58.4.4).
  public var name: String
  public var cwd: String
  /// RFC 3339, its last write.
  public var updatedAt: String
  public var turns: Int64
  public var costUsd: Double
  /// Another process drives it (T37.34).
  public var isHeld: Bool
  /// The external ACP agent that drove it (T52.6); `nil` for cox.
  public var agent: String?
  /// The best-of-n group it was launched in (T52.9); the sidebar shows a group as one.
  public var bestOf: String?

  public init(
    id: String, title: String? = nil, name: String? = nil, cwd: String = "",
    updatedAt: String = "", turns: Int64 = 0, costUsd: Double = 0, isHeld: Bool = false,
    agent: String? = nil, bestOf: String? = nil
  ) {
    (self.id, self.title, self.cwd, self.updatedAt) = (id, title, cwd, updatedAt)
    // A fixture's entry without the core's name goes by its title.
    self.name = name ?? title ?? ""
    (self.turns, self.costUsd, self.isHeld, self.agent) = (turns, costUsd, isHeld, agent)
    self.bestOf = bestOf
  }
}

/// `cox_app::Activity`: what a session this process drives is doing.
public enum Activity: Equatable, Sendable { case idle, running, waitingOnYou, failed }

/// `cox_app::RowStatus`: a sidebar row's status dot.
public enum RowStatus: Equatable, Sendable { case running, waiting, idle, error }

/// `cox_app::SubtitlePart`: one part of a sidebar row's subtitle.
public enum SubtitlePart: Equatable, Sendable {
  case text(String)
  /// RFC 3339, the session's last write, which the client words in its locale (`2h ago`).
  case age(updatedAt: String)
}

/// `cox_app::SidebarRow`: an inbox item's or a session's row, as the core decided it.
public struct SidebarEntry: Equatable, Sendable {
  public var id, session: String
  public var status: RowStatus
  public var title: String
  public var subtitle: [SubtitlePart]
  /// `$0.42`; `nil` before it cost anything.
  public var cost: String?
  public var isReadOnly: Bool

  public init(
    id: String, session: String, status: RowStatus, title: String,
    subtitle: [SubtitlePart] = [], cost: String? = nil, isReadOnly: Bool = false
  ) {
    (self.id, self.session, self.status, self.title) = (id, session, status, title)
    (self.subtitle, self.cost, self.isReadOnly) = (subtitle, cost, isReadOnly)
  }
}

/// `cox_app::SidebarSection`: "Needs you", "Running" or a project, in the core's order.
public struct SidebarGroup: Equatable, Sendable {
  public enum Kind: Equatable, Sendable {
    case section(count: UInt32?)
    case project(isExpanded: Bool)
  }

  public var id, title: String
  public var kind: Kind
  public var rows: [SidebarEntry]

  public init(id: String, title: String, kind: Kind, rows: [SidebarEntry]) {
    (self.id, self.title, self.kind, self.rows) = (id, title, kind, rows)
  }
}

/// The workspace half of cox-ffi's `App`.
public protocol WorkspaceClient: Sendable {
  /// Most recently active first, at most `limit`.
  func projects(limit: UInt32) throws -> [Project]
  /// The sessions under `project`'s root, newest first, at most `limit`.
  func sessions(project: String, limit: UInt32) throws -> [SessionEntry]
  /// `.idle` for a session this process has not driven.
  func activity(session: String) -> Activity
  /// The sidebar's sections as the core decides them: `filter` matched against each row's title
  /// and words, `folded` the roots of the projects folded away.
  func sidebar(filter: String, folded: [String]) throws -> [SidebarGroup]
  /// Returns once the list may read differently: a commit to `cox.db` from another connection,
  /// or a session here that started, stopped or began to wait.
  func changed() async throws
  /// Sets the title of a session no window here has open, as the person's (A113); an open one
  /// renames through its `Intent.rename`. `false` when the title has no text.
  func rename(session: String, title: String) throws -> Bool
}

/// Fixed projects, sessions and activity: enough to drive the sidebar in a test or preview.
public struct FixtureWorkspace: WorkspaceClient {
  public var fixedProjects: [Project]
  /// By project root.
  public var fixedSessions: [String: [SessionEntry]]
  public var fixedActivity: [String: Activity]
  /// The sidebar as a core would send it, whatever the filter.
  public var fixedSidebar: [SidebarGroup]

  public init(
    projects: [Project] = [], sessions: [String: [SessionEntry]] = [:],
    activity: [String: Activity] = [:], sidebar: [SidebarGroup] = []
  ) {
    (fixedProjects, fixedSessions, fixedActivity) = (projects, sessions, activity)
    fixedSidebar = sidebar
  }

  public func projects(limit: UInt32) -> [Project] { Array(fixedProjects.prefix(Int(limit))) }

  public func sessions(project: String, limit: UInt32) -> [SessionEntry] {
    Array((fixedSessions[project] ?? []).prefix(Int(limit)))
  }

  public func activity(session: String) -> Activity { fixedActivity[session] ?? .idle }

  public func sidebar(filter: String, folded: [String]) -> [SidebarGroup] { fixedSidebar }

  /// A fixed workspace never changes: waits until cancelled.
  public func changed() async throws { try await Task.sleep(for: .seconds(86_400)) }

  /// A fixed workspace keeps its titles.
  public func rename(session: String, title: String) -> Bool { false }
}
