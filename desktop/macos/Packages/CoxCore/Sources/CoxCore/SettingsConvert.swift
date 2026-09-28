// The Settings records from cox-ffi into CoxClient's (DT§5.7), apart from
// `Convert.swift`'s timeline so each file stays one concern. Field for
// field; nothing is decided here.

import CoxClient
import CoxFFIBindings

extension CoxClient.SettingsView {
  init(_ view: CoxFFIBindings.SettingsView) {
    self.init(
      settings: view.settings.map { CoxClient.Setting($0) }, userFile: view.userFile,
      projectFile: view.projectFile)
  }
}

extension CoxClient.Setting {
  init(_ setting: CoxFFIBindings.Setting) {
    self.init(
      key: setting.key, value: setting.value, layer: .init(setting.layer),
      editable: setting.editable, kind: .init(setting.kind), description: setting.description)
  }
}

extension CoxClient.Layer {
  init(_ layer: CoxFFIBindings.Layer) {
    switch layer {
    case .default: self = .default
    case .user: self = .user
    case .project: self = .project
    case .claudeSettings: self = .claudeSettings
    case .env: self = .env
    case .flag: self = .flag
    }
  }
}

extension CoxClient.SettingKind {
  init(_ kind: CoxFFIBindings.SettingKind) {
    switch kind {
    case .toggle: self = .toggle
    case .integer(let min, let max): self = .integer(min: min, max: max)
    case .number(let min, let max): self = .number(min: min, max: max)
    case .text: self = .text
    case .choice(let options): self = .choice(options: options)
    case .list: self = .list
    case .other: self = .other
    }
  }
}
