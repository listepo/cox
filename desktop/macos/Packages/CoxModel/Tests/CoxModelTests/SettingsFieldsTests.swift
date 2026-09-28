// The Settings screen's fields over the fixture client (T37.30.1): a group's page by table,
// the project's value read-only with the file that sets it, each kind's control, and a user
// value edited on the screen round-tripping through the client. Secrets stay in memory (A49).

import CoxClient
import Testing

@testable import CoxModel

@MainActor
func tables(_ store: SettingsStore, _ group: SettingsGroup) throws -> [SettingsTable] {
  store.tables(in: try #require(store.sections.first { $0.group == group }))
}

@MainActor
@Test func aProjectValueIsReadOnlyAndNamesTheProjectFile() async throws {
  let (store, _) = await loadedStore()
  let models = try tables(store, .models)
  #expect(models.map(\.name) == ["providers.anthropic", "tiers.code"])
  #expect(models.map(\.provider) == ["anthropic", nil])
  let model = try #require(models.last?.fields.first)
  #expect(model.title == "Model")
  #expect(model.detail == "Set in /project/.cox/config.toml")
  #expect(model.control == .field("project-model"))
  #expect(model.setting.layer == .project && !model.setting.editable)
  let url = try #require(models.first?.fields.first)
  #expect(url.title == "Base url" && url.detail == nil)
}

@MainActor
@Test func eachKindGetsItsControl() async throws {
  let (store, _) = await loadedStore()
  let appearance = try #require(try tables(store, .appearance).first)
  #expect(appearance.name == "desktop.appearance")
  #expect(
    appearance.fields.map(\.control) == [
      .choice("glossy", options: ["frosted", "glossy", "solid"]),
      .slider(0.42, range: 0...1, text: "0.42"),
    ])
  let budget = try #require(try tables(store, .budget).first?.fields.first)
  #expect(budget.control == .field("5.0"))
  #expect(budget.detail == "Session spend cap, in USD.")
}

@MainActor
@Test func editingAUserValueRoundTripsThroughTheFixtureClient() async throws {
  let (store, client) = await loadedStore()
  await store.edit("desktop.appearance.material", .text("solid"))
  await store.edit("budget.session_usd", .text(" 7.5 "))
  await store.edit("budget.session_usd", .text("lots"))
  #expect(
    client.sent == [
      "desktop.appearance.material=\"solid\"", "budget.session_usd=7.5",
      "budget.session_usd=\"lots\"",
    ])
  await store.edit("budget.session_usd", .text("7.5"))
  await store.load()
  let material = try #require(try tables(store, .appearance).first?.fields.first)
  #expect(material.control == .choice("solid", options: ["frosted", "glossy", "solid"]))
  #expect(material.setting.layer == .user)
  let budget = try #require(try tables(store, .budget).first?.fields.first)
  #expect(budget.control == .field("7.5") && budget.setting.layer == .user)
}
