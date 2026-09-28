// The Tasks tab's check (T37.29.4, DT§5.1, DS§6.4): the inspector on its Tasks tab per
// light/dark × Solid/Frosted cell, and empty; the open intent a row click sends; its header.

import Testing

@testable import CoxUI

@MainActor
@Suite struct TasksTabSnapshotTests {
  @Test(arguments: Variant.all) func tasksTab(_ variant: Variant) throws {
    try assertCoxSnapshot(
      TasksInspectorSample(state: PreviewState.tasks), variant, named: variant.name)
  }

  @Test func emptyTasksTab() throws {
    try assertCoxSnapshot(
      TasksInspectorSample(state: .init()), Variant.all[0], named: Variant.all[0].name)
  }
}

@MainActor
@Suite struct TasksTabTests {
  /// Records what the tab sends.
  final class Log {
    var intents: [TasksTab.Intent] = []
  }

  @Test func aRowClickOpensTheTaskTranscriptByItsId() {
    let log = Log()
    let open = TasksTab.openAction("task-2") { log.intents.append($0) }
    open.perform()
    #expect(open.title == "Open transcript")
    #expect(log.intents == [.open(task: "task-2")])
  }

  @Test func theHeaderCountsTheTasks() {
    #expect(PreviewState.tasks.title == "Subagents & background · 3")
  }
}
