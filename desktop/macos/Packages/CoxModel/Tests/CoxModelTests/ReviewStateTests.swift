// Review's state (T37.28.2): the changed files grouped by the turn that changed each last, and
// the open file's diff read through SessionStore from the fixture session.

import CoxClient
import Testing

@testable import CoxModel

private let changes = Changes(
  files: [
    ChangedFile(path: "src/retry.rs", change: .edited, added: 18, removed: 4, call: "c3", turn: 2),
    ChangedFile(path: "notes.md", change: .edited, added: 1, removed: 1, call: "c1", turn: 1),
    ChangedFile(path: "new.rs", change: .created, added: 1, removed: 0, call: "c2", turn: 2),
  ],
  checkpoints: [
    Checkpoint(turn: 1, label: "Turn 1 · before notes.md", time: ""),
    Checkpoint(turn: 2, label: "Turn 2 · before retry.rs and 1 more", time: ""),
  ])

private let notes = DiffModel(
  path: "notes.md",
  hunks: [
    DiffHunk(
      header: "@@ -1 +1 @@",
      lines: [
        DiffLine(kind: .del, old: 1, new: nil, spans: []),
        DiffLine(kind: .add, old: nil, new: 1, spans: []),
      ])
  ])

@Test func filesGroupUnderTheTurnThatChangedThemLastOldestFirst() {
  let review = ReviewState(changes)
  #expect(review.turns.map(\.turn) == [1, 2])
  #expect(review.turns.map { $0.files.map(\.path) } == [["notes.md"], ["src/retry.rs", "new.rs"]])
  #expect(review.checkpoints.map(\.id) == ["1", "2"])
  #expect((review.selection, review.diff) == (nil, nil))
}

@MainActor
@Test func theStoreOpensTheFirstFileOrTheAskedOneWithItsDiff() async throws {
  let session = FixtureSession(
    fixture: Fixture(batches: [], snapshot: []), changes: changes, reviews: ["notes.md": notes])
  let store = SessionStore(session: session)
  let first = try await store.review()
  #expect((first.selection, first.diff) == ("src/retry.rs", nil))
  let asked = try await store.review(path: "notes.md")
  #expect((asked.selection, asked.diff) == ("notes.md", notes))
  #expect(
    try await SessionStore(session: FixtureSession(fixture: .init(batches: [], snapshot: [])))
      .review() == ReviewState())
}
