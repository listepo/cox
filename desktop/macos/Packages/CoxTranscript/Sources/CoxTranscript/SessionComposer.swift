// `SessionComposer` (DT§5.3, DS§6.4 row `Composer`): CoxUI's `Composer` over a session's
// `ComposerStore` — the store's draft and rows copied into the organism's value, and each of its
// intents handed to the store. Here, beside `TranscriptView`, because this package is where
// CoxUI and CoxModel meet; CoxUI stays free of the stores and the store free of views.

import CoxClient
import CoxModel
import CoxUI
import Foundation
import SwiftUI

/// The composer under a session's transcript.
public struct SessionComposer: View {
  let store: ComposerStore

  public init(store: ComposerStore) {
    self.store = store
  }

  public var body: some View {
    Composer(state: state, send: handle)
  }

  private var state: Composer.State {
    var state = Composer.State()
    state.text = store.text
    state.isShell = store.isShell
    state.shareOutput = store.shareOutput
    state.mentions = store.mentions.map {
      Composer.Mention(
        id: $0, label: URL(fileURLWithPath: String($0.dropFirst())).lastPathComponent)
    }
    if let first = store.completions.first {
      state.completion = CompletionList.State(
        title: first.insert.hasPrefix("@") ? "Files" : "Commands",
        rows: store.completions.map {
          CompletionList.Row(id: $0.insert, title: $0.insert, detail: $0.detail)
        },
        selection: store.selection)
    }
    state.canSend = store.canSend
    return state
  }

  private func handle(_ intent: Composer.Intent) {
    switch intent {
    case .edit(let text): store.edit(text)
    case .submit, .submitNow: Task { await store.submit() }
    case .moveSelection(let step): store.moveSelection(by: step)
    case .pick(let index): store.pick(index)
    case .dismissCompletion: store.dismissCompletion()
    case .removeMention(let insert): store.removeMention(insert)
    case .leaveShell: store.leaveShell()
    case .shareOutput(let share): store.shareOutput = share
    }
  }
}
