// `ReviewPane` (DS§6.4 row `ReviewPane`; DT§5.4 Review, T37.28.2): the split that replaces the
// transcript column — the changed files grouped by the turn that changed each last, Review's
// rewind timeline under them, and the open file's diff as `DiffHunkView`s. Separate from
// `ChangesTab`, which only names the files, because this is where a file's diff is read.

import SwiftUI

/// The list at the inspector's width, a hairline, then the diff filling the rest.
struct ReviewPane: View {
  /// The files one turn changed last.
  struct Turn: Equatable, Sendable {
    /// `Turn 2`.
    var title: String
    var files: [ChangedFileRow.File]
  }

  /// What the pane shows, formatted by the core.
  struct State: Equatable, Sendable {
    /// Oldest first.
    var turns: [Turn] = []
    var timeline = RewindTimeline.State()
    /// The path of the open file; its row is lifted.
    var selection: String?
    /// Its net diff (A101); empty when nothing is left to show.
    var hunks: [ToolCard.Hunk] = []
  }

  /// What the pane asks the app to do.
  enum Intent: Equatable, Sendable {
    /// Open this file's diff.
    case open(path: String)
    /// What the timeline asked.
    case timeline(RewindTimeline.Intent)
  }

  let state: State
  let send: @MainActor (Intent) -> Void

  var body: some View {
    HStack(spacing: 0) {
      ScrollView { files }
        .frame(width: Size.inspectorWidth)
        .hairline(.trailing)
      diff.frame(maxWidth: .infinity, maxHeight: .infinity)
    }
  }

  private var files: some View {
    VStack(alignment: .leading, spacing: Space.xl) {
      ForEach(state.turns, id: \.title) { turn in
        InspectorSection(turn.title) {
          ForEach(turn.files, id: \.path) { file in
            ChangedFileRow(file, isSelected: file.path == state.selection)
              .onTapGesture { send(.open(path: file.path)) }
              .accessibilityAction { send(.open(path: file.path)) }
          }
        }
      }
      RewindTimeline(state: state.timeline) { send(.timeline($0)) }
    }
    .padding(.horizontal, Space.xl)
    .padding(.vertical, Space.l)
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  @ViewBuilder private var diff: some View {
    if state.hunks.isEmpty {
      Text(state.selection == nil ? "No changes to review" : "No difference left on disk")
        .textStyle(.caption)
        .foregroundStyle(Color(.textSecondary))
    } else {
      let shape = RoundedRectangle(cornerRadius: Radius.l, style: .continuous)
      ScrollView {
        VStack(alignment: .leading, spacing: 0) {
          ForEach(state.hunks.indices, id: \.self) {
            DiffHunkView(header: state.hunks[$0].header, lines: state.hunks[$0].lines)
          }
        }
        .clipShape(shape)
        .hairline(in: shape)
        .padding(Space.l)
      }
    }
  }
}

#Preview("review") { PreviewMatrix { ReviewPaneSample(state: PreviewState.review) } }
#Preview("nothing left") {
  PreviewMatrix { ReviewPaneSample(state: PreviewState.reviewNothingLeft) }
}
#Preview("empty") { PreviewMatrix { ReviewPaneSample(state: .init()) } }
