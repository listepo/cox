// `SettingsScreen` (DS§6.5; DT§5.7): the Settings window — the page list, and the selected
// page's config tables as boxes of `SettingRow`s, each value with the layer it comes from, and
// a secure key field in each provider's box. Composition only (DS§5): what each field shows,
// whether it is read-only and which file sets it arrive in `state`; the app binds `state` and
// `send` to `SettingsStore`.

import SwiftUI

/// The pages that hold settings, the selected one and its tables, and the files behind them.
struct SettingsScreenState: Equatable, Sendable {
  var pages: [SettingsPage] = []
  var selection = SettingsPage.general
  /// The selected page's tables, in key order.
  var tables: [SettingsScreen.Table] = []
  var userFile = ""
  var projectFile: String?
  /// The MCP servers' logins, on the MCP page.
  var logins: [SettingsScreen.Login] = []
}

/// Every intent the Settings screen reports.
enum SettingsScreenIntent: Equatable, Sendable {
  case select(SettingsPage)
  /// A new value for `key`; a slider reports it while it moves.
  case set(key: String, SettingsScreen.Edit)
  /// A key typed for a provider section, bound for its `SecretStore`.
  case storeKey(provider: String, secret: String)
  /// Log in to (`true`) or out of an MCP server.
  case setLogin(server: String, Bool)
}

/// The Settings window: `SettingsSidebar` beside a column of `SettingsGroupBox`es.
struct SettingsScreen: View {
  let state: SettingsScreenState
  let send: (SettingsScreenIntent) -> Void

  var body: some View {
    ShellPane(.window) {
      HStack(spacing: Size.paneGap) {
        SettingsSidebar(
          pages: state.pages, selection: state.selection, userFile: state.userFile,
          projectFile: state.projectFile
        ) { send(.select($0)) }
        ShellPane(.column) {
          ScrollView {
            VStack(alignment: .leading, spacing: Space.xl) {
              if !state.logins.isEmpty { LoginsBox(logins: state.logins, send: send) }
              ForEach(state.tables) { TableBox(table: $0, send: send) }
            }
            .frame(maxWidth: Size.readingWidth)
            .padding(Space.xxl)
            .frame(maxWidth: .infinity)
          }
        }
      }
      .padding(Size.paneGap)
    }
  }
}

extension SettingsScreen {
  /// One config table: its settings and, for a provider's table, the key field.
  struct Table: Identifiable, Equatable, Sendable {
    /// The keys' shared prefix, `tiers.code`; the box's header.
    let id: String
    var fields: [Field]
    var key: Key?
  }

  struct Field: Identifiable, Equatable, Sendable {
    /// The dotted config key.
    let id: String
    var title: String
    /// The schema's help text, or the file a read-only value is set in.
    var detail: String?
    var source: SettingSource
    var control: Control
  }

  /// A provider section's key: whether one is stored, never the key itself.
  struct Key: Equatable, Sendable {
    var provider: String
    var isStored: Bool
  }

  enum Control: Equatable, Sendable {
    case toggle(Bool)
    case slider(Double, range: ClosedRange<Double>, text: String)
    case choice(String, options: [String])
    case field(String)
    /// A value Settings shows but does not edit.
    case json(String)
  }

  enum Edit: Equatable, Sendable {
    case bool(Bool)
    case number(Double)
    /// Typed or chosen text; the store types it by the field's kind.
    case text(String)
  }
}

/// A table's box: the key field first, then a row per setting.
private struct TableBox: View {
  let table: SettingsScreen.Table
  let send: (SettingsScreenIntent) -> Void

  var body: some View {
    SettingsGroupBox(table.id) {
      if let key = table.key {
        TitledSetting(
          title: "API key", detail: key.isStored ? "Stored in the Keychain" : "No key",
          control: SettingField(
            "", prompt: key.isStored ? "Replace key" : "Add key", isSecure: true
          ) { send(.storeKey(provider: key.provider, secret: $0)) }
        )
        // `SettingRow`'s insets; a key has no config layer, so no badge.
        .padding(.horizontal, Space.l)
        .padding(.vertical, Space.ml)
      }
      ForEach(table.fields) { FieldRow(field: $0, send: send) }
    }
  }
}

/// One setting in the row its control takes.
private struct FieldRow: View {
  let field: SettingsScreen.Field
  let send: (SettingsScreenIntent) -> Void

  var body: some View {
    switch field.control {
    case .toggle(let isOn):
      SettingRow(source: field.source) {
        LabeledToggle(field.title, detail: field.detail, isOn: binding(isOn) { .bool($0) })
      }
    case .slider(let value, let range, let text):
      SettingRow(source: field.source) {
        LabeledSlider(
          field.title, value: binding(value) { .number($0) }, in: range, valueText: text)
      }
    case .choice(let selection, let options):
      SettingRow(field.title, detail: field.detail, source: field.source) {
        CoxSegmented(
          LocalizedStringKey(field.title), selection: binding(selection) { .text($0) },
          options: options, title: { Text($0) })
      }
    case .field(let text):
      SettingRow(field.title, detail: field.detail, source: field.source) {
        SettingField(text, prompt: field.title) { send(.set(key: field.id, .text($0))) }
      }
    case .json(let text):
      SettingRow(field.title, detail: field.detail, source: field.source) {
        SettingField(text, prompt: field.title) { _ in }.disabled(true)
      }
    }
  }

  private func binding<Value>(
    _ value: Value, _ edit: @escaping (Value) -> SettingsScreen.Edit
  ) -> Binding<Value> {
    Binding(get: { value }, set: { send(.set(key: field.id, edit($0))) })
  }
}

#Preview("models") {
  SettingsScreen(state: PreviewState.settingsModels) { _ in }
    .frame(width: Size.windowMinWidth, height: Size.windowMinHeight)
    .padding(Space.xxl)
    .background(PreviewBackdrop())
}
