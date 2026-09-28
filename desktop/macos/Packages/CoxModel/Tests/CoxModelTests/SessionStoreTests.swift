// SessionStore against the recorded fixtures (T37.16's Check) and the patch
// kinds the recorded scenarios do not reach yet.

import CoxClient
import Foundation
import Testing

@testable import CoxModel

/// Every `desktop/macos/Fixtures/*.json`, found from this file so the tests
/// read the recorder's output in place.
let fixtures: [URL] = {
    let dir = URL(filePath: #filePath)
        .deletingLastPathComponent()  // CoxModelTests
        .deletingLastPathComponent()  // Tests
        .deletingLastPathComponent()  // CoxModel
        .deletingLastPathComponent()  // Packages
        .deletingLastPathComponent()  // macos
        .appending(path: "Fixtures")
    let names = (try? FileManager.default.contentsOfDirectory(atPath: dir.path())) ?? []
    return names.filter { $0.hasSuffix(".json") }.sorted().map { dir.appending(path: $0) }
}()

@MainActor
@Test func fixturesAreFound() {
    #expect(!fixtures.isEmpty)
}

@MainActor
@Test(arguments: fixtures)
func replayingAFixtureEndsAtItsSnapshot(url: URL) async throws {
    let fixture = try Fixture(contentsOf: url)
    let client = FixtureCoreClient(fixture: fixture)
    let session = try await client.open(OpenSession(cwd: "/", theme: "base16-ocean.dark"))
    let store = SessionStore(session: session)

    await store.run()

    #expect(Array(store.blocks.values) == fixture.snapshot)
    #expect(Array(store.blocks.keys) == fixture.snapshot.map(\.id))
    #expect(store.usage != nil)
}

@MainActor
@Test func sendReachesTheSession() async throws {
    let session = FixtureSession(fixture: Fixture(batches: [], snapshot: []))
    let store = SessionStore(session: session)

    _ = try await store.send(.send(text: "hi", attachments: []))

    #expect(session.sent == [.send(text: "hi", attachments: [])])
}

private func tool(_ id: BlockID, tail: String) -> Block {
    Block(
        id: id, turn: 1,
        kind: .tool(
            tool: "bash", summary: "Ran", icon: .shell, risk: .exec, state: .running, tail: tail,
            archive: nil, diff: nil, durationMs: 0))
}

private func paragraph(_ text: String) -> DocBlock {
    .text(kind: .paragraph, lines: [[Span(text: text)]])
}

@MainActor
@Test func upsertInsertsAfterItsAnchorAndReplacesInPlace() {
    let store = SessionStore(session: FixtureSession(fixture: Fixture(batches: [], snapshot: [])))
    store.apply([
        .upsert(block: Block(id: "a", turn: 1, kind: .thinking(text: "")), after: nil),
        .upsert(block: Block(id: "c", turn: 1, kind: .thinking(text: "")), after: "a"),
        .upsert(block: Block(id: "b", turn: 1, kind: .thinking(text: "")), after: "a"),
        .upsert(block: Block(id: "a", turn: 1, kind: .thinking(text: "x")), after: nil),
        .upsert(block: Block(id: "z", turn: 1, kind: .thinking(text: "")), after: "gone"),
    ])
    #expect(Array(store.blocks.keys) == ["a", "b", "c", "z"])
    #expect(store.blocks["a"]?.kind == .thinking(text: "x"))
}

@MainActor
@Test func appendTextKeepsTheLastFiveLinesOfAToolTail() {
    let store = SessionStore(session: FixtureSession(fixture: Fixture(batches: [], snapshot: [])))
    store.apply([
        .reset(blocks: [tool("t", tail: "1\n2\n3\n"), Block(id: "k", turn: 1, kind: .thinking(text: "a"))]),
        .appendText(id: "t", text: "4\r\n5\n6\n"),
        .appendText(id: "k", text: "b"),
    ])
    #expect(store.blocks["t"] == tool("t", tail: "2\n3\n4\r\n5\n6\n"))
    #expect(store.blocks["k"]?.kind == .thinking(text: "ab"))
}

@MainActor
@Test func docTailReplacesFromItsIndexAndRemoveDrops() {
    let store = SessionStore(session: FixtureSession(fixture: Fixture(batches: [], snapshot: [])))
    let doc = StyledDoc(blocks: [paragraph("a"), paragraph("b")])
    store.apply([
        .reset(blocks: [Block(id: "m", turn: 1, kind: .assistant(text: "", doc: doc)), tool("t", tail: "")]),
        .docTail(id: "m", from: 1, blocks: [paragraph("B"), paragraph("c")]),
        .docTail(id: "m", from: 9, blocks: [paragraph("ignored")]),
        .remove(id: "t"),
    ])
    let want = StyledDoc(blocks: [paragraph("a"), paragraph("B"), paragraph("c")])
    #expect(store.blocks["m"]?.kind == .assistant(text: "", doc: want))
    #expect(Array(store.blocks.keys) == ["m"])
}

@Test func lastLinesMatchesTheRustTail() {
    #expect(lastLines("a\nb") == "a\nb")
    #expect(lastLines("1\n2\n3\n4\n5\n6\n") == "2\n3\n4\n5\n6\n")
    #expect(lastLines("1\n2\n3\n4\n5\n6") == "2\n3\n4\n5\n6")
}
