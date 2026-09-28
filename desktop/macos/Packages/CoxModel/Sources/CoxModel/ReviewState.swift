// Review (T37.28.2, DT§5.4): cox-app's `Changes` as the files grouped by the turn that changed
// each last, the checkpoints Review's timeline rewinds to, and the open file's net diff from
// `SessionClient.review` (A101). Here, not in CoxUI, because these decide what the pane shows
// (DS§1); the app copies them into `ReviewPane.State`. The grouping reads `changes::build`'s turn.

import CoxClient
import Foundation

public struct ReviewState: Equatable, Sendable {
  /// The files one turn changed last.
  public struct Turn: Equatable, Sendable {
    public var turn: UInt32
    public var files: [ChangesTabState.File]
  }

  /// Oldest turn first, each file in the order it was first changed.
  public var turns: [Turn] = []
  /// Oldest first, as the Changes tab lists them.
  public var checkpoints: [ChangesTabState.Checkpoint] = []
  /// The path of the open file.
  public var selection: String?
  /// Its diff; `nil` while none is open or it has none to show (a copy over the size cap).
  public var diff: DiffModel?

  public init() {}

  public init(
    _ changes: Changes, selection: String? = nil, diff: DiffModel? = nil,
    locale: Locale = .current, timeZone: TimeZone = .current
  ) {
    let tab = ChangesTabState(changes, locale: locale, timeZone: timeZone)
    for (file, row) in zip(changes.files, tab.files) {
      if let index = turns.firstIndex(where: { $0.turn == file.turn }) {
        turns[index].files.append(row)
      } else {
        turns.append(Turn(turn: file.turn, files: [row]))
      }
    }
    turns.sort { $0.turn < $1.turn }
    (checkpoints, self.selection, self.diff) = (tab.checkpoints, selection, diff)
  }
}

extension SessionStore {
  /// Review with `path` open, else the first changed file, read from the core on request.
  public func review(path: String? = nil) async throws -> ReviewState {
    let changes = try await session.changes()
    guard let selection = path ?? changes.files.first?.path else { return ReviewState(changes) }
    return ReviewState(changes, selection: selection, diff: try await session.review(selection))
  }
}
