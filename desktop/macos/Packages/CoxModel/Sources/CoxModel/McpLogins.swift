// The MCP page's logins (T37.30.3, DT§5.7): per server, the line saying whether cox can reach
// it and the button that changes that. Here, not in CoxUI, because these decide what the screen
// shows (DS§1); the app copies them into CoxUI's `SettingsScreen.Login` field for field.

import CoxClient

public struct McpLoginRow: Identifiable, Equatable, Sendable {
  public enum Action: Equatable, Sendable { case logIn, logOut }

  public let server: String
  public let detail: String
  /// `nil` for a server with no login.
  public let action: Action?
  public var id: String { server }
}

extension SettingsStore {
  /// One row per MCP server in effect, in name order.
  public var logins: [McpLoginRow] {
    (view?.mcp ?? []).map { server in
      let (detail, action): (String, McpLoginRow.Action?) =
        switch server.login {
        case .stdio: ("Runs locally from \(server.source); no login", nil)
        case .loggedOut: ("Not logged in", .logIn)
        case .loggedIn(let expires?): ("Logged in, expires in \(expires)", .logOut)
        case .loggedIn(nil): ("Logged in", .logOut)
        case .expired: ("Login expired", .logIn)
        case .unreadable(let error): ("Token store unreadable: \(error)", .logIn)
        }
      return McpLoginRow(server: server.name, detail: detail, action: action)
    }
  }
}
