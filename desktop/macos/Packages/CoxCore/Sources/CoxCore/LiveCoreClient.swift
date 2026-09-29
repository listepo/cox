// The `CoreClient` over cox-ffi (DT§4.4, §4.6): opens sessions, reads the
// inbox, and reads and edits settings (DT§5.7), through the generated `App` and hands the
// stores `CoxClient` values, a terminal pane's shell among them (T51.6). Separate from the
// conversions, which are data only; this file is the one place the app
// calls into Rust.

import CoxClient
import CoxFFIBindings
import Foundation

public final class LiveCoreClient: CoreClient {
  /// Internal so the conversion files' extensions (the checklist) call it too.
  let app: App

  /// `home` is `COX_HOME`; `nil` means `~/.cox`. Call `loadLoginEnv()`
  /// first, once per launch (DT§4.8).
  public init(home: String?, host: any AppHost) throws {
    app = try App(home: home, host: host)
  }

  public func open(_ request: OpenSession) async throws -> any SessionClient {
    let handle = try await app.open(
      request: OpenRequest(
        cwd: request.cwd, resume: request.resume, theme: request.theme, agent: request.agent))
    return LiveSession(handle)
  }
}

extension LiveCoreClient: InboxClient {
  public func inbox() -> [CoxClient.InboxItem] { app.inbox().map { CoxClient.InboxItem($0) } }
}

final class LiveSession: SessionClient {
  private let handle: SessionHandle

  init(_ handle: SessionHandle) { self.handle = handle }

  var id: String { handle.id() }

  func snapshot() -> [CoxClient.Block] { handle.snapshot().map { CoxClient.Block($0) } }

  func nextPatches() async -> [CoxClient.TimelinePatch]? {
    await handle.nextPatches()?.map { CoxClient.TimelinePatch($0) }
  }

  func send(_ intent: CoxClient.Intent) async throws -> (any SessionClient)? {
    try await handle.send(intent: CoxFFIBindings.Intent(intent)).map { LiveSession($0) }
  }

  func complete(_ token: String, limit: UInt32) -> [CoxClient.Completion] {
    handle.complete(token: token, limit: limit).map {
      CoxClient.Completion(insert: $0.insert, detail: $0.detail)
    }
  }

  func palette(
    _ query: String, items: [CoxClient.PaletteItem], limit: UInt32
  ) -> [CoxClient.PaletteHit] {
    handle.palette(query: query, items: items.map { .init($0) }, limit: limit)
      .map { CoxClient.PaletteHit($0) }
  }

  func history(limit: UInt32) throws -> [String] { try handle.history(limit: limit) }

  func changes() async throws -> CoxClient.Changes {
    CoxClient.Changes(try await handle.changes())
  }

  func review(_ path: String) async throws -> CoxClient.DiffModel? {
    try await handle.review(path: path).map { CoxClient.DiffModel($0) }
  }

  func reviewMessage(_ comments: [CoxClient.LineComment]) -> String? {
    CoxFFIBindings.reviewMessage(comments: comments.map { CoxFFIBindings.LineComment($0) })
  }

  func plan() -> [CoxClient.TodoItem] { handle.plan().map { CoxClient.TodoItem($0) } }
  func openTask(_ task: String) throws -> CoxClient.TaskTarget? {
    try handle.openTask(task: task).map { CoxClient.TaskTarget($0) }
  }
  func output(archive: String) throws -> String { try handle.output(archive: archive) }
  func info() async throws -> CoxClient.Info { CoxClient.Info(try await handle.info()) }

  func turnCosts() async throws -> CoxClient.TurnCosts {
    CoxClient.TurnCosts(try await handle.turnCosts())
  }

  func openTerminal(cols: UInt16, rows: UInt16) throws -> any TerminalClient {
    LiveTerminal(try handle.openTerminal(cols: cols, rows: rows))
  }

  func close() { handle.close() }
  func closePluginOverlay() { handle.closePluginOverlay() }
  func pluginArea(width: UInt16, height: UInt16) { handle.pluginArea(width: width, height: height) }
}

// A local and a remote session build the one review prompt the same way (T52.21).
extension CoxFFIBindings.LineComment {
  init(_ value: CoxClient.LineComment) {
    self.init(path: value.path, line: value.line, removed: value.removed, text: value.text)
  }
}

/// A terminal pane's shell over the generated handle (T51.6): `outputs` pulls `nextOutput`
/// until the shell exits and its PTY drains.
final class LiveTerminal: TerminalClient {
  private let handle: TerminalHandle
  let outputs: AsyncStream<[UInt8]>

  init(_ handle: TerminalHandle) {
    self.handle = handle
    outputs = AsyncStream(unfolding: { await handle.nextOutput().map { [UInt8]($0) } })
  }

  func write(_ bytes: [UInt8]) throws { try handle.write(bytes: Data(bytes)) }
  func resize(cols: UInt16, rows: UInt16) throws { try handle.resize(cols: cols, rows: rows) }
  func exitStatus() -> UInt32? { handle.exitStatus() }
  func isBusy() -> Bool { handle.isBusy() }
  func close() { handle.close() }
}

/// CoxClient's, not the generated record of the same name.
public typealias ClientSettings = CoxClient.SettingsView

extension LiveCoreClient: SettingsClient {
  public func settings(cwd: String) async throws -> ClientSettings {
    ClientSettings(try await app.settings(cwd: cwd))
  }

  public func setSetting(cwd: String, key: String, json: String) async throws -> ClientSettings {
    ClientSettings(try await app.setSetting(cwd: cwd, key: key, value: json))
  }

  public func mcpLogin(cwd: String, server: String, login: Bool) async throws {
    try await app.mcpLogin(cwd: cwd, name: server, login: login)
  }

  public func setPermissionRule(
    cwd: String, kind: CoxClient.RuleKind, old: String?, new: String?
  ) async throws -> ClientSettings {
    do {
      return ClientSettings(
        try await app.setPermissionRule(cwd: cwd, kind: .init(kind), old: old, new: new))
    } catch AppError.Settings(let message) {
      // The grammar's or the layer's refusal, shown under the add row as Rust wrote it.
      throw RuleRefused(message)
    }
  }

  public func revokeGrant(
    cwd: String, grant: CoxClient.SessionGrant
  ) async throws -> ClientSettings {
    ClientSettings(try await app.revokeGrant(cwd: cwd, grant: .init(grant)))
  }
}
