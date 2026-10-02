// The welcome hero's state (Figma frame 22-empty-session): the service's facts reach CoxUI's
// `WelcomeHero.State` field for field, under the toolbar's project name.

import CoxModel
import CoxUI
import Testing

@testable import CoxTranscript

@MainActor
@Test func theWelcomeFactsBecomeTheHerosStateFieldForField() async throws {
  let facts = try await MockWelcomeService().welcome(cwd: "/w/cox")
  let state = SessionWelcome.state(project: "cox", facts: facts)
  #expect(state.project == "cox")
  #expect(state.summary == facts.summary)
  #expect(state.suggestions.map(\.prompt) == facts.suggestions.map(\.prompt))
  #expect(state.suggestions.map(\.title) == facts.suggestions.map(\.title))
}
