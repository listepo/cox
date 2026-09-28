// The Settings window (DT§5.7, T37.30.1): `SettingsStore`'s pages, tables and fields copied into
// CoxUI's `SettingsScreenState` case for case, and the screen's intents sent back to the store.
// A slider's steps are coalesced into one write per rest; until Rust answers, the slider shows
// the value it was dragged to. Wiring only: what each field shows was decided in CoxModel.

import CoxClient
import CoxModel
import CoxUI
import SwiftUI

struct SettingsWindow: View {
  let model: AppModel
  @State private var page = SettingsPage.general
  @State private var sliderWrites = Coalescer()
  /// Slider values sent but not yet stored, by key.
  @State private var dragged: [String: Double] = [:]
  /// Why the last key could not be stored; shown until dismissed.
  @State private var refused: String?

  var body: some View {
    Group {
      if let settings = model.settings {
        SettingsScreen(state: state(settings), recorder: Hotkeys.recorder) {
          handle($0, settings)
        }
        .task {
          sliderWrites.onIdle = { dragged = [:] }
          await settings.load()
        }
      } else if case .failure(let error) = model.launch.live {
        Text(String(describing: error)).textSelection(.enabled).padding(Space.xxl)
      }
    }
    .frame(minWidth: Size.windowMinWidth, minHeight: Size.windowMinHeight)
    .alert(refused ?? "", isPresented: isRefused) {}
  }

  private var isRefused: Binding<Bool> {
    Binding(get: { refused != nil }, set: { if !$0 { refused = nil } })
  }

  private func state(_ settings: SettingsStore) -> SettingsScreenState {
    let section = settings.sections.first { $0.group.rawValue == page.rawValue }
    let tables = section.map { settings.tables(in: $0) } ?? []
    return SettingsScreenState(
      pages: settings.sections.compactMap { SettingsPage(rawValue: $0.group.rawValue) },
      selection: page,
      tables: tables.map { table in
        SettingsScreen.Table(
          id: table.name, fields: table.fields.map(field),
          key: table.provider.map { .init(provider: $0, isStored: settings.hasKey(for: $0)) })
      },
      userFile: settings.view?.userFile ?? "", projectFile: settings.view?.projectFile,
      logins: page == .mcp
        ? settings.logins.map { .init(id: $0.server, detail: $0.detail, action: action($0)) } : [],
      dropped: SettingsGroup(rawValue: page.rawValue).map { group in
        settings.dropped(in: group).map { .init(id: $0.key, reason: $0.reason, change: $0.change) }
      } ?? [],
      shortcuts: page == .general ? Hotkeys.shortcuts : [])
  }

  private func field(_ field: SettingsField) -> SettingsScreen.Field {
    let control: SettingsScreen.Control =
      switch field.control {
      case .toggle(let isOn): .toggle(isOn)
      case .slider(let value, let range, let text):
        dragged[field.id].map { .slider($0, range: range, text: $0.formatted()) }
          ?? .slider(value, range: range, text: text)
      case .choice(let value, let options): .choice(value, options: options)
      case .field(let text): .field(text)
      case .json(let text): .json(text)
      }
    return SettingsScreen.Field(
      id: field.id, title: field.title, detail: field.detail,
      source: Self.source(field.setting.layer), control: control)
  }

  private func handle(_ intent: SettingsScreenIntent, _ settings: SettingsStore) {
    switch intent {
    case .select(let selected): page = selected
    case .set(let key, .number(let value)):
      dragged[key] = value
      sliderWrites.submit(key) { await settings.edit(key, .number(value)) }
    case .set(let key, .bool(let isOn)): Task { await settings.edit(key, .bool(isOn)) }
    case .set(let key, .text(let text)): Task { await settings.edit(key, .text(text)) }
    case .storeKey(let provider, let secret):
      do {
        try settings.storeKey(secret, for: provider)
      } catch {
        refused = String(describing: error)
      }
    case .setLogin(let server, let login): Task { await settings.setLogin(server, login) }
    }
  }

  private func action(_ row: McpLoginRow) -> LoginAction? {
    row.action.map { $0 == .logIn ? .logIn : .logOut }
  }

  private static func source(_ layer: Layer) -> SettingSource {
    switch layer {
    case .default: .default
    case .user: .user
    case .project: .project
    case .claudeSettings: .claudeSettings
    case .env: .env
    case .flag: .flag
    }
  }
}
