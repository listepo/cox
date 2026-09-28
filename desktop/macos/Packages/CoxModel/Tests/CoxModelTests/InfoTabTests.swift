// The Info tab's state from cox-app's `Info` (T37.29.5): the session's facts and the config
// layers fill the tab, read through SessionStore from the fixture session.

import CoxClient
import Testing

@testable import CoxModel

private let info = Info(
  session: "01J9ZK4Q", cwd: "/Users/me/GitHub/cox",
  worktree: Linked(path: "/Users/me/GitHub/_worktrees/cox-t1", branch: "t1", bytes: 0),
  config: [
    ConfigSource(layer: .default, keys: 112),
    ConfigSource(layer: .user, file: "/Users/me/.cox/config.toml", keys: 1),
    ConfigSource(layer: .env, keys: 2),
  ],
  rollout: "/Users/me/.cox/sessions/01J9ZK4Q.jsonl")

@MainActor
@Test func theStoreFillsTheInfoTabFromTheSession() async throws {
  let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []), info: info)
  let tab = try await SessionStore(session: session).infoTab()
  #expect(tab == InfoTabState(info))
}

@Test func theMappingShortensHomeAndPutsEachFileUnderItsLayer() {
  let tab = InfoTabState(info, home: "/Users/me")
  #expect(
    tab.session == [
      .init(label: "Session", values: ["01J9ZK4Q"]),
      .init(label: "Folder", values: ["~/GitHub/cox"]),
      .init(label: "Worktree", values: ["~/GitHub/_worktrees/cox-t1"]),
      .init(label: "Branch", values: ["t1"], isDetail: true),
      .init(label: "Rollout", values: ["~/.cox/sessions/01J9ZK4Q.jsonl"]),
    ])
  #expect(
    tab.config == [
      .init(label: "default", values: ["112 keys"]),
      .init(label: "user", values: ["1 key"]),
      .init(label: "~/.cox/config.toml", values: [], isDetail: true),
      .init(label: "env", values: ["2 keys"]),
    ])
}

@Test func outsideAWorktreeAndHomeThePathsStayWhole() {
  let info = Info(session: "s", cwd: "/Users/meta/x", rollout: "/r.jsonl")
  let tab = InfoTabState(info, home: "/Users/me")
  #expect(tab.session.map(\.label) == ["Session", "Folder", "Rollout"])
  #expect(tab.session[1].values == ["/Users/meta/x"])
  #expect(tab.config.isEmpty)
}
