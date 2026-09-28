// `ReviewPane` (DS§6.4 row `ReviewPane`; DT§5.4 Review, T37.28.2): the split that replaces the
// transcript column — the changed files grouped by the turn that changed each last, Review's
// rewind timeline under them, and the open file's diff as `DiffHunkView`s. Separate from
// `ChangesTab`, which only names the files, because this is where a file's diff is read. A click
// on a line number starts a comment; the draft collects under the diff until "Send to agent"
// (T37.28.4). The draft is the app's; the typed text of the open comment is the pane's until saved.

import SwiftUI

/// The list at the inspector's width, a hairline, then the diff filling the rest.
public struct ReviewPane: View {
  /// The files one turn changed last.
  public struct Turn: Equatable, Sendable {
    /// `Turn 2`.
    public var title: String
    public var files: [ChangedFileRow.File]

    public init(title: String, files: [ChangedFileRow.File]) {
      (self.title, self.files) = (title, files)
    }
  }

  /// What the pane shows, formatted by the core.
  public struct State: Equatable, Sendable {
    /// Oldest first.
    public var turns: [Turn] = []
    public var timeline = RewindTimeline.State()
    /// The path of the open file; its row is lifted.
    public var selection: String?
    /// Its net diff (A101); empty when nothing is left to show.
    public var hunks: [ToolCard.Hunk] = []
    /// The draft's comments, oldest first (T37.28.4).
    public var comments: [Comment] = []
    /// The anchor of the line a click picked, while its comment is being typed.
    public var editing: String?

    public init(
      turns: [Turn] = [], timeline: RewindTimeline.State = .init(), selection: String? = nil,
      hunks: [ToolCard.Hunk] = [], comments: [Comment] = [], editing: String? = nil
    ) {
      (self.turns, self.timeline, self.selection) = (turns, timeline, selection)
      (self.hunks, self.comments, self.editing) = (hunks, comments, editing)
    }
  }

  /// One comment of the draft.
  public struct Comment: Equatable, Sendable {
    /// `src/retry.rs:42`.
    public var anchor: String
    public var text: String

    public init(anchor: String, text: String) { (self.anchor, self.text) = (anchor, text) }
  }

  /// What the pane asks the app to do.
  public enum Intent: Equatable, Sendable {
    /// Open this file's diff.
    case open(path: String)
    /// What the timeline asked.
    case timeline(RewindTimeline.Intent)
    /// Start a comment on this line, by index, of this hunk of the open diff.
    case comment(hunk: Int, line: Int)
    /// Add the open comment with this text; blank text drops it.
    case save(text: String)
    /// Drop the draft's comment at this index.
    case remove(Int)
    /// Post the draft to the agent as one message.
    case sendComments
  }

  let state: State
  let send: @MainActor (Intent) -> Void
  @State private var text = ""
  @FocusState private var typing: Bool

  public init(state: State, send: @escaping @MainActor (Intent) -> Void) {
    (self.state, self.send) = (state, send)
  }

  public var body: some View {
    HStack(spacing: 0) {
      ScrollView { files }
        .frame(width: Size.reviewFileListWidth)
        .hairline(.trailing)
      VStack(spacing: 0) {
        diff.frame(maxWidth: .infinity, maxHeight: .infinity)
        if !state.comments.isEmpty || state.editing != nil { draft.hairline(.top) }
      }
    }
    .onChange(of: state.editing) {
      text = ""
      typing = state.editing != nil
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
    .padding(.horizontal, Space.ml)
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
          ForEach(state.hunks.indices, id: \.self) { hunk in
            DiffHunkView(header: state.hunks[hunk].header, lines: state.hunks[hunk].lines) {
              send(.comment(hunk: hunk, line: $0))
            }
          }
        }
        .clipShape(shape)
        .hairline(in: shape)
        .padding(Space.l)
      }
    }
  }

  /// Each comment's anchor over its text, the open one's field, then the count and Send.
  private var draft: some View {
    VStack(alignment: .leading, spacing: Space.m) {
      ForEach(state.comments.indices, id: \.self) { index in
        HStack(alignment: .top, spacing: Space.m) {
          VStack(alignment: .leading, spacing: Space.xs) {
            InlineCode(state.comments[index].anchor)
            Text(state.comments[index].text)
              .textStyle(.body)
              .foregroundStyle(Color(.textPrimary))
          }
          .frame(maxWidth: .infinity, alignment: .leading)
          Button {
            send(.remove(index))
          } label: {
            Image(systemName: "xmark")
          }
          .buttonStyle(CoxButtonStyle(.plain, size: .small))
          .accessibilityLabel("Remove comment")
        }
      }
      if let editing = state.editing {
        VStack(alignment: .leading, spacing: Space.xs) {
          InlineCode(editing)
          TextField(
            "Comment", text: $text,
            prompt: Text("Comment on this line…").foregroundStyle(Color(.textTertiary))
          )
          .textFieldStyle(.plain)
          .textStyle(.body)
          .foregroundStyle(Color(.textPrimary))
          .focused($typing)
          .onSubmit { send(.save(text: text)) }
          .onExitCommand { send(.save(text: "")) }
          .padding(.horizontal, Space.ml)
          .frame(height: Size.buttonHeightSmall)
          .insetWell(Color(.surfaceWindow), cornerRadius: Radius.m)
        }
      }
      HStack {
        Text("^[\(state.comments.count) comment](inflect: true)")
          .textStyle(.caption)
          .foregroundStyle(Color(.textSecondary))
        Spacer()
        Button("Send to agent") { send(.sendComments) }
          .buttonStyle(CoxButtonStyle(.primary, size: .small))
          .disabled(state.comments.isEmpty)
      }
    }
    .padding(.horizontal, Space.xl)
    .padding(.vertical, Space.l)
  }
}

#Preview("review") { PreviewMatrix { ReviewPaneSample(state: PreviewState.review) } }
#Preview("nothing left") {
  PreviewMatrix { ReviewPaneSample(state: PreviewState.reviewNothingLeft) }
}
#Preview("draft") { PreviewMatrix { ReviewPaneSample(state: PreviewState.reviewDraft) } }
#Preview("empty") { PreviewMatrix { ReviewPaneSample(state: .init()) } }
