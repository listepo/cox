// The "Needs you" store's check (T37.27.7): with the `approve-write` fixture it lists one row,
// which clears once the card is answered; and each kind of item as its row, an expired one
// read-only.

import CoxClient
import Foundation
import Testing

@testable import CoxModel

@MainActor
@Test func theRecordedApprovalIsOneRowThatClearsOnceAnswered() async throws {
  let fixture = try Fixture(contentsOf: try #require(approveWrite))
  let client = FixtureCoreClient(fixture: fixture, waitsForYou: true)
  let inbox = InboxStore(client: client)
  let session = try await client.open(OpenSession(cwd: "/", theme: "base16-ocean.dark"))
  let store = SessionStore(session: session)
  let run = Task { await store.run() }

  inbox.refresh()
  #expect(inbox.rows.isEmpty)
  let deadline = Date(timeIntervalSinceNow: 10)
  while pendingApproval(store) == nil, Date() < deadline { await Task.yield() }
  let call = try #require(pendingApproval(store))

  inbox.refresh()
  let item = try #require(fixture.notes.first?.item)
  #expect(
    inbox.rows == [
      InboxRow(
        id: "\(item.session)#1", session: item.session, status: .waiting,
        title: "write summary.md", subtitle: "approval waiting", isReadOnly: false)
    ])
  #expect(inbox.count == "1")

  _ = try await store.send(.approve(call: call, decision: .allow))
  await run.value
  inbox.refresh()
  #expect(inbox.rows.isEmpty)
  #expect(inbox.count == nil)
}

@Test func eachItemBecomesItsRowAndAnExpiredOneIsReadOnly() {
  func row(_ need: Need, expired: Bool = false, agent: String? = nil) -> InboxRow {
    let source = agent.map { Source(session: "child", agent: $0, preset: nil) }
    return InboxRow(InboxItem(session: "s", source: source, need: need, expired: expired, seq: 7))
  }
  func expected(
    _ status: InboxRow.Status, _ title: String, _ subtitle: String, readOnly: Bool = false
  ) -> InboxRow {
    InboxRow(
      id: "s#7", session: "s", status: status, title: title, subtitle: subtitle,
      isReadOnly: readOnly)
  }
  let approval = Need.approval(
    call: "c1", tool: "bash", subject: "git push", why: .risk(risk: .exec))
  #expect(row(approval) == expected(.waiting, "bash git push", "approval waiting"))
  #expect(
    row(.question(call: "c2", question: "Which branch?", options: []), agent: "reviewer")
      == expected(.waiting, "Which branch?", "reviewer · question waiting"))
  #expect(row(.failed(text: "boom")) == expected(.error, "boom", "turn failed"))
  #expect(
    row(.taskDone(task: "t", label: "tests", succeeded: true))
      == expected(.idle, "tests", "task done"))
  #expect(
    row(.taskDone(task: "t", label: "tests", succeeded: false))
      == expected(.error, "tests", "task failed"))
  #expect(
    row(approval, expired: true)
      == expected(
        .idle, "bash git push", "expired", readOnly: true))
}
