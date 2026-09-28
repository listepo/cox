// Project values the guard list threw out (T37.30.4, DT§5.7), on the page their key belongs to:
// the key, why the project may not set it, and what it asked for against what holds. Here, not
// in CoxUI, because these decide what the screen shows (DS§1); the app copies them into CoxUI's
// `SettingsScreen.DroppedValue` field for field.

import CoxClient

public struct DroppedRow: Identifiable, Equatable, Sendable {
  /// Dotted, as the project file names it.
  public let key: String
  /// Why the project may not set it, from Rust's guard list.
  public let reason: String
  /// `999 → 5`: what the project set, then what holds.
  public let change: String
  public var id: String { key }
}

extension SettingsStore {
  /// The values the project set under `group` that the guard list dropped, in Rust's order.
  public func dropped(in group: SettingsGroup) -> [DroppedRow] {
    (view?.dropped ?? []).filter { SettingsGroup(key: $0.key) == group }.map {
      DroppedRow(key: $0.key, reason: $0.reason, change: "\($0.value) → \($0.kept)")
    }
  }
}
