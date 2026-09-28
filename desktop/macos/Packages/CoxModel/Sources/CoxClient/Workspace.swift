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
  public var cwd: String
  /// RFC 3339, its last write.
  public var updatedAt: String
  public var turns: Int64
  public var costUsd: Double
  /// Another process drives it (T37.34).
  public var isHeld: Bool

  public init(
    id: String, title: String? = nil, cwd: String = "", updatedAt: String = "", turns: Int64 = 0,
    costUsd: Double = 0, isHeld: Bool = false
  ) {
    (self.id, self.title, self.cwd, self.updatedAt) = (id, title, cwd, updatedAt)
    (self.turns, self.costUsd, self.isHeld) = (turns, costUsd, isHeld)
  }
}

/// `cox_app::Activity`: what a session this process drives is doing.
public enum Activity: Equatable, Sendable { case idle, running, waitingOnYou, failed }

/// The workspace half of cox-ffi's `App`.
public protocol WorkspaceClient: Sendable {
  /// Most recently active first, at most `limit`.
  func projects(limit: UInt32) throws -> [Project]
  /// The sessions under `project`'s root, newest first, at most `limit`.
  func sessions(project: String, limit: UInt32) throws -> [SessionEntry]
  /// `.idle` for a session this process has not driven.
  func activity(session: String) -> Activity
}

/// Fixed projects, sessions and activity: enough to drive the sidebar in a test or preview.
public struct FixtureWorkspace: WorkspaceClient {
  public var fixedProjects: [Project]
  /// By project root.
  public var fixedSessions: [String: [SessionEntry]]
  public var fixedActivity: [String: Activity]

  public init(
    projects: [Project] = [], sessions: [String: [SessionEntry]] = [:],
    activity: [String: Activity] = [:]
  ) {
    (fixedProjects, fixedSessions, fixedActivity) = (projects, sessions, activity)
  }

  public func projects(limit: UInt32) -> [Project] { Array(fixedProjects.prefix(Int(limit))) }

  public func sessions(project: String, limit: UInt32) -> [SessionEntry] {
    Array((fixedSessions[project] ?? []).prefix(Int(limit)))
  }

  public func activity(session: String) -> Activity { fixedActivity[session] ?? .idle }
}
