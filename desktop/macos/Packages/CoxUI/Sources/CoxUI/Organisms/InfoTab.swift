// `InfoTab` (DS§6.4 row `InfoTab`; DT§5.1 Info): the inspector's last tab — what the session is
// and where it lives: its id, folder, worktree and rollout file, and the config layers it runs
// with. Separate so the `Inspector` frame stays a slot and each tab is its own view, fed plain
// values the app copies from the core (T37.29.5).

import SwiftUI

/// Two `InspectorSection`s, each one `KeyValueGrid`: the session's facts, then the config
/// layers with each file under its layer. With nothing to show, one quiet line.
public struct InfoTab: View {
  /// What the tab lists, formatted by the core.
  public struct State: Equatable, Sendable {
    /// Session, folder, worktree and branch, rollout.
    public var session: [KeyValueGrid.Row] = []
    /// A layer and its key count, its file as a detail row under it.
    public var config: [KeyValueGrid.Row] = []

    public init(session: [KeyValueGrid.Row] = [], config: [KeyValueGrid.Row] = []) {
      (self.session, self.config) = (session, config)
    }
  }

  let state: State

  public init(state: State) { self.state = state }

  public var body: some View {
    VStack(alignment: .leading, spacing: Space.xl) {
      if !state.session.isEmpty {
        InspectorSection("Session") { KeyValueGrid(rows: state.session) }
      }
      if !state.config.isEmpty {
        InspectorSection("Config") { KeyValueGrid(rows: state.config) }
      }
      if state == State() {
        Text("No session yet")
          .textStyle(.caption)
          .foregroundStyle(Color(.textSecondary))
      }
    }
  }
}

#Preview("info") { PreviewMatrix { InfoInspectorSample(state: PreviewState.info) } }
#Preview("empty") { PreviewMatrix { InfoInspectorSample(state: .init()) } }
