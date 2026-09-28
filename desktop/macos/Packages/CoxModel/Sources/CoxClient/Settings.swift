// The Settings screen's values (DT§5.7): every config leaf with the layer it
// came from and the control its schema gives it, field for field as cox-ffi
// exports `cox_app::settings`, and the `SettingsClient` seam the store reads
// them through. Separate from the timeline because Settings is its own
// window with its own client; `LiveCoreClient` (CoxCore) is the Rust side,
// `FixtureSettingsClient` the one previews and tests use (DT§8).

import Synchronization

/// Where a value came from: the badge beside each field.
public enum Layer: String, Equatable, Sendable {
  case `default`, user, project, env, flag
  case claudeSettings = "claude-settings"
}

/// The control a field takes, from `docs/config.jsonschema`.
public enum SettingKind: Equatable, Sendable {
  case toggle
  case integer(min: Double?, max: Double?)
  case number(min: Double?, max: Double?)
  case text
  case choice(options: [String])
  case list
  /// A shape the schema leaves open (plugin tables, hook entries).
  case other
}

public struct Setting: Identifiable, Equatable, Sendable {
  /// Dotted, as `cox config set` takes it.
  public var key: String
  /// JSON text.
  public var value: String
  public var layer: Layer
  /// `false` once a layer above the user file sets the key: an edit there
  /// would not take effect, so the field is read-only (DT§5.7).
  public var editable: Bool
  public var kind: SettingKind
  public var description: String

  public var id: String { key }

  public init(
    key: String, value: String, layer: Layer, editable: Bool, kind: SettingKind,
    description: String
  ) {
    (self.key, self.value, self.layer, self.editable) = (key, value, layer, editable)
    (self.kind, self.description) = (kind, description)
  }
}

public struct SettingsView: Equatable, Sendable {
  /// Sorted by key.
  public var settings: [Setting]
  /// Where edits go.
  public var userFile: String
  /// The project's `.cox/config.toml`, when there is one.
  public var projectFile: String?
  /// The MCP servers in effect and their logins, sorted by name.
  public var mcp: [McpServer]

  public init(
    settings: [Setting], userFile: String, projectFile: String? = nil, mcp: [McpServer] = []
  ) {
    (self.settings, self.userFile, self.projectFile, self.mcp) = (
      settings, userFile, projectFile, mcp
    )
  }
}

public protocol SettingsClient: Sendable {
  /// The effective config for a session in `cwd`.
  func settings(cwd: String) async throws -> SettingsView
  /// Writes `json` for `key` to the user file; the view after the edit.
  /// Rust refuses a read-only key and a value the config loader rejects.
  func setSetting(cwd: String, key: String, json: String) async throws -> SettingsView
  /// Logs in to (`login`) or out of the MCP server `server`; a login's page goes to the host's
  /// `open` and the call returns once the browser comes back.
  func mcpLogin(cwd: String, server: String, login: Bool) async throws
}

/// A fixed view that takes edits the way Rust does for an editable key:
/// the value changes and its layer becomes `user`. Keeps what it was sent.
/// A login opens `loginPage` through `host`, then its callback is scripted: the
/// server is logged in with an hour left.
public final class FixtureSettingsClient: SettingsClient {
  public struct ReadOnly: Error, Equatable { public let key: String }
  public struct NoLogin: Error, Equatable { public let server: String }

  public static let loginPage = "https://auth.example.test/authorize?client_id=cox"

  private let state: Mutex<(view: SettingsView, sent: [String])>
  private let host: (any PlatformHost)?

  public init(view: SettingsView, host: (any PlatformHost)? = nil) {
    state = Mutex((view, []))
    self.host = host
  }

  /// `key=json`, in order.
  public var sent: [String] { state.withLock { $0.sent } }

  public func settings(cwd: String) async throws -> SettingsView {
    state.withLock { $0.view }
  }

  public func setSetting(cwd: String, key: String, json: String) async throws -> SettingsView {
    try state.withLock { state in
      guard let index = state.view.settings.firstIndex(where: { $0.key == key }),
        state.view.settings[index].editable
      else { throw ReadOnly(key: key) }
      state.sent.append("\(key)=\(json)")
      state.view.settings[index].value = json
      state.view.settings[index].layer = .user
      return state.view
    }
  }

  public func mcpLogin(cwd: String, server: String, login: Bool) async throws {
    let index = state.withLock { state in
      state.view.mcp.firstIndex { $0.name == server && $0.login != .stdio }
    }
    guard let index else { throw NoLogin(server: server) }
    if login { host?.open(Self.loginPage) }
    state.withLock { state in
      state.sent.append("\(login ? "login" : "logout")=\(server)")
      state.view.mcp[index].login = login ? .loggedIn(expires: "1h") : .loggedOut
    }
  }
}
