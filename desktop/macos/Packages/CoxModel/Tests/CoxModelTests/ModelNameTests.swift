// The model pill's name (T37.22.7, A111): the catalog's name loses its vendor prefix, a model
// without a name shows its id, and the recorded `approve-write` session's pill reads
// `Sonnet 5 · high`.

import CoxClient
import Foundation
import Testing

@testable import CoxModel

@Test func aClaudeNameLosesItsVendorPrefixAndOthersStayWhole() {
  #expect(ModelName.short("Claude Sonnet 5", id: "claude-sonnet-5") == "Sonnet 5")
  #expect(ModelName.short("GPT-5.1", id: "gpt-5.1") == "GPT-5.1")
  #expect(ModelName.short("DeepSeek V4 Pro", id: "deepseek-v4-pro") == "DeepSeek V4 Pro")
  // A name that is only the prefix keeps it rather than showing nothing.
  #expect(ModelName.short("Claude ", id: "claude") == "Claude ")
}

@Test func aModelWithoutANameShowsItsId() {
  #expect(ModelName.short(nil, id: "qwen3-coder") == "qwen3-coder")
  #expect(ModelName.short("", id: "qwen3-coder") == "qwen3-coder")
}

@MainActor
@Test func theRecordedSessionsPillReadsSonnet5High() async throws {
  let fixture = try Fixture(contentsOf: try #require(approveWrite))
  let client = FixtureCoreClient(fixture: fixture)
  let session = try await client.open(OpenSession(cwd: "/", theme: "base16-ocean.dark"))
  let store = SessionStore(session: session)
  let composer = ComposerStore(session: store)
  let run = Task { await store.run() }
  let deadline = Date(timeIntervalSinceNow: 10)
  while composer.model == nil, Date() < deadline { await Task.yield() }
  #expect(composer.model == "Sonnet 5 · high")
  run.cancel()
}
