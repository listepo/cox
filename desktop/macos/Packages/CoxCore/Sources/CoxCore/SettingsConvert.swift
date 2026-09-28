// The Settings records from cox-ffi into CoxClient's (DT§5.7), apart from
// `Convert.swift`'s timeline so each file stays one concern. Field for
// field; nothing is decided here.

import CoxClient
import CoxFFIBindings

extension CoxClient.SettingsView {
  init(_ view: CoxFFIBindings.SettingsView) {
    self.init(
      settings: view.settings.map { CoxClient.Setting($0) }, userFile: view.userFile,
      projectFile: view.projectFile, mcp: view.mcp.map { CoxClient.McpServer($0) },
      dropped: view.dropped.map { CoxClient.Dropped($0) })
  }
}

extension CoxClient.Dropped {
  init(_ dropped: CoxFFIBindings.Dropped) {
    self.init(key: dropped.key, value: dropped.value, kept: dropped.kept, reason: dropped.reason)
  }
}

extension CoxClient.McpServer {
  init(_ server: CoxFFIBindings.McpServer) {
    self.init(name: server.name, source: server.source, login: .init(server.login))
  }
}

extension CoxClient.McpLogin {
  init(_ login: CoxFFIBindings.McpLogin) {
    switch login {
    case .stdio: self = .stdio
    case .loggedOut: self = .loggedOut
    case .loggedIn(let expires): self = .loggedIn(expires: expires)
    case .expired: self = .expired
    case .unreadable(let error): self = .unreadable(error: error)
    }
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
