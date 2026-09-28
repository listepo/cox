// The `Composer` organism's and `CompletionList`'s check (T37.24, DS§6.3–§6.4): a snapshot per
// state × light/dark × Solid/Frosted — empty, a file mentioned with more files offered,
// commands offered, and a shell line with a prompt queued.

import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct ComposerSnapshotTests {
  @Test(arguments: Variant.all) func composer(_ variant: Variant) throws {
    let states = [
      ("empty", PreviewState.composerEmpty), ("mention", PreviewState.composerMention),
      ("commands", PreviewState.composerCommands), ("shell-queued", PreviewState.composerShell),
    ]
    for (look, state) in states {
      try assertCoxSnapshot(
        ComposerSample(state: state).fixedSize(), variant, named: "\(look).\(variant.name)")
    }
  }

  @Test(arguments: Variant.all) func completionList(_ variant: Variant) throws {
    let lists = [
      ("files", PreviewState.fileCompletion), ("commands", PreviewState.commandCompletion),
    ]
    for (look, list) in lists {
      try assertCoxSnapshot(
        PreviewPane { CompletionList(state: list) { _ in }.fixedSize() }, variant,
        named: "\(look).\(variant.name)")
    }
  }
}
