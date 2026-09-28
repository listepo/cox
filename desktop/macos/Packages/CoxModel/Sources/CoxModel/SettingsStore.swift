// The Settings window's state (DT§5.7): the view Rust built from the schema
// and the config layers, grouped for the sidebar, plus provider keys through
// a `SecretStore`. Every decision — the layer, the control, whether a field
// is read-only, whether a value loads — already came from Rust; this store
// sends edits and keeps the answer.

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
  /// The project whose layer applies.
  public let cwd: String
  @ObservationIgnored private let client: any SettingsClient
  @ObservationIgnored private let secrets: any SecretStore

  public init(client: any SettingsClient, secrets: any SecretStore, cwd: String) {
    (self.client, self.secrets, self.cwd) = (client, secrets, cwd)
  }

  public func load() async {
    await attempt { try await $0.client.settings(cwd: $0.cwd) }
  }

  public func set(_ key: String, to value: SettingValue) async {
    await attempt { try await $0.client.setSetting(cwd: $0.cwd, key: key, json: try value.json()) }
  }

  /// Non-empty groups in DT§5.7's order, keys sorted within each.
  public var sections: [SettingsSection] {
    let rows = Dictionary(grouping: view?.settings ?? []) { SettingsGroup(key: $0.key) }
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

  public func hasKey(for section: String) -> Bool {
    ((try? secrets.secret(for: section)) ?? nil) != nil
  }

  public func storeKey(_ secret: String, for section: String) throws {
    let secret = secret.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !secret.isEmpty else { throw KeyError.empty }
    guard providers.contains(section) else { throw KeyError.unknownProvider(section) }
    try secrets.store(secret, for: section)
  }

  public func removeKey(for section: String) throws {
    try secrets.remove(for: section)
  }

  private func attempt(_ fetch: (SettingsStore) async throws -> SettingsView) async {
    do {
      view = try await fetch(self)
      failure = nil
    } catch {
      failure = String(describing: error)
    }
  }
}
