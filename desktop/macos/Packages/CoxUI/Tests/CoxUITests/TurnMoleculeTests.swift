// The turn molecules' check (T37.21.5, T37.21.6, DS§6.3): a snapshot per variant × light/dark
// × Solid/Frosted, each molecule on a pane from `PreviewState` as its `#Preview` shows it.

import SwiftUI
import Testing

@testable import CoxUI

@MainActor
@Suite struct TurnMoleculeSnapshotTests {
  @Test(arguments: Variant.all) func userBubble(_ variant: Variant) throws {
    try check(UserBubbleSample(hasAttachments: false), variant, "text")
    try check(UserBubbleSample(hasAttachments: true), variant, "attachments")
  }

  @Test(arguments: Variant.all) func thinkingDisclosure(_ variant: Variant) throws {
    try check(ThinkingDisclosureSample(isExpanded: false), variant, "collapsed")
    try check(ThinkingDisclosureSample(isExpanded: true), variant, "expanded")
  }

  /// One image per molecule variant, named `<variant>.<cell>`, at its ideal size (see
  /// `ShellMoleculeSnapshotTests.check`).
  private func check(
    _ molecule: some View, _ variant: Variant, _ look: String, test: String = #function
  ) throws {
    try assertCoxSnapshot(
      PreviewPane { molecule.fixedSize() }, variant, named: "\(look).\(variant.name)",
      testName: test)
  }
}
