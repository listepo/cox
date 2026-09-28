// The Context tab's state from the token meter (T37.29.3.1): the recorded fixture's context split
// reaches the tab through SessionStore, a part of an unknown kind is left out, and "Compact now"
// sends one manual compaction; the cost by turn (T37.29.3.2) reaches it with the total last, and
// the project footnote (T37.29.3.3) stays when the session has not spent yet.

import CoxClient
import Foundation
import Testing

@testable import CoxModel

@MainActor
@Test func theFixturesContextSplitReachesTheContextTab() async throws {
  let url = try #require(fixtures.first { $0.lastPathComponent == "read-and-reply.json" })
  let session = try await FixtureCoreClient(fixture: try Fixture(contentsOf: url))
    .open(OpenSession(cwd: "/", theme: "base16-ocean.dark"))
  let store = SessionStore(session: session)
  #expect(store.contextTab == ContextTabState())

  await store.run()

  let tab = store.contextTab
  #expect(tab.split.context == "Context · 3.7k")
  #expect(tab.split.share == "0.4% of 1M")
  #expect(tab.split.free == "996.3k")
  #expect(tab.split.parts.map(\.kind) == ContextSplit.Kind.allCases)
  #expect(tab.split.parts.map(\.label) == ["System", "Tools", "Instructions", "History"])
  #expect(tab.split.parts.allSatisfy { $0.fraction > 0 })
  #expect(tab.cacheHit == "0% this turn")
}

@Test func aPartOfAnUnknownKindIsLeftOut() {
  var text = MeterText()
  text.contextParts = [
    ContextPart(kind: "system", label: "System", tokens: "7.6k", share: 0.1),
    ContextPart(kind: "memory", label: "Memory", tokens: "1k", share: 0.01),
  ]
  let system = ContextSplit.Part(kind: .system, label: "System", tokens: "7.6k", fraction: 0.1)
  #expect(ContextSplit(text).parts == [system])
}

@MainActor
@Test func compactNowSendsOneManualCompaction() async throws {
  let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []))
  try await SessionStore(session: session).compactNow()
  #expect(session.sent == [.compact(focus: nil)])
}

@MainActor
@Test func theCostHistoryIsTheTurnsThenTheSessionTotal() async throws {
  let costs = TurnCosts(
    columns: ["In", "Out", "Cache r/w", "$"],
    rows: [
      CostRow(label: "1 · code", values: ["31.4k", "2.2k", "28.0k/3.1k", "0.29"]),
      CostRow(label: "explore", values: ["9.8k", "600", "0/9.8k", "0.03"], detail: true),
    ],
    total: CostRow(label: "Session", values: ["41.2k", "2.8k", "28.0k/12.9k", "0.32"]),
    project: "Project cox today: $3.18 · this week: $21.40.")
  let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []), costs: costs)
  let history = try await SessionStore(session: session).costHistory()
  #expect(history.columns == costs.columns)
  #expect(history.rows.map(\.label) == ["1 · code", "explore", "Session"])
  #expect(history.rows.map(\.isDetail) == [false, true, false])
  #expect(history.rows.last?.values == ["41.2k", "2.8k", "28.0k/12.9k", "0.32"])
  #expect(history.footnote == "Project cox today: $3.18 · this week: $21.40.")
}

@Test func anEmptyLedgerHidesTheCostHistoryButKeepsTheProjectFootnote() {
  let empty = TurnCosts(columns: ["In"], total: CostRow(label: "Session"), project: "Project cox")
  var expected = CostHistoryState()
  expected.footnote = "Project cox"
  #expect(CostHistoryState(empty) == expected)
}
