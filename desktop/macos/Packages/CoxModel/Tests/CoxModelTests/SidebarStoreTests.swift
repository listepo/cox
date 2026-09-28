// The sidebar's session list (T37.22.5): the inbox first, running sessions next, then each project
// with its other sessions, each row with its status, age and cost; a filter narrows every section
// and opens a folded project; the toolbar's title and project come from a session's entry; the list
// re-reads when the workspace changed; and the providers' footer counts the usable providers
// (A110) with the checklist's key health as its dot.

import CoxClient
import Foundation
import Testing

@testable import CoxModel

private let now = Date(timeIntervalSince1970: 1_790_000_000)

private func ago(_ seconds: TimeInterval) -> String {
  Date(timeInterval: -seconds, since: now).formatted(.iso8601)
}

private let workspace = FixtureWorkspace(
  projects: [
    Project(root: "/src/cox", name: "cox"), Project(root: "/src/acme-web", name: "acme-web"),
  ],
  sessions: [
    "/src/cox": [
      SessionEntry(
        id: "jitter", title: "Add retry jitter", cwd: "/src/cox", updatedAt: ago(60), turns: 3,
        costUsd: 0.42),
      SessionEntry(
        id: "bench", title: "Bench plugin cold start", cwd: "/src/cox", updatedAt: ago(7200),
        turns: 5, costUsd: 1.18),
    ],
    "/src/acme-web": [
      SessionEntry(id: "sitemap", title: "Sitemap generator", updatedAt: ago(3600), turns: 1),
      SessionEntry(id: "fresh", updatedAt: ago(30)),
    ],
  ],
  activity: ["jitter": .running, "sitemap": .failed])

@MainActor
private func listed() -> SidebarStore {
  let store = SidebarStore(
    workspace: workspace, inbox: nil, locale: Locale(identifier: "en_US_POSIX"))
  store.refresh(now: now)
  return store
}

@MainActor
@Test func runningSessionsLeaveTheirProjectForTheRunningSection() {
  let sections = listed().sections
  #expect(sections.map(\.id) == ["running", "/src/cox", "/src/acme-web"])
  #expect(sections[0].kind == .section(count: nil))
  #expect(
    sections[0].rows == [
      SidebarRow(
        id: "jitter", session: "jitter", status: .running, title: "Add retry jitter",
        subtitle: "cox · running", cost: "$0.42", isReadOnly: false)
    ])
  #expect(sections[1].kind == .project(isExpanded: true))
  #expect(sections[1].rows.map(\.id) == ["bench"])
  #expect(sections[1].rows[0].subtitle == "2h ago · done")
  #expect(sections[1].rows[0].cost == "$1.18")
  let acme = sections[2].rows
  #expect(acme.map(\.status) == [.error, .idle])
  #expect(acme[0].subtitle == "1h ago · failed")
  #expect(acme[1].title == "Untitled session")
  #expect(acme[1].cost == nil)
}

@MainActor
@Test func aFilterNarrowsEverySectionAndOpensAFoldedProject() {
  let store = listed()
  store.toggle("/src/acme-web")
  #expect(store.sections.last?.kind == .project(isExpanded: false))

  store.filter = "sitemap"
  #expect(store.sections.map(\.id) == ["/src/acme-web"])
  #expect(store.sections[0].kind == .project(isExpanded: true))
  #expect(store.sections[0].rows.map(\.id) == ["sitemap"])

  store.filter = ""
  store.toggle("/src/acme-web")
  #expect(store.sections.last?.kind == .project(isExpanded: true))
}

@MainActor
@Test func theInboxComesFirstWithItsCount() async throws {
  let url = try #require(fixtures.first { $0.lastPathComponent == "approve-write.json" })
  let client = FixtureCoreClient(fixture: try Fixture(contentsOf: url), waitsForYou: true)
  let store = SidebarStore(workspace: nil, inbox: InboxStore(client: client))
  let session = try await client.open(OpenSession(cwd: "/", theme: "base16-ocean.dark"))
  let run = Task { await SessionStore(session: session).run() }
  defer {
    session.close()
    run.cancel()
  }
  let deadline = Date(timeIntervalSinceNow: 10)
  while client.inbox().isEmpty, Date() < deadline { await Task.yield() }

  store.refresh()
  let needs = try #require(store.sections.first)
  #expect(needs.id == "needs-you")
  #expect(needs.kind == .section(count: "1"))
  #expect(needs.rows.map(\.status) == [.waiting])
  #expect(needs.rows[0].subtitle == "approval waiting")
}

@MainActor
@Test func aSessionsEntryNamesTheToolbarsTitleAndProject() {
  let store = listed()
  let entry = store.entry("bench")
  #expect(entry?.session.title == "Bench plugin cold start")
  #expect(entry?.project.name == "cox")
  #expect(store.entry("nope") == nil)
}

@Test func theFooterCountsUsableProvidersAndShowsTheKeyCheck() {
  let passed = CheckRow(id: .providerKey, status: .passed, detail: "anthropic key found")
  let health = ProviderHealth(usable: ["anthropic", "local", "openai"], check: passed)
  #expect(health.text == "3 providers")
  #expect(health.status == .running)
  #expect(ProviderHealth(usable: ["anthropic"], check: nil).text == "1 provider")
  let failed = CheckRow(id: .providerKey, status: .failed, detail: "")
  #expect(ProviderHealth(usable: [], check: failed).status == .error)
  #expect(ProviderHealth(usable: [], check: failed).text == "0 providers")
  #expect(ProviderHealth(usable: nil, check: failed).text.isEmpty)
}

/// A workspace whose sessions grow by one each time it reports a change.
private final class Growing: WorkspaceClient, @unchecked Sendable {
  private let lock = NSLock()
  private var count = 0
  private let project = Project(root: "/src/cox", name: "cox")

  func projects(limit: UInt32) -> [Project] { [project] }
  func sessions(project: String, limit: UInt32) -> [SessionEntry] {
    lock.withLock { (0..<count).map { SessionEntry(id: "s\($0)") } }
  }
  func activity(session: String) -> Activity { .idle }
  func changed() async throws {
    try await Task.sleep(for: .milliseconds(10))
    lock.withLock { count += 1 }
  }
  func rename(session: String, title: String) -> Bool { false }
}

/// One session whose title a rename sets, as `cox.db` keeps it.
private final class Titled: WorkspaceClient, @unchecked Sendable {
  private let lock = NSLock()
  private var title: String?
  private let project = Project(root: "/src/cox", name: "cox")

  func projects(limit: UInt32) -> [Project] { [project] }
  func sessions(project: String, limit: UInt32) -> [SessionEntry] {
    lock.withLock { [SessionEntry(id: "s1", title: title)] }
  }
  func activity(session: String) -> Activity { .idle }
  func changed() async throws { try await Task.sleep(for: .seconds(86_400)) }
  func rename(session: String, title: String) -> Bool {
    lock.withLock { self.title = title }
    return true
  }
}

/// A113: a sidebar rename reads the list again, so the row and the toolbar show the new title.
@MainActor
@Test func aRenameShowsTheNewTitleInTheRowAndTheToolbar() {
  let store = SidebarStore(workspace: Titled(), inbox: nil)
  store.refresh()
  #expect(store.sections.first?.rows.first?.title == "Untitled session")
  store.rename("s1", to: "Fix the ledger")
  #expect(store.sections.first?.rows.first?.title == "Fix the ledger")
  #expect(store.entry("s1")?.session.name == "Fix the ledger")
}

@MainActor
@Test func theListReadsAgainEachTimeTheWorkspaceChanged() async {
  let store = SidebarStore(workspace: Growing(), inbox: nil)
  let watching = Task { await store.watch() }
  defer { watching.cancel() }
  let deadline = Date(timeIntervalSinceNow: 10)
  while (store.sections.first?.rows.count ?? 0) < 2, Date() < deadline {
    try? await Task.sleep(for: .milliseconds(5))
  }
  #expect((store.sections.first?.rows.count ?? 0) >= 2)
  #expect(store.sections.first?.rows.first?.title == "Untitled session")
}

@MainActor
@Test func anExternalAgentsSessionRowNamesItsAgentFirst() {
  let store = SidebarStore(
    workspace: FixtureWorkspace(
      projects: [Project(root: "/src/cox", name: "cox")],
      sessions: [
        "/src/cox": [
          SessionEntry(
            id: "acp", title: "Refactor", cwd: "/src/cox", updatedAt: ago(7200), turns: 1,
            agent: "claude")
        ]
      ]),
    inbox: nil, locale: Locale(identifier: "en_US_POSIX"))
  store.refresh(now: now)
  let row = store.sections.flatMap(\.rows).first { $0.id == "acp" }
  #expect(row?.subtitle == "claude · 2h ago · done")
}
