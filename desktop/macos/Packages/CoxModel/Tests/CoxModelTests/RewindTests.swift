// The rewind timeline's intent (T37.28.1): a Changes tab checkpoint and a scope reach the session
// as `Intent.rewind` to that turn.

import CoxClient
import Testing

@testable import CoxModel

@MainActor
@Test func aCheckpointRewindsToItsTurnInThePickedScope() async throws {
  let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []))
  let store = SessionStore(session: session)
  let changes = Changes(checkpoints: [Checkpoint(turn: 2, label: "Turn 2", time: "")])
  let id = try #require(ChangesTabState(changes).checkpoints.first?.id)
  try await store.rewind(checkpoint: id, code: true, conversation: false)
  try await store.rewind(checkpoint: id, code: true, conversation: true)
  try await store.rewind(checkpoint: "not a turn", code: true, conversation: true)
  #expect(
    session.sent == [
      .rewind(toTurn: 2, code: true, conversation: false),
      .rewind(toTurn: 2, code: true, conversation: true),
    ])
}

@MainActor
@Test func theChangesTabsPlainRewindRestoresCodeOnly() async throws {
  let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []))
  try await SessionStore(session: session).rewind(checkpoint: "3")
  #expect(session.sent == [.rewind(toTurn: 3, code: true, conversation: false)])
}
