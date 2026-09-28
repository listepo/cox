// MCP logins on the Settings screen (T37.30.3, DT§5.7): each server in effect for a directory
// with whether cox holds a token for it, field for field as cox-ffi exports `cox_app::McpServer`.
// Separate from `Settings.swift` because a login is no config value: it lives in the token store
// and changes through Log in / Log out, whose page Rust hands to `PlatformHost.open`.

/// Whether cox can reach a server without asking the person to log in.
public enum McpLogin: Equatable, Sendable {
  /// A stdio server: it runs locally and has no login.
  case stdio
  case loggedOut
  /// `expires` is how long the token has left (`3h`), `nil` when the server gave no expiry.
  case loggedIn(expires: String?)
  /// Expired with no refresh token: log in again.
  case expired
  /// The token store could not be read (a locked keychain).
  case unreadable(error: String)
}

/// A server's badge (T37.45.4), as `cox_app::McpStatus` decides it from the config, the token
/// store and the last session opened in the project.
public enum McpStatus: Equatable, Sendable {
  case connected, needsLogin, failed, disabled
  /// No session in the project has tried the server yet.
  case unknown
}

public struct McpServer: Identifiable, Equatable, Sendable {
  public var name: String
  /// Where it is configured: `config`, `.mcp.json`, `~/.claude.json`.
  public var source: String
  public var login: McpLogin
  public var status: McpStatus
  /// Why it failed, already sanitized and capped by Rust; empty when nothing went wrong.
  public var log: [String]

  public var id: String { name }

  public init(
    name: String, source: String, login: McpLogin, status: McpStatus = .unknown,
    log: [String] = []
  ) {
    (self.name, self.source, self.login, self.status, self.log) = (
      name, source, login, status, log
    )
  }
}
