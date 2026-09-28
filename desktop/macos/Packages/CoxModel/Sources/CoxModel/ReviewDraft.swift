// Review's line comments (T37.28.4, DT§5.4): the draft a line-number click adds to and "Send to
// agent" posts as one `Intent.send`. Here, not in CoxUI, because the anchor is read from the open
// file's diff and the draft outlives the open file (it lives on `SessionStore`); the prompt's
// words are cox-app's (`SessionClient.reviewMessage`), so every surface sends the same.

import CoxClient
import Foundation

public struct ReviewDraft: Equatable, Sendable {
  /// In the order they were written.
  public var comments: [LineComment] = []
  /// The line a click picked, its text still being typed; `nil` while none is.
  public var editing: LineComment?

  public init(comments: [LineComment] = [], editing: LineComment? = nil) {
    (self.comments, self.editing) = (comments, editing)
  }

  /// Starts a comment on line `line` of hunk `hunk` of `review`'s open diff, anchored at its
  /// number on disk, or at its number before the change for a removed line. An index outside
  /// the diff picks nothing.
  public mutating func pick(_ review: ReviewState, hunk: Int, line: Int) {
    guard let path = review.selection, let hunks = review.diff?.hunks,
      hunks.indices.contains(hunk), hunks[hunk].lines.indices.contains(line)
    else { return }
    let picked = hunks[hunk].lines[line]
    guard let number = picked.new ?? picked.old else { return }
    editing = LineComment(path: path, line: number, removed: picked.new == nil, text: "")
  }

  /// Adds the picked line with `text`, trimmed; blank text drops it instead.
  public mutating func save(_ text: String) {
    defer { editing = nil }
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard var comment = editing, !trimmed.isEmpty else { return }
    comment.text = trimmed
    comments.append(comment)
  }

  public mutating func remove(at index: Int) {
    guard comments.indices.contains(index) else { return }
    comments.remove(at: index)
  }
}

extension SessionStore {
  /// Posts the draft as one turn in cox-app's words and empties it once the core took it; sends
  /// nothing while no comment has text.
  public func sendReview() async throws {
    guard let text = session.reviewMessage(reviewDraft.comments) else { return }
    _ = try await send(.send(text: text, attachments: [], confirmThink: false))
    reviewDraft = ReviewDraft()
  }
}
