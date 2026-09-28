// The sidebar's workspace over cox-ffi (DT§5.1, DT§4.3): `App.projects`, `sessions`,
// `activity` and the change wait as CoxClient's values, the toolbar's model catalog and the
// footer's usable providers (T37.22.6, A110), and the launch's login-shell environment (DT§4.8). Separate
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

  public func changed() async throws { try await app.workspaceChanged() }

  public func rename(session: String, title: String) throws -> Bool {
    try app.rename(session: session, title: title)
  }

  /// Reads the login shell's environment into this process; call once at launch, before a
  /// session opens. Returns why it kept the inherited environment, if it did.
  public static func loadLoginEnv() async throws -> String? {
    try await CoxFFIBindings.loadLoginEnv()
  }
}

extension LiveCoreClient: ModelsClient {
  public func models(cwd: String) throws -> [CoxClient.ModelChoice] {
    try app.models(cwd: cwd).map {
      CoxClient.ModelChoice(
        tier: CoxClient.Tier($0.tier), provider: $0.provider, id: $0.id,
        efforts: $0.efforts.map { CoxClient.Effort($0) }, contextWindow: $0.contextWindow)
    }
  }

  public func usableProviders(cwd: String) async throws -> [String] {
    try await app.usableProviders(cwd: cwd)
  }
}
