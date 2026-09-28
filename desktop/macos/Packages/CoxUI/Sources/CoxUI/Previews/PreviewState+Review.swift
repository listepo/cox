// `PreviewState` fixtures for Review (T37.28.2): the Changes tab's files under two turns, the
// first open with the mockup's hunk, and the same after a code-only rewind left nothing on disk.
// Separate so organisms built in parallel add their fixtures without editing one file.

import SwiftUI

extension PreviewState {
  /// Two turns of files, the first file open.
  static let review = ReviewPane.State(
    turns: [
      .init(title: "Turn 1", files: Array(changedFiles.prefix(2))),
      .init(title: "Turn 2", files: [changedFiles[2], deletedFile]),
    ],
    timeline: .init(checkpoints: checkpoints),
    selection: changedFiles[0].path,
    hunks: [ToolCard.Hunk(header: hunkHeader, lines: hunk)])

  /// The first file open after a code-only rewind: its diff nets to nothing.
  static let reviewNothingLeft = ReviewPane.State(
    turns: review.turns, timeline: review.timeline, selection: review.selection)
}

/// The pane in the transcript column's place inside the window, as `MainScreen` nests it, at the
/// window's smallest size less the sidebar.
struct ReviewPaneSample: View {
  let state: ReviewPane.State

  var body: some View {
    ShellPane(.window) {
      ShellPane(.column) { ReviewPane(state: state) { _ in } }
        .padding(Size.paneGap)
    }
    .frame(width: Size.windowMinWidth - Size.sidebarWidth, height: Size.windowMinHeight)
  }
}
