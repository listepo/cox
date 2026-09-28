// The `CoreClient` over cox-ffi (DT§4.4, §4.6): opens sessions, and reads
// and edits settings (DT§5.7), through the generated `App` and hands the
// stores `CoxClient` values. Separate from the
// conversions, which are data only; this file is the one place the app
// calls into Rust.

import CoxClient
import CoxFFIBindings

public final class LiveCoreClient: CoreClient {
  private let app: App

  /// `home` is `COX_HOME`; `nil` means `~/.cox`. Call `loadLoginEnv()`
  /// first, once per launch (DT§4.8).
  public init(home: String?, host: any AppHost) throws {
    app = try App(home: home, host: host)
  }

  public func open(_ request: OpenSession) async throws -> any SessionClient {
    let handle = try await app.open(
      request: OpenRequest(cwd: request.cwd, resume: request.resume, theme: request.theme))
    return LiveSession(handle)
  }
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

  func history(limit: UInt32) throws -> [String] { try handle.history(limit: limit) }

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
}
