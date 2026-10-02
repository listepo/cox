// Figma sync seams (frames 22-empty-session and 14-rewind-edit-resend): a welcome suggestion
// drafts its prompt; the rewind menu carries its turn and restored count, a failed preview leaves
// the count unknown, and its scopes and fork reach the core as `Intent.rewind` and `Intent.fork`.

import CoxClient
import Testing

@testable import CoxModel

private struct FailingPreview: RewindPreviewService {
  struct Failure: Error {}
  func restoredFiles(beforeTurn turn: UInt32) async throws -> Int { throw Failure() }
}

@MainActor
@Test func aWelcomeSuggestionDraftsItsPrompt() async throws {
  let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []))
  let composer = ComposerStore(session: SessionStore(session: session))
  let facts = try await MockWelcomeService().welcome(cwd: "/w/cox")
  let first = try #require(facts.suggestions.first)
  composer.suggest(first)
  #expect(composer.text == first.prompt)
  #expect(session.sent.isEmpty, "a suggestion drafts; the person sends")
}

@MainActor
@Test func theRewindMenuCountsTheFilesOrLeavesThemUnknown() async {
  let store = SessionStore(session: FixtureSession(fixture: Fixture(batches: [], snapshot: [])))
  #expect(
    await store.rewindMenu(turn: 2, preview: MockRewindPreviewService())
      == RewindMenuState(turn: 2, restoredFiles: 2))
  #expect(
    await store.rewindMenu(turn: 2, preview: FailingPreview())
      == RewindMenuState(turn: 2, restoredFiles: nil))
}

@MainActor
@Test func theRewindMenuRewindsAndForksBeforeItsTurn() async throws {
  let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []))
  let store = SessionStore(session: session)
  try await store.rewind(toTurn: 2, code: true, conversation: true)
  try await store.fork(beforeTurn: 2)
  #expect(session.sent == [.rewind(toTurn: 2, code: true, conversation: true), .fork(turn: 2)])
}
