// `PreviewState` fixtures for the turn molecules (T37.21.5, T37.21.6): a user prompt with its
// attachments, a thinking block, the notices, the divider and the meta line, as mockup screen
// 1 shows a turn. Separate from `PreviewState.swift` so molecules built in parallel add their
// fixtures without editing one file.

import SwiftUI

extension PreviewState {
  static let userPrompt =
    "Add jitter to the retry backoff in the provider HTTP client. Full jitter, capped at 30 s. "
    + "Keep the existing tests green and add one for the cap."
  /// A pasted screenshot and a log file.
  @MainActor static let userAttachments: [UserBubble.Attachment] = [
    .init(id: "a1", name: imageName, image: screenshot),
    .init(id: "a2", name: fileName),
  ]

  static let thinkingSummary = "Thought for 12 s"
  static let thinkingText =
    "The delay doubles with no ceiling, so attempt 16 waits over an hour. Cap it first, then "
    + "draw uniformly below the cap so clients that failed together retry apart."
}

/// A prompt, with or without attachments, in a narrow column so it wraps.
struct UserBubbleSample: View {
  let hasAttachments: Bool

  var body: some View {
    UserBubble(
      PreviewState.userPrompt, attachments: hasAttachments ? PreviewState.userAttachments : []
    )
    .frame(width: Size.popoverWidth)
  }
}

/// The thinking block, closed or open, in a narrow column so the reasoning wraps.
struct ThinkingDisclosureSample: View {
  let isExpanded: Bool

  var body: some View {
    ThinkingDisclosure(
      PreviewState.thinkingSummary, text: PreviewState.thinkingText, isExpanded: isExpanded
    )
    .frame(width: Size.popoverWidth, alignment: .leading)
  }
}
