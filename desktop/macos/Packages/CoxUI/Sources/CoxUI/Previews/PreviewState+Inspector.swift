// `PreviewState` fixtures for the inspector's rows (T37.21.9): the Changes tab's files and
// checkpoints as the mockup's inspector shows them. Separate from `PreviewState.swift` so
// molecules built in parallel add their fixtures without editing one file.

import SwiftUI

extension PreviewState {
  /// The session's changed files; the first is the open one.
  static let changedFiles: [ChangedFileRow.File] = [
    .init(path: "cox-provider-http/src/retry.rs", change: .edited, added: 18, removed: 4),
    .init(path: "cox-provider-http/Cargo.toml", change: .edited, added: 1, removed: 0),
    .init(path: "cox-provider-http/tests/backoff.rs", change: .created, added: 42, removed: 0),
  ]

  static let checkpoints: [CheckpointRow.Checkpoint] = [
    .init(label: "Turn 1 · before edit retry.rs", time: "14:02"),
    .init(label: "Turn 1 · before write backoff.rs", time: "14:03"),
  ]

  /// What you can do to a changed file.
  static let fileActions = [
    RowAction(title: "Review", symbol: "eye") {},
    RowAction(title: "Revert", symbol: "arrow.uturn.backward") {},
  ]

  static let checkpointActions = [RowAction(title: "Rewind", symbol: "arrow.uturn.backward") {}]
}

/// The first changed file at the inspector's width, selected or not.
struct ChangedFileSample: View {
  let isSelected: Bool

  var body: some View {
    ChangedFileRow(
      PreviewState.changedFiles[0], isSelected: isSelected, actions: PreviewState.fileActions
    )
    .frame(width: Size.inspectorWidth)
  }
}

/// Every changed file stacked at the inspector's width, none selected.
struct ChangedFileList: View {
  var body: some View {
    VStack(spacing: 0) {
      ForEach(PreviewState.changedFiles, id: \.path) {
        ChangedFileRow($0, actions: PreviewState.fileActions)
      }
    }
    .frame(width: Size.inspectorWidth)
  }
}

/// The first checkpoint at the inspector's width, selected or not.
struct CheckpointSample: View {
  let isSelected: Bool

  var body: some View {
    CheckpointRow(
      PreviewState.checkpoints[0], isSelected: isSelected,
      actions: PreviewState.checkpointActions
    )
    .frame(width: Size.inspectorWidth)
  }
}

/// Every checkpoint stacked at the inspector's width, none selected.
struct CheckpointList: View {
  var body: some View {
    VStack(spacing: 0) {
      ForEach(PreviewState.checkpoints, id: \.label) {
        CheckpointRow($0, actions: PreviewState.checkpointActions)
      }
    }
    .frame(width: Size.inspectorWidth)
  }
}
