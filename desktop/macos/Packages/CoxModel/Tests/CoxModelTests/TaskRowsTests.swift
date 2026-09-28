// The Tasks tab's rows through SessionStore (T37.29.4): task blocks become rows in timeline
// order with their state and cost, and every other block is left out.

import CoxClient
import Testing

@testable import CoxModel

@MainActor
@Suite struct TaskRowsTests {
  @Test func taskBlocksBecomeRowsWithStateAndCostInTimelineOrder() {
    let blocks = [
      Block(
        id: "task:a", turn: 1,
        kind: .task(
          task: "a", label: "reviewer", tier: .cheap, done: false, costUsd: 0, exitCode: nil)),
      Block(id: "notice:1", turn: 1, kind: .notice(level: .info, text: "hi")),
      Block(
        id: "task:b", turn: 1,
        kind: .task(
          task: "b", label: "test-writer", tier: .code, done: true, costUsd: 0.071, exitCode: nil)),
      Block(
        id: "task:c", turn: 2,
        kind: .task(
          task: "c", label: "bash: cargo test", tier: .cheap, done: true, costUsd: 0,
          exitCode: 101)),
    ]
    let store = SessionStore(session: FixtureSession(fixture: Fixture(batches: [], snapshot: [])))
    store.apply([.reset(blocks: blocks)])

    #expect(
      store.tasks == [
        TaskRow(id: "a", label: "reviewer", tier: "cheap", state: .running, cost: nil),
        TaskRow(id: "b", label: "test-writer", tier: "code", state: .succeeded, cost: "$0.07"),
        TaskRow(id: "c", label: "bash: cargo test", tier: "cheap", state: .failed, cost: "$0.00"),
      ])
  }
}
