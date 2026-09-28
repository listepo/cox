// The sidebar's workspace over cox-ffi (DT§5.1, DT§4.3): `App.projects`, `sessions`,
// `activity` and the change wait as CoxClient's values, the toolbar's model catalog and the
// footer's usable providers (T37.22.6, A110), the launch's login-shell environment (DT§4.8) and the
// browser pane's address check (T51.10), and the row conversions a remote host's list shares
// (T52.21). Separate
// from `LiveCoreClient.swift` like the other conversions, so that file stays the list of calls
// into Rust for one session.

import CoxClient
import CoxFFIBindings

extension LiveCoreClient: WorkspaceClient {
  public func projects(limit: UInt32) throws -> [CoxClient.Project] {
    try app.projects(limit: limit).map { CoxClient.Project($0) }
  }

  public func sessions(project: String, limit: UInt32) throws -> [CoxClient.SessionEntry] {
    try app.sessions(project: project, limit: limit).map { CoxClient.SessionEntry($0) }
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

  /// The menu bar's "Today" figures, `$4.02 · 7 sessions`, from the cost ledger (T51.12).
  public func today() throws -> String { try app.today().text }

  public func rename(session: String, title: String) throws -> Bool {
    try app.rename(session: session, title: title)
  }

  /// Reads the login shell's environment into this process; call once at launch, before a
  /// session opens. Returns why it kept the inherited environment, if it did.
  public static func loadLoginEnv() async throws -> String? {
    try await CoxFFIBindings.loadLoginEnv()
  }

  /// The browser pane's typed address as the URL to load, or `nil` when it is not an `http` or
  /// `https` page: Rust's check, the one `browser_open` makes (T51.10).
  public static func webAddress(_ text: String) -> String? {
    CoxFFIBindings.webAddress(text: text)
  }
}

// The local and the remote workspace list the same rows (T52.21).
extension CoxClient.Project {
  init(_ value: CoxFFIBindings.Project) {
    self.init(root: value.root, name: value.name, costUsd: value.costUsd)
  }
}

extension CoxClient.SessionEntry {
  init(_ value: CoxFFIBindings.SessionEntry) {
    self.init(
      id: value.info.id, title: value.info.title, cwd: value.info.cwd,
      updatedAt: value.info.updatedAt, turns: value.info.turns, costUsd: value.info.costUsd,
      isHeld: value.heldBy != nil, agent: value.agent)
  }
}

extension LiveCoreClient: AgentsClient {
  public func agents(cwd: String) async throws -> [CoxClient.AgentChoice] {
    try await app.agents(cwd: cwd).map {
      CoxClient.AgentChoice(
        name: $0.name, label: $0.label, origin: $0.origin, launch: $0.launch,
        unavailable: $0.unavailable)
    }
  }
}

extension LiveCoreClient: ModelsClient {
  public func models(cwd: String) throws -> [CoxClient.ModelChoice] {
    try app.models(cwd: cwd).map {
      CoxClient.ModelChoice(
        tier: CoxClient.Tier($0.tier), provider: $0.provider, id: $0.id,
        displayName: $0.displayName, efforts: $0.efforts.map { CoxClient.Effort($0) },
        contextWindow: $0.contextWindow)
    }
  }

  public func usableProviders(cwd: String) async throws -> [String] {
    try await app.usableProviders(cwd: cwd)
  }
}
