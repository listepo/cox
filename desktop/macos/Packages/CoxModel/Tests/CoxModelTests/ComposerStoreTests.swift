// ComposerStore (T37.24): the draft asks the client for rows only for an `@` token or a leading
// `/` token, a picked row replaces the token, and each kind of draft leaves as its one intent.

import CoxClient
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
