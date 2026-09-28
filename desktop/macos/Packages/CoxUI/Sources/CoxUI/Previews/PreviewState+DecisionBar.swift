// `PreviewState` fixtures for `DecisionBar` (T37.27.5): the mockup's pinned push, a question whose
// answers fit on the line, and the mockup's retry question, whose answers do not. Separate so
// the organism's fixtures do not edit the shared file.

import SwiftUI

extension PreviewState {
  /// The mockup's `03-approval-required` bar.
  static let decisionApproval = DecisionBar.Content.approval("git push -u origin wt/retry-jitter")

  static let decisionQuestion = DecisionBar.Content.question(
    "Keep the sleep as a fallback?", options: ["Keep it", "Drop it"])

  /// The `04-question-ask-user` question: its answers are sentences, so the bar shows none.
  static let decisionLongQuestion = DecisionBar.Content.question(
    questionOptions.question, options: questionOptions.options)
}

/// A pinned bar across the reading column.
struct DecisionBarSample: View {
  let content: DecisionBar.Content

  init(_ content: DecisionBar.Content) { self.content = content }

  var body: some View { DecisionBar(content) { _ in }.frame(width: Size.readingWidth) }
}
