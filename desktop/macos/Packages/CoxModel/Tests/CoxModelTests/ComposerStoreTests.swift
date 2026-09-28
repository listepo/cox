// ComposerStore (T37.24): the draft asks the client for rows only for an `@` token or a leading
// `/` token, a picked row replaces the token, and each kind of draft leaves as its one intent.

import CoxClient
import Foundation
import Testing

@testable import CoxModel

@MainActor
private func composer() -> (ComposerStore, FixtureSession) {
  let rows = [
    Completion(insert: "@src/lib.rs", detail: "src/lib.rs"),
    Completion(insert: "@src/main.rs", detail: "src/main.rs"),
    Completion(insert: "/compact", detail: "/compact [focus]"),
  ]
  let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []), completions: rows)
  return (ComposerStore(session: SessionStore(session: session)), session)
}

@MainActor
@Test func anAtTokenOffersFilesAndAPickedFileIsMentionedAndSent() async {
  let (store, session) = composer()
  store.edit("look at @ma")
  #expect(store.completions.map(\.insert) == ["@src/main.rs"])

  store.pick(0)
  #expect(store.text == "look at @src/main.rs ")
  #expect(store.mentions == ["@src/main.rs"])
  #expect(store.completions.isEmpty)

  await store.submit()
  #expect(session.sent == [.send(text: "look at @src/main.rs ", attachments: [])])
  #expect(store.text.isEmpty && store.mentions.isEmpty)
}

/// T37.24.9: the token is the word the caret ends, wherever it stands; the offsets are UTF-16, so
/// the `é` before it counts once.
@MainActor
@Test func theTokenAtTheCaretIsCompletedMidTextAndTheRestStays() {
  let (store, _) = composer()
  store.edit("café @ma and more")
  #expect(store.completions.isEmpty)
  store.select(8..<8)
  #expect(store.completions.map(\.insert) == ["@src/main.rs"])

  store.pick(0)
  #expect(store.text == "café @src/main.rs and more")
  #expect(store.selectedRange == 18..<18)
  #expect(store.mentions == ["@src/main.rs"])

  store.select(6..<6)
  #expect(store.completions.isEmpty)
  store.select(5..<9)
  #expect(store.completions.isEmpty)
  store.select(26..<26)
  #expect(store.selectedRange == nil)
}

@MainActor
@Test func aSlashTokenOffersCommandsOnlyAsTheFirstWord() async {
  let (store, session) = composer()
  store.edit("/co")
  #expect(store.completions.map(\.insert) == ["/compact"])
  store.edit("fix /co")
  #expect(store.completions.isEmpty)

  store.edit("/compact")
  await store.submit()
  #expect(session.sent == [.command(line: "/compact")])
}

@MainActor
@Test func aBangIntoAnEmptyDraftEntersShellModeAndSendsAShellLine() async {
  let (store, session) = composer()
  store.edit("!")
  #expect(store.isShell && store.text.isEmpty)
  store.edit("git status @src")
  #expect(store.completions.isEmpty)
  store.shareOutput = false

  await store.submit()
  #expect(session.sent == [.shell(command: "git status @src", share: false)])
  #expect(!store.isShell)
}

@MainActor
@Test func removingAMentionTakesItOutOfTheDraft() {
  let (store, _) = composer()
  store.edit("@li")
  store.moveSelection(by: 1)
  #expect(store.selection == 0)
  store.pick(0)
  store.edit(store.text + "please")
  store.removeMention("@src/lib.rs")
  #expect(store.text == "please")
  #expect(store.mentions.isEmpty)
}

@MainActor
@Test func attachedFilesAreReadAndSentWithTheTurn() async throws {
  let (store, session) = composer()
  let dir = FileManager.default.temporaryDirectory.appending(path: "cox-t37.24-\(UUID())")
  try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
  defer { try? FileManager.default.removeItem(at: dir) }
  let log = dir.appending(path: "trace.txt")
  try Data("429 after 3 tries\n".utf8).write(to: log)

  await store.attach([log, dir.appending(path: "missing.png")])
  #expect(store.failure != nil)
  #expect(store.canSend)
  await store.submit()

  let sent = Attachment(
    name: "trace.txt", mediaType: "text/plain",
    dataB64: Data("429 after 3 tries\n".utf8).base64EncodedString())
  #expect(session.sent == [.send(text: "", attachments: [sent])])
  #expect(store.attachments.isEmpty)
}

/// A usage view of a turn that runs (`done` false) or finished.
private func usage(done: Bool) -> UsageView {
  let tally = Tally(
    sent: 0, received: 0, cacheRead: 0, cacheWrite: 0, uncached: 0, costUsd: 0, calls: 0,
    estimated: false)
  let turn = TurnUsage(
    turn: "t", tally: tally, thinkingTokens: 0, ttftMs: nil, tokPerS: nil, exact: false,
    sparkline: [], done: done)
  return UsageView(session: tally, turn: turn, contextTokens: 0)
}

private func user(_ turn: UInt32) -> TimelinePatch {
  .upsert(
    block: Block(id: "u\(turn)", turn: turn, kind: .user(text: "", attachments: [])), after: nil)
}

@MainActor
@Test func whileATurnRunsReturnQueuesAndTheCountDropsAsQueuedTurnsStart() async {
  let (store, session) = composer()
  store.session.apply([user(1), .usage(usage: usage(done: false))])
  #expect(store.isRunning)

  store.edit("next")
  await store.submit()
  store.edit("after that")
  await store.submit()
  #expect(session.sent == [.queue(text: "next"), .queue(text: "after that")])
  #expect(store.queued == 2)

  store.session.apply([
    .upsert(
      block: Block(id: "u2", turn: 2, kind: .user(text: "next", attachments: [])), after: "u1")
  ])
  #expect(store.queued == 1)
  store.session.apply([.usage(usage: usage(done: true))])
  #expect(!store.isRunning)
}

@MainActor
@Test func commandReturnInterruptsAndSendsNowAndAttachmentsNeverQueue() async throws {
  let (store, session) = composer()
  store.session.apply([user(1), .usage(usage: usage(done: false))])
  let file = FileManager.default.temporaryDirectory.appending(path: "cox-t37.24-\(UUID()).txt")
  try Data("x".utf8).write(to: file)
  defer { try? FileManager.default.removeItem(at: file) }
  await store.attach([file])
  store.edit("look")

  await store.submit()
  #expect(session.sent.isEmpty)
  #expect(store.failure != nil)

  await store.submitNow()
  #expect(session.sent.first == .interrupt)
  #expect(session.sent.count == 2)
  #expect(store.queued == 0 && store.attachments.isEmpty)
}

@MainActor
@Test func upWalksOlderPromptsStopsAtTheOldestAndDownPastTheNewestEmptiesTheDraft() {
  let session = FixtureSession(
    fixture: Fixture(batches: [], snapshot: []), prompts: ["second", "first"])
  let store = ComposerStore(session: SessionStore(session: session))
  store.edit("draft")
  store.recall(-1)
  #expect(store.text == "draft" && !store.isRecalling)

  store.edit("")
  store.recall(-1)
  store.recall(-1)
  store.recall(-1)
  #expect(store.text == "first" && store.isRecalling)
  store.recall(1)
  #expect(store.text == "second")
  store.recall(1)
  #expect(store.text.isEmpty && !store.isRecalling)

  store.recall(-1)
  store.edit("second, edited")
  #expect(!store.isRecalling)
}
