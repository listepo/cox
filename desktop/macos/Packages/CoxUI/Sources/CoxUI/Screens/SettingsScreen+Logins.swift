// The MCP page's logins box (T37.30.3, DT§5.7): per MCP server, whether cox can reach it and a
// Log in or Log out button. Composition only (DS§5): the line and the action arrive in the state
// from `CoxModel`'s `SettingsStore.logins`; the button reports `.setLogin`, and Rust opens the
// login page through the host. Apart from `SettingsScreen.swift` so the box is its own card.

import SwiftUI

/// What a server's login button does.
public enum LoginAction: Equatable, Sendable { case logIn, logOut }

extension SettingsScreen {
  public struct Login: Identifiable, Equatable, Sendable {
    /// The server's name.
    public let id: String
    /// `Logged in, expires in 3h`, `Not logged in`, …
    var detail: String
    /// `nil` for a server with no login (stdio).
    var action: LoginAction?

    public init(id: String, detail: String, action: LoginAction?) {
      (self.id, self.detail, self.action) = (id, detail, action)
    }
  }
}

/// A `SettingsGroupBox` with one `TitledSetting` per server, its button at the trailing edge.
struct LoginsBox: View {
  let logins: [SettingsScreen.Login]
  let send: (SettingsScreenIntent) -> Void

  var body: some View {
    SettingsGroupBox("Logins") {
      ForEach(logins) { login in
        TitledSetting(
          title: login.id, detail: login.detail, namesItem: true, control: button(login)
        )
        // `SettingRow`'s insets; a login has no config layer, so no badge.
        .padding(.horizontal, Space.l)
        .padding(.vertical, Space.ml)
      }
    }
  }

  @ViewBuilder private func button(_ login: SettingsScreen.Login) -> some View {
    if let action = login.action {
      Button(action == .logIn ? "Log in" : "Log out") {
        send(.setLogin(server: login.id, action == .logIn))
      }
      .buttonStyle(CoxButtonStyle(action == .logIn ? .primary : .secondary, size: .small))
    }
  }
}

#Preview("logins") {
  SettingsScreen(state: PreviewState.settingsMcp) { _ in }
    .frame(width: Size.windowMinWidth, height: Size.windowMinHeight)
    .padding(Space.xxl)
    .background(PreviewBackdrop())
}
