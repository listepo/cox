// The toolbar's model popover (T37.22.6): each tier's models under its name with the running one
// marked, each by its short name (T37.22.7), a model listed once across tiers, and a pick sends
// the TUI's `/model <tier> <id>` switch.

import CoxClient
import Testing

@testable import CoxModel

private let catalog = [
  ModelChoice(
    tier: .code, provider: "anthropic", id: "claude-sonnet-5", displayName: "Claude Sonnet 5",
    efforts: [.low, .high]),
  ModelChoice(tier: .code, provider: "anthropic", id: "claude-haiku-4-5"),
  ModelChoice(tier: .think, provider: "anthropic", id: "claude-fable-5-1"),
  ModelChoice(tier: .think, provider: "anthropic", id: "claude-sonnet-5"),
  ModelChoice(tier: .cheap, provider: "anthropic", id: "claude-haiku-4-5"),
]

@Test func eachTierListsItsModelsOnceWithTheRunningOneMarked() {
  let menu = ModelMenu(choices: catalog, status: Status(model: "claude-sonnet-5", effort: .high))
  #expect(menu.sections.map(\.title) == ["Code", "Think"])
  #expect(menu.sections[0].rows.map(\.model) == ["claude-sonnet-5", "claude-haiku-4-5"])
  // The catalog's name without its vendor prefix, else the id (A111).
  #expect(menu.sections[0].rows.map(\.name) == ["Sonnet 5", "claude-haiku-4-5"])
  #expect(menu.sections[0].rows.map(\.detail) == ["low · high", ""])
  #expect(menu.sections[0].rows.map(\.isSelected) == [true, false])
  #expect(menu.sections[1].rows.map(\.model) == ["claude-fable-5-1"])
}

@Test func aPickSwitchesTheRowsTierToItsModel() {
  let menu = ModelMenu(choices: catalog, status: Status())
  let fable = menu.sections[1].rows[0]
  #expect(menu.pick(fable.id) == .switchModel(tier: .think, model: "claude-fable-5-1"))
  #expect(menu.pick("code/nope") == nil)
}
