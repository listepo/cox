// `TasksTab` (DS§6.4 row `TasksTab`, the mockup's Tasks `.ib`; DT§5.1 Tasks): the inspector's
// fourth tab — the subagents and background calls this session started, each with its tier,
// state and cost. Separate so the `Inspector` frame stays a slot and each tab is its own view,
// fed plain values the app copies from the core (T37.29.4).

import SwiftUI

/// One `InspectorSection` of task rows, newest last as the core started them; with none, one
/// quiet line. A row click asks the app to open that task's transcript.
struct TasksTab: View {
  /// What the tab lists, formatted by the core.
  struct State: Equatable, Sendable {
    var items: [Item] = []
  }

  /// A subagent or a background call.
  struct Item: Equatable, Sendable {
    /// The core's id for it, which opening its transcript names.
    var id: String
    /// `reviewer`, `bash: cargo nextest run`.
    var label: String
    /// The model tier it runs on, `cheap`.
    var tier: String
    var state: ToolHeader.State
    /// What it cost, `$0.03`; `nil` while it runs.
    var cost: String?
  }

  /// What the tab asks the app to do.
  enum Intent: Equatable, Sendable {
    /// Open the transcript of the task with this id.
    case open(task: String)
  }

  let state: State
  let send: @MainActor (Intent) -> Void

  var body: some View {
    if state.items.isEmpty {
      Text("No tasks yet")
        .textStyle(.caption)
        .foregroundStyle(Color(.textSecondary))
    } else {
      InspectorSection(state.title) {
        ForEach(state.items, id: \.id) { item in
          let open = Self.openAction(item.id, send: send)
          TaskItemRow(item: item, actions: [open])
            .onTapGesture { open.perform() }
            .accessibilityAction { open.perform() }
        }
      }
    }
  }
}

extension TasksTab.State {
  /// `Subagents & background · 3`.
  var title: String { "Subagents & background · \(items.count)" }
}

extension TasksTab {
  /// A row's action, which a click on the row performs too: open the task's transcript.
  nonisolated static func openAction(
    _ task: String, send: @escaping @MainActor (Intent) -> Void
  ) -> RowAction {
    RowAction(title: "Open transcript", symbol: "text.bubble") { send(.open(task: task)) }
  }
}

/// An `InspectorRow` with the transcript's agent glyph (DS§3.7 `person.2`, as the task's
/// ToolCard shows it): the label, the tier as a Badge, the cost in `text.secondary`, then the
/// ToolHeader's spinner, check or cross.
private struct TaskItemRow: View {
  let item: TasksTab.Item
  let actions: [RowAction]

  var body: some View {
    InspectorRow(symbol: "person.2", isSelected: false, actions: actions) {
      Text(item.label).frame(maxWidth: .infinity, alignment: .leading)
      Badge(item.tier)
      if let cost = item.cost {
        Text(cost)
          .textStyle(.footnote, tabularDigits: true)
          .foregroundStyle(Color(.textSecondary))
      }
      ToolHeaderStatus(state: item.state, duration: nil)
    }
  }
}

#Preview("tasks") { PreviewMatrix { TasksInspectorSample(state: PreviewState.tasks) } }
#Preview("empty") { PreviewMatrix { TasksInspectorSample(state: .init()) } }
