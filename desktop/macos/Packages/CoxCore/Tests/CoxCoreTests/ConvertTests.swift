// The generated cox-ffi values convert to the `CoxClient` values a recorded
// fixture decodes to, so the stores see the same timeline live and replayed.

import CoxClient
import CoxFFIBindings
import Testing

@testable import CoxCore

@Test func aToolUpsertConvertsFieldForField() {
  let live = CoxFFIBindings.TimelinePatch.upsert(
    block: .init(
      id: "call:1", turn: 2,
      kind: .tool(
        tool: "read", summary: "Read `a`", icon: .read, risk: .readOnly, state: .done,
        tail: "1\thello", archive: .init(id: "01A", bytes: 24), diff: nil, durationMs: 7)),
    after: "item:1")
  let want = CoxClient.TimelinePatch.upsert(
    block: .init(
      id: "call:1", turn: 2,
      kind: .tool(
        tool: "read", summary: "Read `a`", icon: .read, risk: .readOnly, state: .done,
        tail: "1\thello", archive: .init(id: "01A", bytes: 24), diff: nil, durationMs: 7)),
    after: "item:1")
  #expect(CoxClient.TimelinePatch(live) == want)
}

@Test func aDocTailKeepsSpanStyleAndColour() {
  let span = CoxFFIBindings.Span(
    text: "fn", token: .accent, rgb: 0xB48EAD, bold: true, italic: false, strike: false,
    underline: false, link: nil)
  let live = CoxFFIBindings.TimelinePatch.docTail(
    id: "item:2", from: 1,
    blocks: [.code(lang: "rust", lines: [[span]]), .text(kind: .heading(2), lines: [])])
  var want = CoxClient.Span(text: "fn")
  (want.token, want.rgb, want.bold) = (.accent, 0xB48EAD, true)
  let blocks: [CoxClient.DocBlock] = [
    .code(lang: "rust", lines: [[want]]), .text(kind: .heading(2), lines: []),
  ]
  #expect(CoxClient.TimelinePatch(live) == .docTail(id: "item:2", from: 1, blocks: blocks))
}

@Test func anApprovalIntentConvertsToTheGeneratedIntent() {
  let intent = CoxClient.Intent.approve(call: "c1", decision: .edit(input: #"{"path":"a"}"#))
  let want = CoxFFIBindings.Intent.approve(call: "c1", decision: .edit(input: #"{"path":"a"}"#))
  #expect(CoxFFIBindings.Intent(intent) == want)
  #expect(CoxFFIBindings.Intent(.setEffort(effort: nil)) == .setEffort(effort: nil))
}

@Test func aQueuedIntentKeepsItsAttachmentsAndTheStatusKeepsTheCount() {
  let shot = CoxClient.Attachment(name: "shot.png", mediaType: "image/png", dataB64: "iVBO")
  let want = CoxFFIBindings.Intent.queue(
    text: "look", attachments: [.init(name: "shot.png", mediaType: "image/png", dataB64: "iVBO")])
  #expect(CoxFFIBindings.Intent(.queue(text: "look", attachments: [shot])) == want)
  let live = CoxFFIBindings.TimelinePatch.status(status: .init(queued: 2))
  #expect(CoxClient.TimelinePatch(live) == .status(status: .init(queued: 2)))
}

@Test func aChangesRecordConvertsFieldForField() {
  let live = CoxFFIBindings.Changes(
    files: [.init(path: "a.rs", change: .created, added: 3, removed: 0, call: "c1", turn: 2)],
    checkpoints: [.init(turn: 2, label: "Turn 2 · before a.rs", time: "2026-09-28T14:02:00.000Z")],
    worktree: .init(path: "/w", branch: "t1", base: "main", commit: "4273daa", bytes: 9))
  let want = CoxClient.Changes(
    files: [.init(path: "a.rs", change: .created, added: 3, removed: 0, call: "c1", turn: 2)],
    checkpoints: [.init(turn: 2, label: "Turn 2 · before a.rs", time: "2026-09-28T14:02:00.000Z")],
    worktree: .init(path: "/w", branch: "t1", base: "main", commit: "4273daa", bytes: 9))
  #expect(CoxClient.Changes(live) == want)
}
