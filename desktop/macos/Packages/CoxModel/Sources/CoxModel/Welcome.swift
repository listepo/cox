// What an empty session's welcome hero shows (Figma frame 22-empty-session): one line about the
// workspace and the suggestions that fill the composer. `WelcomeService` is the seam the app
// reads them through; until T37.49 gives cox-app a call for them, `MockWelcomeService` returns
// the frame's own values. The app copies them into CoxUI's `WelcomeHero.State`.

import CoxClient

/// One suggestion card: its title, the line under it, and the prompt a click drafts.
public struct WelcomeSuggestion: Equatable, Sendable {
  public var title: String
  public var detail: String
  public var prompt: String

  public init(title: String, detail: String, prompt: String) {
    (self.title, self.detail, self.prompt) = (title, detail, prompt)
  }
}

/// The hero's facts about one project.
public struct WelcomeFacts: Equatable, Sendable {
  /// `Rust workspace · 31 crates · AGENTS.md loaded`.
  public var summary: String
  public var suggestions: [WelcomeSuggestion]

  public init(summary: String = "", suggestions: [WelcomeSuggestion] = []) {
    (self.summary, self.suggestions) = (summary, suggestions)
  }
}

/// Where the hero's facts come from.
public protocol WelcomeService: Sendable {
  /// The facts for the session's working folder.
  func welcome(cwd: String) async throws -> WelcomeFacts
}

// MOCK: frame 22's summary and suggestions for every folder; T37.49 replaces this with the
// workspace facts and suggestions cox-app reads from the folder.
public struct MockWelcomeService: WelcomeService {
  public init() {}

  public func welcome(cwd: String) async throws -> WelcomeFacts {
    WelcomeFacts(
      summary: "Rust workspace · 31 crates · AGENTS.md loaded",
      suggestions: [
        WelcomeSuggestion(
          title: "Explain the architecture",
          detail: "How do cox-core, cox-protocol and the surfaces fit together?",
          prompt:
            "Explain the architecture: how do cox-core, cox-protocol and the surfaces fit together?"
        ),
        WelcomeSuggestion(
          title: "Find and fix a failing test",
          detail: "Run cargo nextest and fix the first failure",
          prompt: "Run cargo nextest and fix the first failure."),
        WelcomeSuggestion(
          title: "Review my uncommitted diff",
          detail: "Check git diff for bugs before I commit",
          prompt: "Review my uncommitted diff: check git diff for bugs before I commit."),
      ])
  }
}

extension ComposerStore {
  /// A welcome suggestion's click: its prompt becomes the draft, to read and send.
  public func suggest(_ suggestion: WelcomeSuggestion) { edit(suggestion.prompt) }
}
