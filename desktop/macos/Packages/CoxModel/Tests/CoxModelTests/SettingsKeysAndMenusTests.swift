// Mockup 18's provider keys and pop-ups (T37.45.2, A120): a key is added and changed in the
// in-memory `SecretStore` alone — never sent to the settings file, never in the view or an error
// — and an enum with more options than segments, or a tier's model from the catalog, takes a
// pop-up whose pick writes through the settings set path. No real Keychain (A49).

import CoxClient
import Testing

@testable import CoxModel

/// A Keychain that refuses every write, as a locked one does.
private struct RefusingSecrets: SecretStore {
  struct Locked: Error {}
  func secret(for section: String) throws -> String? { nil }
  func store(_ secret: String, for section: String) throws { throw Locked() }
  func remove(for section: String) throws { throw Locked() }
}

private let tierView = SettingsView(
  settings: [
    Setting(
      key: "tiers.cheap.model", value: "\"my-finetune\"", layer: .user, editable: true,
      kind: .text, description: ""),
    Setting(
      key: "tiers.code.effort", value: "\"high\"", layer: .user, editable: true,
      kind: .choice(options: ["low", "medium", "high", "xhigh"]), description: ""),
    Setting(
      key: "tiers.code.model", value: "\"claude-sonnet-5\"", layer: .default, editable: true,
      kind: .text, description: ""),
    Setting(
      key: "tiers.code.thinking", value: "\"off\"", layer: .default, editable: true,
      kind: .choice(options: ["off", "adaptive"]), description: ""),
  ],
  userFile: "/home/.cox/config.toml")

private let catalog = FixtureModels(models: [
  ModelChoice(tier: .cheap, provider: "anthropic", id: "claude-haiku-4-5"),
  ModelChoice(
    tier: .code, provider: "anthropic", id: "claude-sonnet-5", displayName: "Claude Sonnet 5"),
  ModelChoice(
    tier: .code, provider: "anthropic", id: "claude-opus-5-5", displayName: "Claude Opus 5.5"),
])

@MainActor
@Test func aKeyIsAddedThenChangedInTheSecretStoreAndNeverReachesTheSettingsFile() async throws {
  let secrets = MemorySecretStore()
  let (store, client) = await loadedStore(secrets: secrets)
  // Made up here, so a match anywhere below can only be this test's value leaking.
  let first = "fixture-first-\(UInt64.random(in: 1...UInt64.max))"
  let second = "fixture-second-\(UInt64.random(in: 1...UInt64.max))"

  try store.storeKey(first, for: "anthropic")
  #expect(try secrets.secret(for: "anthropic") == first)
  #expect(store.hasKey(for: "anthropic"))
  try store.storeKey(second, for: "anthropic")
  #expect(try secrets.secret(for: "anthropic") == second)

  #expect(client.sent.isEmpty)
  #expect(store.view == fixtureView)
  #expect(store.failure == nil)
  let shown = String(describing: store.view) + String(describing: store.storedKeys)
  #expect(!shown.contains(first) && !shown.contains(second))
}

@MainActor
@Test func aKeyTheSecretStoreRefusesIsNotEchoedInTheError() async throws {
  let (store, client) = await loadedStore(secrets: RefusingSecrets())
  let secret = "fixture-\(UInt64.random(in: 1...UInt64.max))"
  let error = #expect(throws: RefusingSecrets.Locked.self) {
    try store.storeKey(secret, for: "anthropic")
  }
  #expect(!String(describing: error).contains(secret))
  #expect(!store.hasKey(for: "anthropic"))
  #expect(client.sent.isEmpty && store.failure == nil)
}

@MainActor
@Test func anEnumPastTheSegmentLimitAndATiersModelArePopUps() async throws {
  let store = SettingsStore(
    client: FixtureSettingsClient(view: tierView), secrets: MemorySecretStore(),
    catalog: catalog, cwd: "/p")
  await store.load()
  let fields = try tables(store, .models).flatMap(\.fields)
  let control = { key in fields.first { $0.id == key }?.control }
  #expect(
    control("tiers.code.model")
      == .menu(
        "claude-sonnet-5",
        options: [
          .init(value: "claude-sonnet-5", title: "Sonnet 5"),
          .init(value: "claude-opus-5-5", title: "Opus 5.5"),
        ]))
  // A model the catalog does not list stays first, so the pop-up still shows it.
  #expect(
    control("tiers.cheap.model")
      == .menu(
        "my-finetune",
        options: [
          .init(value: "my-finetune", title: "my-finetune"),
          .init(value: "claude-haiku-4-5", title: "claude-haiku-4-5"),
        ]))
  #expect(
    control("tiers.code.effort")
      == .menu(
        "high", options: ["low", "medium", "high", "xhigh"].map { .init(value: $0, title: $0) }))
  #expect(control("tiers.code.thinking") == .choice("off", options: ["off", "adaptive"]))
}

@MainActor
@Test func aPopUpPickWritesThroughTheSettingsSetPath() async throws {
  let client = FixtureSettingsClient(view: tierView)
  let store = SettingsStore(
    client: client, secrets: MemorySecretStore(), catalog: catalog, cwd: "/p")
  await store.load()
  await store.edit("tiers.code.model", .text("claude-opus-5-5"))
  await store.edit("tiers.code.effort", .text("xhigh"))
  #expect(client.sent == [#"tiers.code.model="claude-opus-5-5""#, #"tiers.code.effort="xhigh""#])
  #expect(store.failure == nil)
}
