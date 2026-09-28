// The inspector's Changes tab (T37.29.1, DT§5.1): cox-app's `Changes` as the rows CoxUI's
// `ChangesTab.State` holds — the changed files, the checkpoints with their id and time, and the
// worktree's facts. Here, not in CoxUI, because these decide what the tab shows (DS§1); the app
// copies them into `ChangesTab.State` field for field.

import CoxClient
import Foundation

public struct ChangesTabState: Equatable, Sendable {
  /// `ChangedFileRow.File`.
  public struct File: Equatable, Sendable {
    public var path: String
    public var change: FileChange
    public var added: Int
    public var removed: Int
  }

  /// `CheckpointRow.Checkpoint`; `id` is the turn a rewind goes back to.
  public struct Checkpoint: Equatable, Sendable {
    public var id: String
    public var label: String
    public var time: String
  }

  /// `KeyValueGrid.Row`.
  public struct Fact: Equatable, Sendable {
    public var label: String
    public var values: [String]
  }

  public var files: [File] = []
  public var checkpoints: [Checkpoint] = []
  /// Branch, base and size; empty outside a linked worktree.
  public var worktree: [Fact] = []

  public init() {}

  /// `locale` and `timeZone` format a checkpoint's time and the worktree's size.
  public init(_ changes: Changes, locale: Locale = .current, timeZone: TimeZone = .current) {
    files = changes.files.map {
      File(path: $0.path, change: $0.change, added: Int($0.added), removed: Int($0.removed))
    }
    let clock = Date.FormatStyle(
      date: .omitted, time: .shortened, locale: locale, timeZone: timeZone)
    checkpoints = changes.checkpoints.map {
      Checkpoint(
        id: String($0.turn), label: $0.label,
        time: Self.date($0.time).map { $0.formatted(clock) } ?? "")
    }
    guard let tree = changes.worktree else { return }
    worktree.append(Fact(label: "Branch", values: [tree.branch ?? "detached"]))
    if let base = tree.base, let commit = tree.commit {
      worktree.append(Fact(label: "Base", values: ["\(base) @ \(commit)"]))
    }
    let size = ByteCountFormatStyle(style: .file, locale: locale)
    worktree.append(Fact(label: "Size", values: [Int64(clamping: tree.bytes).formatted(size)]))
  }

  /// An RFC 3339 time as cox.db writes it, with or without milliseconds.
  private static func date(_ text: String) -> Date? {
    let withFraction = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
    return (try? withFraction.parse(text)) ?? (try? Date.ISO8601FormatStyle().parse(text))
  }
}

extension SessionStore {
  /// The Changes tab's state, read from the core when the tab asks (T37.29.1).
  public func changesTab() async throws -> ChangesTabState {
    ChangesTabState(try await session.changes())
  }
}
