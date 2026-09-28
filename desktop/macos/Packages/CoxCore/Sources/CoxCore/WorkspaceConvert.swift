// The sidebar's workspace over cox-ffi (DT§5.1, DT§4.3): `App.projects`, `sessions` and
// `activity` as CoxClient's values, and the launch's login-shell environment (DT§4.8). Separate
// from `LiveCoreClient.swift` like the other conversions, so that file stays the list of calls
// into Rust for one session.

import CoxClient
import CoxFFIBindings

extension LiveCoreClient: WorkspaceClient {
  public func projects(limit: UInt32) throws -> [CoxClient.Project] {
    try app.projects(limit: limit).map {
      CoxClient.Project(root: $0.root, name: $0.name, costUsd: $0.costUsd)
    }
  }

  public func sessions(project: String, limit: UInt32) throws -> [CoxClient.SessionEntry] {
    try app.sessions(project: project, limit: limit).map {
      CoxClient.SessionEntry(
        id: $0.info.id, title: $0.info.title, cwd: $0.info.cwd, updatedAt: $0.info.updatedAt,
        turns: $0.info.turns, costUsd: $0.info.costUsd, isHeld: $0.heldBy != nil)
    }
  }

  public func activity(session: String) -> CoxClient.Activity {
    switch app.activity(session: session) {
    case .idle: .idle
    case .running: .running
    case .waitingOnYou: .waitingOnYou
    case .failed: .failed
    }
  }

  /// Reads the login shell's environment into this process; call once at launch, before a
  /// session opens. Returns why it kept the inherited environment, if it did.
  public static func loadLoginEnv() async throws -> String? {
    try await CoxFFIBindings.loadLoginEnv()
  }
}
