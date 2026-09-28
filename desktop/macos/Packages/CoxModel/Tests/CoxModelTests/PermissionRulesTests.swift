// The Permissions page through SettingsStore (T37.45.3) over the fixture client: a rule Rust
// refuses shows its message and changes nothing, a valid one lands in the user layer, a revoke
// drops the grant, and the rule lists leave the generic table rows to the rules box.

import CoxClient
import Testing

@testable import CoxModel

@MainActor
@Suite struct PermissionRulesTests {
  static let grant = SessionGrant(
    session: "01J0000000000000000000000A", title: "Add retry jitter", tool: "bash",
    subject: "git push")

  static let view = SettingsView(
    settings: [
      Setting(
        key: "permissions.allow", value: "[\"Bash(cargo nextest:*)\"]", layer: .user,
        editable: true, kind: .list, description: ""),
      Setting(
        key: "permissions.mode", value: "\"default\"", layer: .default, editable: true,
        kind: .choice(options: ["default", "plan", "auto", "bypass"]), description: ""),
    ],
    userFile: "/u/config.toml",
    rules: [
      PermissionRule(kind: .deny, rule: "Read(~/.ssh/**)", layer: .default, editable: true),
      PermissionRule(kind: .allow, rule: "Bash(cargo nextest:*)", layer: .user, editable: true),
      PermissionRule(kind: .ask, rule: "Edit(**/*.lock)", layer: .project, editable: false),
    ],
    grants: [grant])

  func loaded(_ client: FixtureSettingsClient) async -> SettingsStore {
    let store = SettingsStore(client: client, secrets: MemorySecretStore(), cwd: "/p")
    await store.load()
    return store
  }

  @Test func aRuleRustRefusesShowsItsMessageAndChangesNothing() async {
    let message = "`Bash(git push` is not a rule: missing closing ')'"
    let client = FixtureSettingsClient(view: Self.view, ruleRefusal: message)
    let store = await loaded(client)

    await store.editRule(.deny, old: nil, new: "Bash(git push")

    #expect(store.ruleFailure == message)
    #expect(store.failure == nil)
    #expect(store.view?.rules == Self.view.rules)
    #expect(client.sent.isEmpty)
  }

  @Test func aValidRuleLandsInTheUserLayerAndClearsTheFailure() async {
    let client = FixtureSettingsClient(view: Self.view)
    let store = await loaded(client)
    await store.editRule(.ask, old: nil, new: "Bash(x")
    #expect(store.ruleFailure != nil, "a project's ask list is read-only")

    await store.editRule(.deny, old: nil, new: "Bash(git push:*)")

    #expect(store.ruleFailure == nil)
    #expect(client.sent == ["deny=>Bash(git push:*)"])
    let deny = store.view?.rules.filter { $0.kind == .deny }
    #expect(deny?.map(\.rule) == ["Read(~/.ssh/**)", "Bash(git push:*)"])
    #expect(deny?.allSatisfy { $0.layer == .user } == true)
  }

  @Test func aRevokedGrantLeavesTheList() async {
    let client = FixtureSettingsClient(view: Self.view)
    let store = await loaded(client)

    await store.revoke(Self.grant)

    #expect(store.view?.grants.isEmpty == true)
    #expect(client.sent == ["revoke=bash git push"])
  }

  @Test func theRuleListsAreNoTableRowsAndTheModeStaysSegmented() async {
    let store = await loaded(FixtureSettingsClient(view: Self.view))
    let section = try? #require(store.sections.first { $0.group == .permissions })
    let fields = section.map { store.tables(in: $0).flatMap(\.fields) } ?? []

    #expect(fields.map(\.id) == ["permissions.mode"])
    let modes = ["default", "plan", "auto", "bypass"]
    #expect(fields.first?.control == .choice("default", options: modes))
  }
}
