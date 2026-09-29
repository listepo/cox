// The sidebar's session list (T37.22.5, T58.4.5): the core's sections as they come, each row's
// subtitle parts joined and its age worded in the locale; the filter and the folds go to the core,
// which decides what matches (`cox_app::workspace::sidebar`' tests); without a workspace the inbox
// alone is "Needs you"; the toolbar's title and project come from a session's entry; the list
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

private let sidebar = [
  SidebarGroup(
    id: "running", title: "Running", kind: .section(count: nil),
    rows: [
      SidebarEntry(
        id: "jitter", session: "jitter", status: .running, title: "Add retry jitter",
        subtitle: [.text("cox"), .text("running")], cost: "$0.42")
    ]),
  SidebarGroup(
    id: "/src/cox", title: "cox", kind: .project(isExpanded: false),
    rows: [
      SidebarEntry(
        id: "bench", session: "bench", status: .idle, title: "Bench plugin cold start",
        subtitle: [.text("claude"), .age(updatedAt: ago(7200)), .text("done")], cost: "$1.18"),
      SidebarEntry(
        id: "fresh", session: "fresh", status: .error, title: "Untitled session",
        subtitle: [.age(updatedAt: "not a date"), .text("failed")]),
    ]),
]

private let workspace = FixtureWorkspace(
  projects: [Project(root: "/src/cox", name: "cox")],
  sessions: [
    "/src/cox": [
      SessionEntry(
        id: "bench", title: "Bench plugin cold start", name: "Bench plugin cold start",
        cwd: "/src/cox", updatedAt: ago(7200), turns: 5, costUsd: 1.18)
    ]
  ],
  sidebar: sidebar)

@MainActor
private func listed() -> SidebarStore {
  let store = SidebarStore(
    workspace: workspace, inbox: nil, locale: Locale(identifier: "en_US_POSIX"))
  store.refresh(now: now)
  return store
}

@MainActor
@Test func theCoresSectionsShowWithTheirPartsJoinedAndTheAgeWorded() {
  let sections = listed().sections
  #expect(sections.map(\.id) == ["running", "/src/cox"])
  #expect(sections[0].kind == .section(count: nil))
  #expect(
    sections[0].rows == [
      SidebarRow(
        id: "jitter", session: "jitter", status: .running, title: "Add retry jitter",
        subtitle: "cox · running", cost: "$0.42", isReadOnly: false)
    ])
  #expect(sections[1].kind == .project(isExpanded: false))
  #expect(sections[1].rows[0].subtitle == "claude · 2h ago · done")
  #expect(sections[1].rows[0].cost == "$1.18")
  #expect(sections[1].rows[1].status == .error)
  #expect(sections[1].rows[1].subtitle == "failed", "an age that is no date is left out")
}

/// Records what the sidebar was asked with.
private final class Asked: WorkspaceClient, @unchecked Sendable {
  private let lock = NSLock()
  private var asked: [(String, [String])] = []
  var last: (filter: String, folded: [String])? { lock.withLock { asked.last } }
  var count: Int { lock.withLock { asked.count } }

  func projects(limit: UInt32) -> [Project] { [] }
  func sessions(project: String, limit: UInt32) -> [SessionEntry] { [] }
  func activity(session: String) -> Activity { .idle }
  func sidebar(filter: String, folded: [String]) -> [SidebarGroup] {
    lock.withLock { asked.append((filter, folded)) }
    return []
  }
  func changed() async throws { try await Task.sleep(for: .seconds(86_400)) }
  func rename(session: String, title: String) -> Bool { false }
}

@MainActor
@Test func theFilterAndTheFoldsGoToTheCore() {
  let core = Asked()
  let store = SidebarStore(workspace: core, inbox: nil)
  store.refresh()
  #expect(core.last?.filter == "" && core.last?.folded == [])
  store.toggle("/src/web")
  store.toggle("/src/acme")
  #expect(core.last?.folded == ["/src/acme", "/src/web"])
  store.filter = "sitemap"
  #expect(core.last?.filter == "sitemap")
  let asked = core.count
  store.filter = "sitemap"
  #expect(core.count == asked, "the same filter asks nothing")
  store.toggle("/src/web")
  #expect(core.last?.folded == ["/src/acme"])
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
  func sidebar(filter: String, folded: [String]) -> [SidebarGroup] {
    let rows = sessions(project: project.root, limit: 20).map {
      SidebarEntry(id: $0.id, session: $0.id, status: .idle, title: $0.name)
    }
    return [
      SidebarGroup(
        id: project.root, title: project.name, kind: .project(isExpanded: true), rows: rows)
    ]
  }
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
    lock.withLock { [SessionEntry(id: "s1", title: title, name: title ?? "Untitled session")] }
  }
  func activity(session: String) -> Activity { .idle }
  func sidebar(filter: String, folded: [String]) -> [SidebarGroup] {
    let rows = sessions(project: project.root, limit: 20).map {
      SidebarEntry(id: $0.id, session: $0.id, status: .idle, title: $0.name)
    }
    return [
      SidebarGroup(
        id: project.root, title: project.name, kind: .project(isExpanded: true), rows: rows)
    ]
  }
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
}
