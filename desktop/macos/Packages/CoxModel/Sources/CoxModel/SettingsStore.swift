// The Settings window's state (DT§5.7): the view Rust built from the schema
// and the config layers, grouped for the sidebar, plus provider keys through
// a `SecretStore` and each tier's models from the core's catalog. Every
// decision — the layer, the control, whether a field is read-only, whether a
// value loads — already came from Rust; this store sends edits and keeps the
// answer.

import CoxClient
import Foundation
import Observation

/// The sidebar groups DT§5.7 names, by a key's top-level table.
public enum SettingsGroup: String, CaseIterable, Sendable {
  case general, models, permissions, sandbox, budget, mcp, plugins, appearance, advanced

  public init(key: String) {
    switch key.prefix(while: { $0 != "." }) {
    case "core": self = .general
    case "tiers", "jobs", "providers": self = .models
    case "permissions": self = .permissions
    case "sandbox": self = .sandbox
    case "budget": self = .budget
    case "mcp": self = .mcp
    case "plugins": self = .plugins
    case "desktop": self = .appearance
    default: self = .advanced
    }
  }
}

public struct SettingsSection: Identifiable, Equatable, Sendable {
  public let group: SettingsGroup
  public let settings: [Setting]
  public var id: SettingsGroup { group }
}

/// A typed edit, sent to Rust as JSON.
public enum SettingValue: Equatable, Sendable, Encodable {
  case bool(Bool)
  case integer(Int64)
  case number(Double)
  case text(String)
  case list([String])

  public func encode(to encoder: any Encoder) throws {
    var container = encoder.singleValueContainer()
    switch self {
    case .bool(let value): try container.encode(value)
    case .integer(let value): try container.encode(value)
    case .number(let value): try container.encode(value)
    case .text(let value): try container.encode(value)
    case .list(let value): try container.encode(value)
    }
  }

  /// Throws for a number JSON cannot carry (NaN, infinity).
  func json() throws -> String {
    guard let text = String(bytes: try JSONEncoder().encode(self), encoding: .utf8) else {
      throw EncodingError.invalidValue(self, .init(codingPath: [], debugDescription: "not UTF-8"))
    }
    return text
  }
}

public enum KeyError: Error, Equatable {
  case empty
  /// Not a `[providers.<name>]` table in the current view.
  case unknownProvider(String)
}

@Observable
@MainActor
public final class SettingsStore {
  public private(set) var view: SettingsView?
  /// Why the last load or edit failed; the next success clears it.
  public private(set) var failure: String?
  /// Why Rust refused the last rule edit or revoke (T37.45.3): the rule grammar's message, or a
  /// list a layer above the user file sets. The next one that succeeds clears it.
  public private(set) var ruleFailure: String?
  /// The providers whose key the `SecretStore` holds, read again after each load, store and
  /// removal so the Settings rows that show it redraw.
  public private(set) var storedKeys: Set<String> = []
  /// Each tier's models as the core's catalog lists them, read again after each load and edit so
  /// a tier's picker offers what its provider serves.
  public private(set) var models: [ModelChoice] = []
  /// The sidebar's search: `sections` keeps only the settings whose label or dotted key holds
  /// it, so the pages and their boxes shrink to the matches. Empty keeps every setting.
  public var filter = ""
  /// The project whose layer applies.
  public let cwd: String
  @ObservationIgnored private let client: any SettingsClient
  @ObservationIgnored private let secrets: any SecretStore
  @ObservationIgnored private let catalog: (any ModelsClient)?

  public init(
    client: any SettingsClient, secrets: any SecretStore, catalog: (any ModelsClient)? = nil,
    cwd: String
  ) {
    (self.client, self.secrets, self.catalog, self.cwd) = (client, secrets, catalog, cwd)
  }

  public func load() async {
    await attempt { try await $0.client.settings(cwd: $0.cwd) }
  }

  public func set(_ key: String, to value: SettingValue) async {
    await attempt { try await $0.client.setSetting(cwd: $0.cwd, key: key, json: try value.json()) }
  }

  /// Logs in to (`login`) or out of an MCP server, then reads its status back.
  public func setLogin(_ server: String, _ login: Bool) async {
    await attempt {
      try await $0.client.mcpLogin(cwd: $0.cwd, server: server, login: login)
      return try await $0.client.settings(cwd: $0.cwd)
    }
  }

  /// Adds (`old` nil), replaces or removes (`new` nil) one permission rule, in the user file only.
  public func editRule(_ kind: RuleKind, old: String?, new: String?) async {
    await attempt(\.ruleFailure) {
      try await $0.client.setPermissionRule(cwd: $0.cwd, kind: kind, old: old, new: new)
    }
  }

  /// Revokes an "allow for session" grant through its session's core.
  public func revoke(_ grant: SessionGrant) async {
    await attempt(\.ruleFailure) { try await $0.client.revokeGrant(cwd: $0.cwd, grant: grant) }
  }

  /// Non-empty groups in DT§5.7's order, keys sorted within each, narrowed to `filter`.
  public var sections: [SettingsSection] {
    let query = filter.trimmingCharacters(in: .whitespaces)
    let shown = (view?.settings ?? []).filter {
      query.isEmpty || $0.key.localizedStandardContains(query)
        || Self.title(of: $0.key).localizedStandardContains(query)
    }
    let rows = Dictionary(grouping: shown) { SettingsGroup(key: $0.key) }
    return SettingsGroup.allCases.compactMap { group in
      rows[group].map { SettingsSection(group: group, settings: $0) }
    }
  }

  /// The provider sections a key can be stored for, sorted.
  public var providers: [String] {
    let names = (view?.settings ?? []).compactMap { setting -> String? in
      let parts = setting.key.split(separator: ".")
      return parts.count > 2 && parts[0] == "providers" ? String(parts[1]) : nil
    }
    return Set(names).sorted()
  }

  public func hasKey(for section: String) -> Bool { storedKeys.contains(section) }

  public func storeKey(_ secret: String, for section: String) throws {
    let secret = secret.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !secret.isEmpty else { throw KeyError.empty }
    guard providers.contains(section) else { throw KeyError.unknownProvider(section) }
    try secrets.store(secret, for: section)
    readKeys()
  }

  public func removeKey(for section: String) throws {
    try secrets.remove(for: section)
    readKeys()
  }

  private func readKeys() {
    storedKeys = Set(providers.filter { ((try? secrets.secret(for: $0)) ?? nil) != nil })
  }

  private func attempt(
    _ failed: ReferenceWritableKeyPath<SettingsStore, String?> = \.failure,
    _ fetch: (SettingsStore) async throws -> SettingsView
  ) async {
    do {
      view = try await fetch(self)
      self[keyPath: failed] = nil
      readKeys()
      models = (try? catalog?.models(cwd: cwd)) ?? []
    } catch {
      self[keyPath: failed] = String(describing: error)
    }
  }
}
