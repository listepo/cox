// The toolbar's model popover (DT§5.1 "model chip", T37.22.6): each tier's models from the core's
// catalog under the tier's name, by id since the catalog names none, the one the session runs on
// marked. A pick is `/model <tier> <id>`, the TUI's switch. Here, not in CoxUI,
// because these decide what the menu offers (DS§1); the app copies it into `ModelPopover.State`.

import CoxClient

public struct ModelMenu: Equatable, Sendable {
  public struct Row: Identifiable, Equatable, Sendable {
    public var id: String { "\(tier.rawValue)/\(model)" }
    public let tier: Tier
    /// The id the switch sends and the row shows.
    public let model: String
    /// `low · high`: the efforts it takes; empty when it takes any.
    public let detail: String
    public let isSelected: Bool
  }

  public struct Section: Identifiable, Equatable, Sendable {
    public var id: String { title }
    /// `Code`, `Think`, `Cheap`.
    public let title: String
    public let rows: [Row]
  }

  public var sections: [Section] = []

  public init() {}

  /// A model a tier's section already lists is left out of a later tier's: with every tier on
  /// one provider, the menu is one list.
  public init(choices: [ModelChoice], status: Status) {
    var listed: Set<String> = []
    var order: [Tier] = []
    var rows: [Tier: [Row]] = [:]
    for choice in choices where listed.insert(choice.id).inserted {
      if rows[choice.tier] == nil { order.append(choice.tier) }
      rows[choice.tier, default: []].append(
        Row(
          tier: choice.tier, model: choice.id,
          detail: choice.efforts.map(\.rawValue).joined(separator: " · "),
          isSelected: choice.id == status.model))
    }
    sections = order.map { Section(title: $0.rawValue.capitalized, rows: rows[$0] ?? []) }
  }

  /// The switch a row's click sends; `nil` for a row the menu does not list.
  public func pick(_ row: Row.ID) -> Intent? {
    sections.lazy.flatMap(\.rows).first { $0.id == row }.map {
      .switchModel(tier: $0.tier, model: $0.model)
    }
  }
}
