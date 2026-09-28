// What a hovered prompt's actions do (T37.23.9, DT§5.2): Copy puts the prompt's text on the
// pasteboard, Edit and resend puts it in the session's composer draft. Here because the
// transcript's actions meet the composer's store only in this package; CoxUI draws the strip
// and CoxTranscriptText shows it on hover.

import AppKit
import CoxClient
import CoxModel
import CoxTranscriptText
import CoxUI
import SwiftUI

extension TranscriptView {
  /// The transcript with a prompt's Edit and resend filling `composer`'s draft; without one a
  /// prompt offers Copy only.
  public func composer(_ composer: ComposerStore?) -> Self {
    var view = self
    view.composer = composer
    return view
  }

  /// Gives `text`'s hovered prompts their actions, reaching this view's composer; again on
  /// every update, so a composer given later is the one Edit and resend fills.
  func offerPromptActions(on text: TranscriptTextView, _ shared: SharedAppearance) {
    let acting = PromptActing(composer: composer)
    text.cards.promptActions = { block in
      AnyView(CardAppearance(shared: shared) { acting.actions(for: block) })
    }
  }
}

/// A prompt's actions and where they reach.
@MainActor
struct PromptActing {
  let composer: ComposerStore?
  /// Where Copy writes; tests pass a private one.
  var pasteboard = NSPasteboard.general

  /// What a hovered prompt offers: Edit and resend only with a composer to fill.
  var offered: [PromptActions.Action] { composer == nil ? [.copy] : PromptActions.Action.allCases }

  func actions(for block: Block) -> PromptActions {
    PromptActions(offered) { perform($0, on: block) }
  }

  /// Copy gives the prompt as shown, without its tiles (T37.23.4); Edit and resend replaces the
  /// draft with it, to change and send again.
  func perform(_ action: PromptActions.Action, on block: Block) {
    guard case .user(let text, _) = block.kind else { return }
    switch action {
    case .copy:
      pasteboard.clearContents()
      pasteboard.setString(text, forType: .string)
    case .edit:
      composer?.edit(text)
    }
  }
}
