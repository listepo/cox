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

@Test func aDocTailKeepsSpanStyleColourAndLineStructure() {
  let span = CoxFFIBindings.Span(
    text: "fn", token: .accent, rgb: 0xB48EAD, light: 0x4F5B66, bold: true, italic: false,
    strike: false, underline: false, link: nil)
  let live = CoxFFIBindings.TimelinePatch.docTail(
    id: "item:2", from: 1,
    blocks: [
      .code(lang: "rust", lines: [[span]]), .text(kind: .heading(2), lines: []),
      .text(kind: .list, lines: [.init(quote: 1, depth: 2, marker: "3.", spans: [span])]),
    ])
  var want = CoxClient.Span(text: "fn")
  (want.token, want.rgb, want.light, want.bold) = (.accent, 0xB48EAD, 0x4F5B66, true)
  let blocks: [CoxClient.DocBlock] = [
    .code(lang: "rust", lines: [[want]]), .text(kind: .heading(2), lines: []),
    .text(kind: .list, lines: [CoxClient.TextLine([want], quote: 1, depth: 2, marker: "3.")]),
  ]
  #expect(CoxClient.TimelinePatch(live) == .docTail(id: "item:2", from: 1, blocks: blocks))
}

@Test func anApprovalIntentConvertsToTheGeneratedIntent() {
  let intent = CoxClient.Intent.approve(call: "c1", decision: .edit(input: #"{"path":"a"}"#))
  let want = CoxFFIBindings.Intent.approve(call: "c1", decision: .edit(input: #"{"path":"a"}"#))
  #expect(CoxFFIBindings.Intent(intent) == want)
  #expect(CoxFFIBindings.Intent(.setEffort(effort: nil)) == .setEffort(effort: nil))
}

@Test func aQueuedIntentKeepsItsAttachmentsAndThinkAndTheStatusKeepsEveryField() {
  let shot = CoxClient.Attachment(name: "shot.png", mediaType: "image/png", dataB64: "iVBO")
  let want = CoxFFIBindings.Intent.queue(
    text: "look", attachments: [.init(name: "shot.png", mediaType: "image/png", dataB64: "iVBO")],
    confirmThink: true)
  let queue = CoxClient.Intent.queue(text: "look", attachments: [shot], confirmThink: true)
  #expect(CoxFFIBindings.Intent(queue) == want)
  let live = CoxFFIBindings.TimelinePatch.status(
    status: .init(
      queued: 2, mode: .plan, nextMode: .auto, model: "claude-sonnet-5",
      modelName: "Claude Sonnet 5", effort: .high))
  let status = CoxClient.Status(
    queued: 2, mode: .plan, nextMode: .auto, model: "claude-sonnet-5", effort: .high,
    modelName: "Claude Sonnet 5")
  #expect(CoxClient.TimelinePatch(live) == .status(status: status))
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

@Test func aTodoItemConvertsWithEachState() {
  let live: [CoxFFIBindings.TodoItem] = [
    .init(id: "1", text: "Read", state: .done), .init(id: "2", text: "Test", state: .inProgress),
    .init(id: "3", text: "Push", state: .pending),
  ]
  let want: [CoxClient.TodoItem] = [
    .init(id: "1", text: "Read", state: .done), .init(id: "2", text: "Test", state: .inProgress),
    .init(id: "3", text: "Push", state: .pending),
  ]
  #expect(live.map { CoxClient.TodoItem($0) } == want)
}

@Test func turnCostsConvertFieldForField() {
  let row = CoxFFIBindings.CostRow(label: "explore", values: ["9.8k", "0.03"], detail: true)
  let total = CoxFFIBindings.CostRow(label: "Session", values: ["9.8k", "0.03"], detail: false)
  let live = CoxFFIBindings.TurnCosts(
    columns: ["In", "$"], rows: [row], total: total, project: "Project cox today: $0.03")
  #expect(
    CoxClient.TurnCosts(live)
      == CoxClient.TurnCosts(
        columns: ["In", "$"],
        rows: [CoxClient.CostRow(label: "explore", values: ["9.8k", "0.03"], detail: true)],
        total: CoxClient.CostRow(label: "Session", values: ["9.8k", "0.03"]),
        project: "Project cox today: $0.03"))
}
